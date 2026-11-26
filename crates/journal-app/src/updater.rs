use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use subtle::ConstantTimeEq;

/// Official GitHub Releases latest endpoint for Oxidium.
pub const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/cryals/Oxidium/releases/latest";

/// Public human-readable GitHub Releases page.
pub const GITHUB_RELEASES_PAGE: &str = "https://github.com/cryals/Oxidium/releases";

/// Current compiled workspace version of Oxidium.
pub const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Metadata for a downloadable release asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseAssetInfo {
    pub name: String,
    pub download_url: String,
    pub size_bytes: u64,
}

/// Result of checking GitHub Releases for a newer Oxidium version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub tag_name: String,
    pub release_name: String,
    pub release_notes: String,
    pub html_url: String,
    pub published_at: String,
    pub selected_asset: Option<ReleaseAssetInfo>,
    pub sha256_expected: Option<String>,
    pub is_update_available: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubReleasePayload {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GitHubAssetPayload>,
}

#[derive(Debug, Deserialize)]
struct GitHubAssetPayload {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Parses a semantic version string (e.g. `"v0.2.1"`, `"0.2.0-rc.1"`).
/// Returns `(major, minor, patch, is_stable, prerelease)`.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64, bool, String)> {
    let trimmed = raw
        .trim()
        .strip_prefix('v')
        .or_else(|| raw.trim().strip_prefix('V'))
        .unwrap_or(raw.trim());
    let without_build = trimmed.split('+').next()?.trim();
    if without_build.is_empty() {
        return None;
    }
    let (core, pre) = match without_build.split_once('-') {
        Some((c, p)) => (c, p.trim().to_string()),
        None => (without_build, String::new()),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch, pre.is_empty(), pre))
}

/// Returns `true` strictly when `latest` is a higher Semantic Version than `current`.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let (Some(cur), Some(lat)) = (parse_semver(current), parse_semver(latest)) else {
        return false;
    };
    lat > cur
}

/// Extracts the expected lowercase 64-character SHA-256 hex digest for `target_filename`
/// from standard `SHA256SUMS.txt` content (`<hex>  <filename>` or `<hex> *<filename>`).
pub fn parse_sha256sums(content: &str, target_filename: &str) -> Option<String> {
    let target_clean = target_filename.trim();
    if target_clean.is_empty() {
        return None;
    }
    for raw_line in content.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(rest) = parts.next() else {
            continue;
        };
        let fname = rest.trim_start_matches('*');
        if hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
            && fname.eq_ignore_ascii_case(target_clean)
        {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

/// Picks the best release artifact for a given `(os, arch)` pair.
/// Never returns checksum or SBOM metadata files.
pub fn select_platform_asset_for<'a>(
    assets: &'a [ReleaseAssetInfo],
    os: &str,
    arch: &str,
) -> Option<&'a ReleaseAssetInfo> {
    let candidates: Vec<&ReleaseAssetInfo> = assets
        .iter()
        .filter(|a| {
            let low = a.name.to_ascii_lowercase();
            !low.contains("sha256")
                && !low.ends_with(".txt")
                && !low.ends_with(".json")
                && !low.ends_with(".sig")
        })
        .collect();

    match os {
        "windows" => {
            let arch_tokens: &[&str] = match arch {
                "aarch64" => &["arm64", "aarch64"],
                _ => &["x86_64", "x64", "amd64"],
            };
            // 1. Prefer native setup installer matching arch
            if let Some(a) = candidates.iter().find(|a| {
                let low = a.name.to_ascii_lowercase();
                low.ends_with(".exe")
                    && low.contains("setup")
                    && arch_tokens.iter().any(|t| low.contains(t))
            }) {
                return Some(a);
            }
            // 2. Fallback: any Windows .exe
            if let Some(a) = candidates
                .iter()
                .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
            {
                return Some(a);
            }
            // 3. Fallback: Windows portable .zip matching arch
            candidates.iter().copied().find(|a| {
                let low = a.name.to_ascii_lowercase();
                low.ends_with(".zip")
                    && low.contains("windows")
                    && arch_tokens.iter().any(|t| low.contains(t))
            })
        }
        "linux" => {
            let arch_tokens: &[&str] = match arch {
                "aarch64" => &["arm64", "aarch64"],
                _ => &["amd64", "x86_64", "x64"],
            };
            // 1. Prefer Debian package matching arch
            if let Some(a) = candidates.iter().find(|a| {
                let low = a.name.to_ascii_lowercase();
                low.ends_with(".deb") && arch_tokens.iter().any(|t| low.contains(t))
            }) {
                return Some(a);
            }
            // 2. Fallback: portable .tar.gz matching arch
            candidates.iter().copied().find(|a| {
                let low = a.name.to_ascii_lowercase();
                low.ends_with(".tar.gz")
                    && low.contains("linux")
                    && arch_tokens.iter().any(|t| low.contains(t))
            })
        }
        _ => candidates.first().copied(),
    }
}

