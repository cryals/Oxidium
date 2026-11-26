use journal_app::AppServices;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn locate_mcp_binary() -> PathBuf {
    // Workspace target/debug/student-journal-mcp(.exe)
    let exe_name = if cfg!(windows) {
        "student-journal-mcp.exe"
    } else {
        "student-journal-mcp"
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest.parent().unwrap().parent().unwrap();
    let candidate = workspace_root.join("target").join("debug").join(exe_name);
    assert!(
        candidate.exists(),
        "student-journal-mcp binary not found at {}; run `cargo build -p student-journal-mcp` first",
        candidate.display()
    );
    candidate
}

#[test]
fn stdio_child_process_protocol_clean_stdout_and_cross_process_sync() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("stdio_sync.db");

    // Simulate open GUI process connected to the same SQLite database
    let gui_app = AppServices::open_file(&db_path).unwrap();
    let initial_rev = gui_app.sync.current_revision();

    // Spawn real standalone student-journal-mcp process over STDIO
    let bin = locate_mcp_binary();
    let mut child = Command::new(bin)
        .args(["--stdio", "--db", db_path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn student-journal-mcp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let send_and_recv = |stdin: &mut std::process::ChildStdin,
                         reader: &mut BufReader<std::process::ChildStdout>,
                         msg: Value|
     -> Value {
        let line = serde_json::to_string(&msg).unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).unwrap();
        // Verify stdout is STRICTLY valid JSON-RPC (zero log contamination!)
        serde_json::from_str(&resp_line)
            .unwrap_or_else(|e| panic!("stdout polluted with non-JSON line '{resp_line}': {e}"))
    };

    // 1. initialize
    let init = send_and_recv(
        &mut stdin,
        &mut reader,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "stdio-test", "version": "1.0" }
            }
        }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "oxidium-student-journal-mcp");

    // Send notifications/initialized
    writeln!(
        stdin,
        "{}",
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
    )
    .unwrap();
    stdin.flush().unwrap();

    // 2. tools/list
    let tools = send_and_recv(
        &mut stdin,
        &mut reader,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    );
    assert!(tools["result"]["tools"].as_array().unwrap().len() >= 78);

    // 3. Create subject + add grade via external STDIO MCP process
    let active_profile = gui_app.get_active_profile().unwrap();
    let subj_resp = send_and_recv(
        &mut stdin,
        &mut reader,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "create_subject",
                "arguments": {
                    "profile_id": active_profile.id.to_string(),
                    "name": "Алгоритмы и структуры данных"
                }
            }
        }),
    );
    let subj_json: Value =
        serde_json::from_str(subj_resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let subj_id = subj_json["id"].as_str().unwrap();

    let _grade_resp = send_and_recv(
        &mut stdin,
        &mut reader,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "add_grade",
                "arguments": {
                    "profile_id": active_profile.id.to_string(),
                    "subject_id": subj_id,
                    "date": "2026-11-26",
                    "grade": 5,
                    "topic": "Красно-чёрные деревья"
                }
            }
        }),
    );

    // Close stdin so child exits gracefully
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "student-journal-mcp exited cleanly");

    // 4. Verify GUI process detects cross-process change via SyncCoordinator without restarting!
    let sync_event = gui_app.sync.poll_external_changes();
    assert!(
        sync_event.is_some(),
        "GUI SyncCoordinator must detect external STDIO MCP write"
    );
    assert!(sync_event.unwrap().revision > initial_rev);

    let subjects_seen_by_gui = gui_app.list_subjects(active_profile.id, false).unwrap();
    assert_eq!(subjects_seen_by_gui.len(), 1);
    assert_eq!(subjects_seen_by_gui[0].name, "Алгоритмы и структуры данных");
}
