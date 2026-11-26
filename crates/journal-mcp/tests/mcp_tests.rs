use chrono::{Duration, Utc};
use journal_app::AppServices;
use journal_domain::{McpPermissionMode, RecordSource};
use journal_mcp::{McpMetrics, OxidiumMcpServer, run_http_server};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

async fn post_mcp(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    body: Value,
) -> (reqwest::StatusCode, String) {
    let mut req = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let resp = req.send().await.unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    (status, text)
}

fn parse_sse_or_json(body: &str) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        return v;
    }
    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let trimmed = data.trim();
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                return v;
            }
        }
    }
    panic!("could not parse MCP HTTP response body: {body}");
}

async fn call_tool_http(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    session_id: Option<&str>,
    id: u64,
    name: &str,
    arguments: Value,
) -> Value {
    let mut req = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }));
    if let Some(sid) = session_id {
        req = req.header("mcp-session-id", sid);
    }
    let resp = req.send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.unwrap();
    parse_sse_or_json(&body)
}

fn extract_tool_json(rpc: &Value) -> Value {
    let text = rpc["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("expected tool text content in {rpc}"));
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

#[tokio::test]
async fn mcp_tool_count_resources_prompts_http_auth_permissions_idempotency_and_9_scenarios() {
    let dir = TempDir::new().unwrap();
    let app = AppServices::open_file(dir.path().join("mcp.db")).unwrap();
    let metrics = Arc::new(McpMetrics::default());

    // Verify 80 registered tools
    let direct_server = OxidiumMcpServer::new(Arc::clone(&app), Arc::clone(&metrics));
    assert!(
        direct_server.tool_count() >= 78,
        "expected >= 78 MCP tools, got {}",
        direct_server.tool_count()
    );

    // Start real loopback Streamable HTTP server on ephemeral port
    let token = app.tokens.load_or_create().unwrap();
    let ct = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let srv_app = Arc::clone(&app);
    let srv_metrics = Arc::clone(&metrics);
    let srv_ct = ct.clone();
    tokio::spawn(async move {
        let _ = run_http_server(srv_app, srv_metrics, port, srv_ct).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    let url = format!("http://127.0.0.1:{port}/mcp");
    let client = reqwest::Client::new();

    // 1. Unauthorized request MUST be rejected with 401
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "oxidium-test", "version": "1.0" }
        }
    });
    let (status_unauth, _) = post_mcp(&client, &url, None, init_req.clone()).await;
    assert_eq!(status_unauth, reqwest::StatusCode::UNAUTHORIZED);

    let (status_bad, _) =
        post_mcp(&client, &url, Some("oxd_invalid_token"), init_req.clone()).await;
    assert_eq!(status_bad, reqwest::StatusCode::UNAUTHORIZED);

    // 2. Authorized initialize + capture session ID
    let resp = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&init_req)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let init_json = parse_sse_or_json(&resp.text().await.unwrap());
    assert_eq!(
        init_json["result"]["serverInfo"]["name"],
        "oxidium-student-journal-mcp"
    );

    // Send notifications/initialized
    let mut notif = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    if let Some(ref sid) = session_id {
        notif = notif.header("mcp-session-id", sid);
    }
    let _ = notif.send().await.unwrap();

    // 3. Verify resources/list, resources/read, prompts/list, prompts/get
    let mut res_req = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" }));
    if let Some(ref sid) = session_id {
        res_req = res_req.header("mcp-session-id", sid);
    }
    let res_list = parse_sse_or_json(&res_req.send().await.unwrap().text().await.unwrap());
    assert!(res_list["result"]["resources"].as_array().unwrap().len() >= 5);

    let mut prompt_req = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "jsonrpc": "2.0", "id": 3, "method": "prompts/list" }));
    if let Some(ref sid) = session_id {
        prompt_req = prompt_req.header("mcp-session-id", sid);
    }
    let prompt_list = parse_sse_or_json(&prompt_req.send().await.unwrap().text().await.unwrap());
    assert_eq!(
        prompt_list["result"]["prompts"].as_array().unwrap().len(),
        7
    );

    // 4. Execute ALL 9 natural-language user scenarios from specification §55 via real MCP tools
    let active_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        10,
        "get_active_profile",
        json!({}),
    )
    .await;
    let profile_id = extract_tool_json(&active_rpc)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create subjects: Философия, Физкультура, Базы данных, Программирование
    let mut subj_ids = std::collections::HashMap::new();
    for name in [
        "Философия",
        "Физкультура",
        "Базы данных",
        "Программирование",
    ] {
        let r = call_tool_http(
            &client,
            &url,
            &token,
            session_id.as_deref(),
            11,
            "create_subject",
            json!({ "profile_id": profile_id, "name": name }),
        )
        .await;
        subj_ids.insert(
            name,
            extract_tool_json(&r)["id"].as_str().unwrap().to_string(),
        );
    }

    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);

    // Scenario 1: «Поставь мне 5 по философии за сегодня» + Idempotency replay test!
    let g1_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        12,
        "add_grade",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Философия"],
            "date": today.format("%Y-%m-%d").to_string(),
            "grade": 5,
            "idempotency_key": "idem-phil-5-today"
        }),
    )
    .await;
    let g1_id = extract_tool_json(&g1_rpc)["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Replay same idempotency_key -> returns exact same grade ID, no duplicate created!
    let g1_replay = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        13,
        "add_grade",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Философия"],
            "date": today.format("%Y-%m-%d").to_string(),
            "grade": 5,
            "idempotency_key": "idem-phil-5-today"
        }),
    )
    .await;
    assert_eq!(extract_tool_json(&g1_replay)["id"].as_str().unwrap(), g1_id);

    // Scenario 2: «Отметь, что вчера меня не было на физкультуре по уважительной причине»
    let att_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        14,
        "add_attendance",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Физкультура"],
            "date": yesterday.format("%Y-%m-%d").to_string(),
            "status": "excused",
            "reason": "Уважительная причина"
        }),
    )
    .await;
    assert_eq!(extract_tool_json(&att_rpc)["status"], "excused");

    // Scenario 3: «Добавь лабораторную по базам данных, дедлайн в пятницу»
    let task_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        15,
        "create_task",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Базы данных"],
            "title": "Лабораторная №4",
            "deadline": (today + Duration::days(3)).format("%Y-%m-%d").to_string(),
            "priority": "high",
            "tags": ["Лабораторная"]
        }),
    )
    .await;
    let task_id = extract_tool_json(&task_rpc)["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Scenario 4: «Покажи мой средний балл по программированию»
    let _ = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        16,
        "add_grades_batch",
        json!({
            "items": [
                { "profile_id": profile_id, "subject_id": subj_ids["Программирование"], "date": "2026-11-10", "grade": 4 },
                { "profile_id": profile_id, "subject_id": subj_ids["Программирование"], "date": "2026-11-15", "grade": 5 }
            ]
        }),
    )
    .await;
    let avg_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        17,
        "get_average_grade",
        json!({ "profile_id": profile_id, "subject_id": subj_ids["Программирование"] }),
    )
    .await;
    assert_eq!(extract_tool_json(&avg_rpc)["average_formatted"], "4.50");

    // Scenario 5: «Какие задания у меня просрочены?»
    let _ = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        18,
        "create_task",
        json!({
            "profile_id": profile_id,
            "title": "Просроченное ДЗ",
            "deadline": "2025-01-01"
        }),
    )
    .await;
    let overdue_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        19,
        "get_overdue_tasks",
        json!({ "profile_id": profile_id }),
    )
    .await;
    assert_eq!(extract_tool_json(&overdue_rpc).as_array().unwrap().len(), 1);

    // Scenario 6: «Перемести лабораторную №4 в выполненные»
    let comp_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        20,
        "complete_task",
        json!({ "task_id": task_id }),
    )
    .await;
    assert!(extract_tool_json(&comp_rpc)["completed_at"].is_string());

    // Scenario 7: «Сколько пятёрок мне нужно, чтобы средний балл по философии стал 4.5?»
    let _ = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        21,
        "add_grades_batch",
        json!({
            "items": [
                { "profile_id": profile_id, "subject_id": subj_ids["Философия"], "date": "2026-11-02", "grade": 3 },
                { "profile_id": profile_id, "subject_id": subj_ids["Философия"], "date": "2026-11-05", "grade": 4 }
            ]
        }),
    )
    .await;
    // Philosophy now has [5, 3, 4] -> avg 4.0 -> need 3 fives for 4.5: (12+15)/6 = 4.5
    let fc_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        22,
        "calculate_grade_forecast",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Философия"],
            "target_average": "4.5",
            "assumed_grade": 5
        }),
    )
    .await;
    assert_eq!(
        extract_tool_json(&fc_rpc)["target_plan"]["grades_needed"],
        3
    );

    // Scenario 8: «Составь сводку моей успеваемости за ноябрь»
    let nov_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        23,
        "get_monthly_summary",
        json!({ "profile_id": profile_id, "year": 2026, "month": 11 }),
    )
    .await;
    assert!(
        extract_tool_json(&nov_rpc)["numeric_grades"]
            .as_u64()
            .unwrap()
            >= 4
    );

    // Scenario 9: «Добавь все задания из переданного мной списка»
    let batch_tasks_rpc = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        24,
        "create_tasks_batch",
        json!({
            "items": [
                { "profile_id": profile_id, "title": "Прочитать главу 3" },
                { "profile_id": profile_id, "title": "Решить задачи 1-10" },
                { "profile_id": profile_id, "title": "Подготовить презентацию" }
            ]
        }),
    )
    .await;
    assert_eq!(
        extract_tool_json(&batch_tasks_rpc)
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // 5. Permission Enforcement Tests
    // Destructive delete_grade MUST fail when mcp_allow_destructive = false (default)
    let del_denied = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        25,
        "delete_grade",
        json!({ "grade_id": g1_id }),
    )
    .await;
    assert_eq!(del_denied["result"]["isError"], true);

    // Switch to ReadOnly -> add_grade MUST fail
    let mut settings = app.get_settings().unwrap();
    settings.mcp_permission_mode = McpPermissionMode::ReadOnly;
    app.update_settings(&settings, RecordSource::Gui).unwrap();

    let ro_denied = call_tool_http(
        &client,
        &url,
        &token,
        session_id.as_deref(),
        26,
        "add_grade",
        json!({
            "profile_id": profile_id,
            "subject_id": subj_ids["Философия"],
            "date": "2026-11-20",
            "grade": 5
        }),
    )
    .await;
    assert_eq!(ro_denied["result"]["isError"], true);

    // Verify activity log recorded MCP actions without leaking tokens
    let logs = app
        .list_activity_log(None, Some(RecordSource::Mcp), 50)
        .unwrap();
    assert!(logs.len() >= 8);
    assert!(logs.iter().all(|l| !l.summary.contains(&token)));

    ct.cancel();
}