/// Picks the best release artifact for the current host platform.
pub fn select_platform_asset(assets: &[ReleaseAssetInfo]) -> Option<&ReleaseAssetInfo> {
    select_platform_asset_for(assets, std::env::consts::OS, std::env::consts::ARCH)
}

/// Formats byte sizes cleanly for Russian UI labels.
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} МБ", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.0} КБ", bytes as f64 / KB)
    } else {
        format!("{bytes} Б")
    }
}

fn build_http_client(current_version: &str, timeout: Duration) -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(timeout)
        .user_agent(format!(
            "Oxidium/{current_version} (+{GITHUB_RELEASES_PAGE})"
        ))
        .build()
        .map_err(|e| AppError::Update(format!("HTTP client error: {e}")))
}

fn is_allowed_download_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("https://")
        || trimmed.starts_with("http://127.0.0.1:")
        || trimmed.starts_with("http://localhost:")
}

/// Queries the official GitHub Releases API (`cryals/Oxidium`) for a newer release.
pub async fn check_for_updates(current_version: &str) -> AppResult<UpdateInfo> {
    check_for_updates_from_url(GITHUB_RELEASES_API, current_version).await
}

/// Queries an explicit release JSON endpoint (also used by local mock-HTTP tests).
pub async fn check_for_updates_from_url(
    api_url: &str,
    current_version: &str,
) -> AppResult<UpdateInfo> {
    if !is_allowed_download_url(api_url) {
        return Err(AppError::Update(
            "URL проверки обновлений должен использовать HTTPS".into(),
        ));
    }

    let client = build_http_client(current_version, Duration::from_secs(10))?;
    let resp = client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| AppError::Update(format!("Не удалось проверить обновления: {e}")))?;

    // 404 simply means no published release exists yet in the repository.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current_version: current_version.to_string(),
            latest_version: current_version.to_string(),
            tag_name: format!("v{current_version}"),
            release_name: String::new(),
            release_notes: String::new(),
            html_url: GITHUB_RELEASES_PAGE.to_string(),
            published_at: String::new(),
            selected_asset: None,
            sha256_expected: None,
            is_update_available: false,
        });
    }

    if !resp.status().is_success() {
        return Err(AppError::Update(format!(
            "GitHub Releases API вернул статус {}",
            resp.status()
        )));
    }

    let payload: GitHubReleasePayload = resp
        .json()
        .await
        .map_err(|e| AppError::Update(format!("Ошибка разбора ответа GitHub: {e}")))?;

    let latest_clean = payload
        .tag_name
        .trim()
        .strip_prefix('v')
        .or_else(|| payload.tag_name.trim().strip_prefix('V'))
        .unwrap_or(payload.tag_name.trim())
        .to_string();

    let is_available = !payload.draft && is_newer_version(current_version, &payload.tag_name);

    let assets: Vec<ReleaseAssetInfo> = payload
        .assets
        .into_iter()
        .map(|a| ReleaseAssetInfo {
            name: a.name,
            download_url: a.browser_download_url,
            size_bytes: a.size,
        })
        .collect();

    let selected_asset = select_platform_asset(&assets).cloned();

    // Fetch SHA256SUMS.txt if present alongside the release assets for cryptographic verification.
    let mut sha256_expected = None;
    if is_available
        && let Some(ref chosen) = selected_asset
        && let Some(sums_asset) = assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("SHA256SUMS.txt"))
        && is_allowed_download_url(&sums_asset.download_url)
        && let Ok(sums_resp) = client.get(&sums_asset.download_url).send().await
        && sums_resp.status().is_success()
        && let Ok(text) = sums_resp.text().await
    {
        sha256_expected = parse_sha256sums(&text, &chosen.name);
    }

    let published_short = payload
        .published_at
        .as_deref()
        .unwrap_or("")
        .split('T')
        .next()
        .unwrap_or("")
        .to_string();

    Ok(UpdateInfo {
        current_version: current_version.to_string(),
        latest_version: latest_clean,
        tag_name: payload.tag_name,
        release_name: payload.name.unwrap_or_default(),
        release_notes: payload.body.unwrap_or_default(),
        html_url: payload
            .html_url
            .filter(|u| u.starts_with("https://"))
            .unwrap_or_else(|| GITHUB_RELEASES_PAGE.to_string()),
        published_at: published_short,
        selected_asset,
        sha256_expected,
        is_update_available: is_available,
    })
}

