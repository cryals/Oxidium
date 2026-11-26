use axum::{Router, http::StatusCode, routing::get};
use journal_app::updater::{
    ReleaseAssetInfo, check_for_updates_from_url, download_and_verify_update, is_newer_version,
    parse_sha256sums, select_platform_asset_for,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn semver_comparison_handles_v_prefix_and_prereleases() {
    assert!(is_newer_version("0.1.0", "0.2.0"));
    assert!(is_newer_version("0.1.0", "v0.1.1"));
    assert!(is_newer_version("0.1.0", "v1.0.0"));
    assert!(is_newer_version("0.2.0-rc.1", "0.2.0"));
    assert!(is_newer_version("0.1.0", "0.2.0-beta.1"));

    assert!(!is_newer_version("0.1.0", "0.1.0"));
    assert!(!is_newer_version("0.1.0", "v0.1.0"));
    assert!(!is_newer_version("0.2.0", "0.1.9"));
    assert!(!is_newer_version("0.2.0", "0.2.0-rc.1"));
    assert!(!is_newer_version("0.1.0", "invalid"));
}

#[test]
fn sha256sums_parser_and_platform_selector() {
    let sums = "\u{feff}e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  Oxidium-0.2.0-windows-x86_64-setup.exe\r\n\
                aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *oxidium_0.2.0_amd64.deb\n";

    assert_eq!(
        parse_sha256sums(sums, "Oxidium-0.2.0-windows-x86_64-setup.exe").as_deref(),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(
        parse_sha256sums(sums, "oxidium_0.2.0_amd64.deb").as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(parse_sha256sums(sums, "missing.zip"), None);

    let assets = vec![
        ReleaseAssetInfo {
            name: "SHA256SUMS.txt".into(),
            download_url: "https://example.invalid/SHA256SUMS.txt".into(),
            size_bytes: 120,
        },
        ReleaseAssetInfo {
            name: "Oxidium-0.2.0-windows-x86_64-portable.zip".into(),
            download_url: "https://example.invalid/portable.zip".into(),
            size_bytes: 1000,
        },
        ReleaseAssetInfo {
            name: "Oxidium-0.2.0-windows-x86_64-setup.exe".into(),
            download_url: "https://example.invalid/setup.exe".into(),
            size_bytes: 1200,
        },
        ReleaseAssetInfo {
            name: "oxidium_0.2.0_amd64.deb".into(),
            download_url: "https://example.invalid/amd64.deb".into(),
            size_bytes: 900,
        },
        ReleaseAssetInfo {
            name: "Oxidium-0.2.0-linux-arm64.tar.gz".into(),
            download_url: "https://example.invalid/arm64.tar.gz".into(),
            size_bytes: 850,
        },
    ];

    assert_eq!(
        select_platform_asset_for(&assets, "windows", "x86_64").map(|a| a.name.as_str()),
        Some("Oxidium-0.2.0-windows-x86_64-setup.exe")
    );
    assert_eq!(
        select_platform_asset_for(&assets, "linux", "x86_64").map(|a| a.name.as_str()),
        Some("oxidium_0.2.0_amd64.deb")
    );
    assert_eq!(
        select_platform_asset_for(&assets, "linux", "aarch64").map(|a| a.name.as_str()),
        Some("Oxidium-0.2.0-linux-arm64.tar.gz")
    );
}

#[tokio::test]
async fn updater_end_to_end_check_download_and_sha256_verification() {
    let payload_bytes: Vec<u8> = b"OXIDIUM-TEST-INSTALLER-PAYLOAD-V0.2.0".to_vec();
    let expected_sha = hex::encode(Sha256::digest(&payload_bytes));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let base = format!("http://{addr}");

    let asset_name = if cfg!(target_os = "windows") {
        "Oxidium-0.2.0-windows-x86_64-setup.exe"
    } else if cfg!(target_arch = "aarch64") {
        "oxidium_0.2.0_arm64.deb"
    } else {
        "oxidium_0.2.0_amd64.deb"
    };

    let release_json = serde_json::json!({
        "tag_name": "v0.2.0",
        "name": "Oxidium v0.2.0",
        "body": "### Новое в 0.2.0\n- Улучшенный журнал\n- Динамические обновления",
        "html_url": "https://github.com/cryals/Oxidium/releases/tag/v0.2.0",
        "published_at": "2026-12-01T12:00:00Z",
        "draft": false,
        "assets": [
            {
                "name": asset_name,
                "browser_download_url": format!("{base}/download/good"),
                "size": payload_bytes.len()
            },
            {
                "name": "SHA256SUMS.txt",
                "browser_download_url": format!("{base}/download/SHA256SUMS.txt"),
                "size": 120
            }
        ]
    });
    let sums_body = format!("{expected_sha}  {asset_name}\n");
    let good_bytes = payload_bytes.clone();

    let app = Router::new()
        .route(
            "/releases/latest",
            get({
                let j = release_json.clone();
                move || async move { axum::Json(j.clone()) }
            }),
        )
        .route(
            "/releases/empty",
            get(|| async { (StatusCode::NOT_FOUND, "Not Found") }),
        )
        .route(
            "/download/SHA256SUMS.txt",
            get({
                let s = sums_body.clone();
                move || async move { s.clone() }
            }),
        )
        .route(
            "/download/good",
            get({
                let b = good_bytes.clone();
                move || async move { b.clone() }
            }),
        )
        .route(
            "/download/tampered",
            get(|| async { b"CORRUPTED-BYTES".to_vec() }),
        );

    let srv = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // 1. Newer release detected (0.1.0 -> 0.2.0) with expected SHA-256
    let info = check_for_updates_from_url(&format!("{base}/releases/latest"), "0.1.0")
        .await
        .expect("check newer");
    assert!(info.is_update_available);
    assert_eq!(info.latest_version, "0.2.0");
    assert_eq!(info.published_at, "2026-12-01");
    assert_eq!(info.sha256_expected.as_deref(), Some(expected_sha.as_str()));
    assert_eq!(
        info.selected_asset.as_ref().map(|a| a.name.as_str()),
        Some(asset_name)
    );

    // 2. Download + valid SHA-256 succeeds and commits atomically
    let tmp = tempdir().expect("tempdir");
    let saved = download_and_verify_update(&info, tmp.path())
        .await
        .expect("download good");
    assert!(saved.exists());
    assert_eq!(std::fs::read(&saved).expect("read saved"), payload_bytes);

    // 3. Tampered payload fails SHA-256 check!
    let mut tampered = info.clone();
    tampered.selected_asset.as_mut().unwrap().download_url = format!("{base}/download/tampered");
    let err = download_and_verify_update(&tampered, tmp.path())
        .await
        .expect_err("tampered asset must be rejected");
    assert!(err.to_string().contains("SHA-256"));

    // 4. Same version reports no update available
    let same = check_for_updates_from_url(&format!("{base}/releases/latest"), "0.2.0")
        .await
        .expect("check same");
    assert!(!same.is_update_available);

    // 5. HTTP 404 (no published release yet) handled cleanly without error
    let empty = check_for_updates_from_url(&format!("{base}/releases/empty"), "0.1.0")
        .await
        .expect("404 should be graceful");
    assert!(!empty.is_update_available);

    srv.abort();
}