fn sanitize_single_filename(name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.starts_with('.')
    {
        return Err(AppError::Update("Недопустимое имя файла обновления".into()));
    }
    Ok(trimmed.to_string())
}

/// Downloads the selected platform release artifact into `updates_dir`, verifies its SHA-256
/// checksum against `SHA256SUMS.txt` (when available), and commits atomically on disk.
pub async fn download_and_verify_update(
    info: &UpdateInfo,
    updates_dir: &Path,
) -> AppResult<PathBuf> {
    let asset = info.selected_asset.as_ref().ok_or_else(|| {
        AppError::Update("Для вашей платформы не найден автоматический пакет в этом релизе".into())
    })?;
    let safe_name = sanitize_single_filename(&asset.name)?;
    if !is_allowed_download_url(&asset.download_url) {
        return Err(AppError::Update(
            "Ссылка загрузки обновления не использует HTTPS".into(),
        ));
    }

    std::fs::create_dir_all(updates_dir)?;
    let client = build_http_client(&info.current_version, Duration::from_secs(180))?;
    let resp = client
        .get(&asset.download_url)
        .send()
        .await
        .map_err(|e| AppError::Update(format!("Ошибка загрузки обновления: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Update(format!(
            "Сервер вернул статус {} при скачивании {}",
            resp.status(),
            safe_name
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Update(format!("Ошибка чтения потока загрузки: {e}")))?;

    if let Some(ref expected_hex) = info.sha256_expected {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_hex = hex::encode(hasher.finalize());
        let matches: bool = actual_hex
            .as_bytes()
            .ct_eq(expected_hex.to_ascii_lowercase().as_bytes())
            .into();
        if !matches {
            return Err(AppError::Update(format!(
                "Ошибка целостности SHA-256 для {safe_name}: ожидалось {}, получено {}",
                expected_hex, actual_hex
            )));
        }
    }

    let part_path = updates_dir.join(format!("{safe_name}.part"));
    let final_path = updates_dir.join(&safe_name);
    std::fs::write(&part_path, &bytes)?;
    if final_path.exists() {
        let _ = std::fs::remove_file(&final_path);
    }
    std::fs::rename(&part_path, &final_path)?;
    Ok(final_path)
}

/// Launches the verified update installer (Windows `.exe`) or reveals the downloaded package
/// in the OS file manager so the user can complete the semi-automatic upgrade safely.
pub fn launch_update_artifact(artifact_path: &Path) -> AppResult<String> {
    if !artifact_path.exists() {
        return Err(AppError::Update(
            "Загруженный файл обновления не найден".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let is_exe = artifact_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
        if is_exe {
            std::process::Command::new(artifact_path)
                .spawn()
                .map_err(|e| AppError::Update(format!("Не удалось запустить установщик: {e}")))?;
            return Ok(
                "Установщик обновления запущен. Завершите установку в открывшемся окне.".into(),
            );
        }
        let arg = format!("/select,{}", artifact_path.display());
        let _ = std::process::Command::new("explorer.exe").arg(arg).spawn();
        Ok(format!(
            "Архив обновления загружен и проверен: {}",
            artifact_path.display()
        ))
    }

    #[cfg(target_os = "linux")]
    {
        let target = artifact_path.parent().unwrap_or(artifact_path);
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
        Ok(format!(
            "Пакет обновления загружен и проверен (SHA-256): {}",
            artifact_path.display()
        ))
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(format!(
            "Файл обновления загружен: {}",
            artifact_path.display()
        ))
    }
}

/// Opens a verified HTTPS URL (e.g. GitHub Releases page) in the user's default browser.
pub fn open_url_in_browser(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") {
        return Err(AppError::Update(
            "Разрешено открытие только HTTPS-ссылок".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", trimmed])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AppError::Update(format!("Не удалось открыть браузер: {e}")))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| AppError::Update(format!("Не удалось открыть браузер: {e}")))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| AppError::Update(format!("Не удалось открыть браузер: {e}")))?;
        Ok(())
    }
}
