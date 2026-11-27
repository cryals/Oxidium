use crate::AppServices;
use crate::error::{AppError, AppResult};
use crate::sync::SyncCoordinator;
use crate::sync_crypto::{
    InvitePayload, SyncKeyStore, VaultMasterKey, canonical_client_auth_message,
    canonical_pair_ack_message, canonical_pair_message, canonical_server_challenge_message,
    decrypt_snapshot_blob, encrypt_snapshot_blob, key_fingerprint, parse_pub_key, random_b64url,
    verify_signature,
};
use chrono::Utc;
use futures_util::StreamExt;
use journal_domain::{RecordSource, SyncConnectionStatus, SyncDeviceInfo, SyncRemoteConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
struct CachedSession {
    server_url: String,
    device_id: String,
    token: String,
    expires_at: i64,
}

#[derive(Default)]
pub struct RemoteSyncRuntime {
    session: Mutex<Option<CachedSession>>,
    sync_op_lock: Mutex<()>,
    wake_notify: Arc<Notify>,
}

impl RemoteSyncRuntime {
    pub fn notify_wake(&self) {
        self.wake_notify.notify_one();
    }
}

#[derive(Debug, Deserialize)]
struct NodeInfoResponse {
    protocol: String,
    node_pub: String,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct PairResponse {
    device_id: String,
    server_pub: String,
    server_fingerprint: String,
    timestamp: i64,
    server_sig: String,
}

#[derive(Debug, Deserialize)]
struct AuthChallengeResponse {
    challenge_id: String,
    server_nonce: String,
    expires_at: i64,
    server_sig: String,
}

#[derive(Debug, Deserialize)]
struct AuthVerifyResponse {
    token: String,
    expires_at: i64,
    device_id: String,
}

#[derive(Debug, Deserialize)]
struct VaultHeadResponse {
    seq: i64,
    #[allow(dead_code)]
    vault_id: Option<String>,
    #[allow(dead_code)]
    device_id: Option<String>,
    #[allow(dead_code)]
    device_name: Option<String>,
    #[allow(dead_code)]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushSnapshotResponse {
    seq: i64,
    #[allow(dead_code)]
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct NodeDeviceRow {
    id: String,
    user_id: String,
    name: String,
    public_key: String,
    fingerprint: String,
    created_at: String,
    last_seen_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NodeErrorPayload {
    #[allow(dead_code)]
    error: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPairingSummary {
    pub config: SyncRemoteConfig,
    pub device_public_key: String,
    pub device_fingerprint: String,
    pub generated_new_vault_key: bool,
}

impl AppServices {
    pub fn sync_keys(&self) -> SyncKeyStore {
        SyncKeyStore::for_data_dir(&self.data_dir)
    }

    pub fn sync_device_identity(&self) -> AppResult<(String, String)> {
        let kp = self.sync_keys().load_or_generate_device_key()?;
        Ok((kp.public_key_str, kp.fingerprint))
    }

    pub fn sync_vault_recovery_code(&self) -> AppResult<Option<Zeroizing<String>>> {
        Ok(self
            .sync_keys()
            .load_vault_key()?
            .map(|k| k.to_recovery_code()))
    }

    pub fn import_vault_recovery_code(&self, code: &str) -> AppResult<String> {
        let vmk = VaultMasterKey::from_recovery_code(code)?;
        self.sync_keys().save_vault_key(&vmk)?;
        let vid = vmk.vault_id();
        let mut cfg = self.db.load_sync_remote_config()?;
        cfg.vault_id = vid.clone();
        self.db.save_sync_remote_config(&cfg)?;
        Ok(vid)
    }

    pub fn load_sync_config(&self) -> AppResult<SyncRemoteConfig> {
        Ok(self.db.load_sync_remote_config()?)
    }

    pub fn update_sync_preferences(
        &self,
        auto_sync: bool,
        batch_debounce_secs: u64,
    ) -> AppResult<SyncRemoteConfig> {
        let mut cfg = self.db.load_sync_remote_config()?;
        cfg.auto_sync = auto_sync;
        cfg.batch_debounce_secs = batch_debounce_secs.clamp(2, 120);
        self.db.save_sync_remote_config(&cfg)?;
        self.remote_sync.notify_wake();
        Ok(cfg)
    }

    pub async fn pair_with_invite(
        &self,
        invite_code: &str,
        device_name: &str,
        optional_vault_code: Option<&str>,
    ) -> AppResult<SyncRemoteConfig> {
        let clean_name = sanitize_device_name(device_name);
        let invite = InvitePayload::decode(invite_code)?;
        let now = Utc::now().timestamp();
        if now > invite.e {
            return Err(AppError::Sync(
                "Срок действия инвайт-кода истёк — создайте новый на ноде".into(),
            ));
        }

        let server_url = normalize_server_url(&invite.u)?;
        let server_vk = invite.server_verifying_key()?;
        let server_pub = invite.server_pub_str()?;
        let server_fp = invite.server_fingerprint()?;
        let ticket_hash = invite.ticket_sha256_hex()?;

        let dev_kp = self.sync_keys().load_or_generate_device_key()?;
        let vmk = resolve_or_store_vmk(&self.sync_keys(), optional_vault_code)?;

        let ts = Utc::now().timestamp();
        let pair_msg = canonical_pair_message(
            &ticket_hash,
            &server_fp,
            &dev_kp.public_key_str,
            &clean_name,
            ts,
        );
        let client_sig = dev_kp.sign(pair_msg.as_bytes());

        let http = build_http_client()?;
        let resp = http
            .post(format!("{server_url}/v1/pair"))
            .json(&serde_json::json!({
                "ticket_b64": invite.t,
                "device_name": clean_name,
                "client_pub": dev_kp.public_key_str,
                "timestamp": ts,
                "client_sig": client_sig,
            }))
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Не удалось подключиться к ноде: {e}")))?;

        let pair_resp: PairResponse = parse_json_response(resp).await?;
        if pair_resp.server_fingerprint != server_fp || pair_resp.server_pub != server_pub {
            return Err(AppError::Sync(
                "Отпечаток ноды в ответе не совпадает с инвайт-кодом (возможна подмена MITM)"
                    .into(),
            ));
        }

        let ack_msg = canonical_pair_ack_message(
            &pair_resp.device_id,
            &server_fp,
            &dev_kp.fingerprint,
            pair_resp.timestamp,
        );
        verify_signature(&server_vk, ack_msg.as_bytes(), &pair_resp.server_sig)?;

        let cfg = SyncRemoteConfig {
            enabled: true,
            auto_sync: true,
            server_url,
            server_pub,
            server_fingerprint: server_fp,
            device_id: pair_resp.device_id,
            device_name: clean_name,
            vault_id: vmk.vault_id(),
            last_synced_seq: 0,
            last_synced_local_rev: 0,
            last_sync_at: None,
            last_status: SyncConnectionStatus::Idle,
            last_error: None,
            batch_debounce_secs: 8,
            last_merge_summary: None,
        };
        self.db.save_sync_remote_config(&cfg)?;
        *self.remote_sync.session.lock().await = None;

        let synced = self.sync_now().await?;
        self.remote_sync.notify_wake();
        Ok(synced)
    }

    pub async fn connect_manual_node(
        &self,
        server_url_raw: &str,
        expected_fingerprint_raw: &str,
        device_name: &str,
        optional_vault_code: Option<&str>,
    ) -> AppResult<SyncRemoteConfig> {
        let clean_name = sanitize_device_name(device_name);
        let server_url = normalize_server_url(server_url_raw)?;
        let expected_fp = expected_fingerprint_raw.trim();
        if expected_fp.is_empty() {
            return Err(AppError::Sync(
                "Укажите SHA256:... отпечаток ноды для защиты от подмены сервера".into(),
            ));
        }

        let http = build_http_client()?;
        let info_resp = http
            .get(format!("{server_url}/v1/info"))
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Нода недоступна: {e}")))?;
        let info: NodeInfoResponse = parse_json_response(info_resp).await?;
        if info.protocol != "oxsync-v1" {
            return Err(AppError::Sync(format!(
                "Несовместимый протокол ноды: {}",
                info.protocol
            )));
        }

        let server_vk = parse_pub_key(&info.node_pub)?;
        let computed_fp = key_fingerprint(&server_vk);
        if computed_fp != expected_fp || info.fingerprint != expected_fp {
            return Err(AppError::Sync(format!(
                "Отпечаток ноды НЕ СОВПАДАЕТ! Ожидался {expected_fp}, получен {computed_fp}"
            )));
        }

        let vmk = resolve_or_store_vmk(&self.sync_keys(), optional_vault_code)?;
        let provisional = SyncRemoteConfig {
            enabled: true,
            auto_sync: true,
            server_url: server_url.clone(),
            server_pub: info.node_pub,
            server_fingerprint: computed_fp,
            device_id: String::new(),
            device_name: clean_name,
            vault_id: vmk.vault_id(),
            last_synced_seq: 0,
            last_synced_local_rev: 0,
            last_sync_at: None,
            last_status: SyncConnectionStatus::Idle,
            last_error: None,
            batch_debounce_secs: 8,
            last_merge_summary: None,
        };
        let auth = self.authenticate_against(&http, &provisional).await?;
        let mut final_cfg = provisional;
        final_cfg.device_id = auth.device_id;
        self.db.save_sync_remote_config(&final_cfg)?;

        let synced = self.sync_now().await?;
        self.remote_sync.notify_wake();
        Ok(synced)
    }

    pub async fn disconnect_sync(&self) -> AppResult<SyncRemoteConfig> {
        *self.remote_sync.session.lock().await = None;
        self.db.reset_sync_remote_config()?;
        self.remote_sync.notify_wake();
        Ok(self.db.load_sync_remote_config()?)
    }

    pub async fn list_remote_devices(&self) -> AppResult<Vec<SyncDeviceInfo>> {
        let cfg = self.db.load_sync_remote_config()?;
        if !cfg.enabled {
            return Ok(Vec::new());
        }
        let http = build_http_client()?;
        let token = self.get_valid_token(&http, &cfg).await?;
        let resp = http
            .get(format!("{}/v1/devices", cfg.server_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка запроса списка устройств: {e}")))?;
        let rows: Vec<NodeDeviceRow> = parse_json_response(resp).await?;
        Ok(rows
            .into_iter()
            .map(|r| SyncDeviceInfo {
                is_current: r.id == cfg.device_id,
                id: r.id,
                user_id: r.user_id,
                name: r.name,
                public_key: r.public_key,
                fingerprint: r.fingerprint,
                created_at: r.created_at,
                last_seen_at: r.last_seen_at,
                revoked_at: r.revoked_at,
            })
            .collect())
    }

    pub async fn revoke_remote_device(&self, device_id: &str) -> AppResult<()> {
        let cfg = self.db.load_sync_remote_config()?;
        if !cfg.enabled {
            return Err(AppError::Sync("Синхронизация не подключена".into()));
        }
        let http = build_http_client()?;
        let token = self.get_valid_token(&http, &cfg).await?;
        let resp = http
            .delete(format!("{}/v1/devices/{device_id}", cfg.server_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка отзыва ключа устройства: {e}")))?;
        let _: NodeDeviceRow = parse_json_response(resp).await?;
        Ok(())
    }

    /// Synchronizes local SQLite vault with the remote zero-knowledge E2EE node:
    /// - Clean pull on new 2nd device or clean local state
    /// - Single batched push when only local changed
    /// - Automatic row-level 3-way SQLite merge on divergence or 409 Conflict,
    ///   publishing a single merged snapshot back to the node.
    pub async fn sync_now(&self) -> AppResult<SyncRemoteConfig> {
        let _guard = self.remote_sync.sync_op_lock.lock().await;
        let mut cfg = self.db.load_sync_remote_config()?;
        if !cfg.enabled || cfg.server_url.is_empty() {
            return Ok(cfg);
        }

        let res: AppResult<SyncRemoteConfig> = self.sync_now_inner(&mut cfg).await;
        match res {
            Ok(updated) => Ok(updated),
            Err(err) => {
                let err_msg = err.to_string();
                cfg.last_status = match &err {
                    AppError::VaultKeyMismatch(_) => SyncConnectionStatus::Conflict,
                    AppError::Sync(msg)
                        if msg.contains("connect")
                            || msg.contains("Нода недоступна")
                            || msg.contains("Не удалось подключиться")
                            || msg.contains("error sending request") =>
                    {
                        SyncConnectionStatus::Offline
                    }
                    _ => SyncConnectionStatus::Error,
                };
                cfg.last_error = Some(err_msg);
                let _ = self.db.save_sync_remote_config(&cfg);
                Err(err)
            }
        }
    }

    async fn sync_now_inner(&self, cfg: &mut SyncRemoteConfig) -> AppResult<SyncRemoteConfig> {
        let vmk = self.sync_keys().load_vault_key()?.ok_or_else(|| {
            AppError::VaultKeyMismatch(
                "Не найден ключ шифрования хранилища sync-vault.key — импортируйте код oxvault-v1-..."
                    .into(),
            )
        })?;
        if cfg.vault_id != vmk.vault_id() {
            cfg.vault_id = vmk.vault_id();
        }

        let http = build_http_client()?;
        let token = self.get_valid_token(&http, cfg).await?;

        let head_resp = http
            .get(format!("{}/v1/vault/head", cfg.server_url))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Не удалось проверить состояние ноды: {e}")))?;
        let head: VaultHeadResponse = parse_json_response(head_resp).await?;

        let local_rev = self.db.get_sync_state()?.revision;
        let local_dirty = cfg.last_synced_local_rev == 0 || local_rev > cfg.last_synced_local_rev;
        let remote_newer = head.seq > cfg.last_synced_seq;

        // Fresh empty 2nd device pulling initial snapshot from non-empty node
        let fresh_second_device =
            cfg.last_synced_seq == 0 && head.seq > 0 && !self.db.has_user_academic_data()?;

        if !remote_newer && !local_dirty {
            cfg.last_status = SyncConnectionStatus::Synced;
            cfg.last_sync_at = Some(Utc::now());
            cfg.last_error = None;
            self.db.save_sync_remote_config(cfg)?;
            return Ok(cfg.clone());
        }

        if fresh_second_device || (remote_newer && !local_dirty) {
            let (seq, summary_opt) = self.pull_clean_snapshot(&http, &token, cfg, &vmk).await?;
            let mut reloaded = self.db.load_sync_remote_config()?;
            reloaded.last_synced_seq = seq;
            reloaded.last_status = SyncConnectionStatus::Synced;
            reloaded.last_error = None;
            if let Some(s) = summary_opt {
                reloaded.last_merge_summary = Some(s);
            }
            self.db.save_sync_remote_config(&reloaded)?;
            self.invalidate_stats_cache();
            self.sync
                .notify_local_change(RecordSource::Import, "remote_sync");
            return Ok(reloaded);
        }

        // If only local changed, attempt single optimistic push; on 409 Conflict or known divergence, merge!
        if !remote_newer && local_dirty {
            match self
                .encrypt_and_push_current_db(&http, &token, cfg, &vmk, head.seq)
                .await?
            {
                Some(push_res) => {
                    cfg.last_synced_seq = push_res.seq;
                    cfg.last_synced_local_rev = local_rev;
                    cfg.last_sync_at = Some(Utc::now());
                    cfg.last_status = SyncConnectionStatus::Synced;
                    cfg.last_error = None;
                    self.db.save_sync_remote_config(cfg)?;
                    return Ok(cfg.clone());
                }
                None => {
                    // 409 Conflict detected -> proceed directly to 3-way SQLite merge loop below
                }
            }
        }

        // Divergence or 409 Conflict -> automatic tombstone-aware 3-way SQLite merge (max 3 attempts)
        self.create_pre_sync_safety_backup()?;
        for _attempt in 0..3 {
            let (remote_seq, remote_bytes) = self.download_latest_blob(&http, &token, cfg).await?;
            let (_aad, sqlite_plain) = decrypt_snapshot_blob(&vmk, &remote_bytes)?;

            let tmp_dir = tempfile::tempdir()?;
            let remote_tmp = tmp_dir.path().join("remote-decrypted.db");
            std::fs::write(&remote_tmp, &sqlite_plain)?;

            let report = self.db.merge_remote_snapshot(&remote_tmp, remote_seq)?;
            let updated_cfg = self.db.load_sync_remote_config()?;
            *cfg = updated_cfg;

            if let Some(push_res) = self
                .encrypt_and_push_current_db(&http, &token, cfg, &vmk, remote_seq)
                .await?
            {
                let new_rev = self.db.get_sync_state()?.revision;
                cfg.last_synced_seq = push_res.seq;
                cfg.last_synced_local_rev = new_rev;
                cfg.last_sync_at = Some(Utc::now());
                cfg.last_status = SyncConnectionStatus::Synced;
                cfg.last_error = None;
                cfg.last_merge_summary = Some(report.summary_ru);
                self.db.save_sync_remote_config(cfg)?;
                self.invalidate_stats_cache();
                self.sync
                    .notify_local_change(RecordSource::Import, "remote_sync");
                return Ok(cfg.clone());
            }
        }

        Err(AppError::Sync(
            "Конкурентные изменения на другом устройстве; повторите синхронизацию позже".into(),
        ))
    }

    async fn pull_clean_snapshot(
        &self,
        http: &Client,
        token: &str,
        cfg: &SyncRemoteConfig,
        vmk: &VaultMasterKey,
    ) -> AppResult<(i64, Option<String>)> {
        let _ = self.create_pre_sync_safety_backup();
        let (remote_seq, blob) = self.download_latest_blob(http, token, cfg).await?;
        let (_aad, plain) = decrypt_snapshot_blob(vmk, &blob)?;

        let tmp = tempfile::tempdir()?;
        let snap_path = tmp.path().join("pulled.db");
        std::fs::write(&snap_path, &plain)?;

        let summary = Some(format!("Загружена ревизия #{remote_seq} с ноды"));
        self.db
            .restore_from_path_preserving_sync_state(&snap_path, remote_seq, summary.clone())?;
        Ok((remote_seq, summary))
    }

    async fn download_latest_blob(
        &self,
        http: &Client,
        token: &str,
        cfg: &SyncRemoteConfig,
    ) -> AppResult<(i64, Vec<u8>)> {
        let resp = http
            .get(format!("{}/v1/vault/snapshots/latest", cfg.server_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка скачивания снимка: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(parse_error_response(status, resp).await);
        }

        let seq: i64 = resp
            .headers()
            .get("x-oxsync-seq")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                AppError::Sync("Ответ ноды не содержит заголовок X-OxSync-Seq".into())
            })?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка чтения тела снимка: {e}")))?;
        Ok((seq, bytes.to_vec()))
    }

    async fn encrypt_and_push_current_db(
        &self,
        http: &Client,
        token: &str,
        cfg: &SyncRemoteConfig,
        vmk: &VaultMasterKey,
        expected_seq: i64,
    ) -> AppResult<Option<PushSnapshotResponse>> {
        let tmp = tempfile::tempdir()?;
        let snap_path = tmp.path().join("upload.db");
        self.db.backup_to_path(&snap_path)?;
        let raw_sqlite = std::fs::read(&snap_path)?;

        let blob = encrypt_snapshot_blob(
            vmk,
            expected_seq + 1,
            &cfg.device_id,
            4,
            &Utc::now().to_rfc3339(),
            &raw_sqlite,
        )?;

        let resp = http
            .post(format!("{}/v1/vault/snapshots", cfg.server_url))
            .bearer_auth(token)
            .header("X-OxSync-Expected-Seq", expected_seq.to_string())
            .header("X-OxSync-Vault-Id", vmk.vault_id())
            .body(blob)
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка отправки снимка: {e}")))?;

        if resp.status() == reqwest::StatusCode::CONFLICT {
            return Ok(None);
        }
        let parsed: PushSnapshotResponse = parse_json_response(resp).await?;
        Ok(Some(parsed))
    }

    async fn get_valid_token(&self, http: &Client, cfg: &SyncRemoteConfig) -> AppResult<String> {
        let now = Utc::now().timestamp();
        {
            let guard = self.remote_sync.session.lock().await;
            if let Some(ref cached) = *guard
                && cached.server_url == cfg.server_url
                && cached.device_id == cfg.device_id
                && cached.expires_at - 60 > now
            {
                return Ok(cached.token.clone());
            }
        }
        let auth = self.authenticate_against(http, cfg).await?;
        Ok(auth.token)
    }

    async fn authenticate_against(
        &self,
        http: &Client,
        cfg: &SyncRemoteConfig,
    ) -> AppResult<CachedSession> {
        let dev_kp = self.sync_keys().load_or_generate_device_key()?;
        let server_vk = parse_pub_key(&cfg.server_pub)?;
        let client_nonce = random_b64url(32);

        let ch_resp = http
            .post(format!("{}/v1/auth/challenge", cfg.server_url))
            .json(&serde_json::json!({
                "client_pub": dev_kp.public_key_str,
                "client_nonce": client_nonce,
            }))
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка запроса challenge: {e}")))?;
        let ch: AuthChallengeResponse = parse_json_response(ch_resp).await?;

        let server_msg = canonical_server_challenge_message(
            &ch.challenge_id,
            &ch.server_nonce,
            &client_nonce,
            &dev_kp.fingerprint,
            ch.expires_at,
        );
        verify_signature(&server_vk, server_msg.as_bytes(), &ch.server_sig)?;

        let auth_msg = canonical_client_auth_message(
            &ch.challenge_id,
            &ch.server_nonce,
            &client_nonce,
            &cfg.server_fingerprint,
            &dev_kp.fingerprint,
        );
        let client_sig = dev_kp.sign(auth_msg.as_bytes());

        let ver_resp = http
            .post(format!("{}/v1/auth/verify", cfg.server_url))
            .json(&serde_json::json!({
                "challenge_id": ch.challenge_id,
                "client_sig": client_sig,
            }))
            .send()
            .await
            .map_err(|e| AppError::Sync(format!("Ошибка подтверждения подписи: {e}")))?;
        let ver: AuthVerifyResponse = parse_json_response(ver_resp).await?;

        let cached = CachedSession {
            server_url: cfg.server_url.clone(),
            device_id: ver.device_id,
            token: ver.token,
            expires_at: ver.expires_at,
        };
        *self.remote_sync.session.lock().await = Some(cached.clone());
        Ok(cached)
    }

    fn create_pre_sync_safety_backup(&self) -> AppResult<PathBuf> {
        let dir = self.backups_dir();
        std::fs::create_dir_all(&dir)?;
        let stamp = Utc::now().format("%Y%m%d-%H%M%S-%3f");
        let path = dir.join(format!("pre-sync-{stamp}.db"));
        self.db.backup_to_path(&path)?;

        // Retain at most 5 recent pre-sync safety snapshots
        let mut pre_syncs: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("pre-sync-") && n.ends_with(".db"))
            })
            .collect();
        if pre_syncs.len() > 5 {
            pre_syncs.sort_by_key(|e| e.file_name());
            let remove_n = pre_syncs.len() - 5;
            for old in pre_syncs.into_iter().take(remove_n) {
                let _ = std::fs::remove_file(old.path());
            }
        }
        Ok(path)
    }
}

/// Runs the non-blocking batched background sync loop + live WebSocket listener.
/// - Local edits are coalesced with `cfg.batch_debounce_secs` (default 8s, max batch window 30s).
/// - Live `/v1/ws` `head_updated` notifications trigger an immediate background pull/merge.
pub async fn run_remote_sync_worker<F>(
    app: Arc<AppServices>,
    stop_flag: Arc<AtomicBool>,
    on_remote_applied: F,
) where
    F: Fn() + Send + Sync + 'static,
{
    let on_applied = Arc::new(on_remote_applied);
    let mut local_rx = app.sync.subscribe();
    let wake = Arc::clone(&app.remote_sync.wake_notify);

    let mut dirty_since: Option<Instant> = None;
    let mut last_change_at: Option<Instant> = None;
    let mut backoff_secs = 2u64;
    let mut ws_handle: Option<tokio::task::JoinHandle<()>> = None;

    // Initial sync tick on startup if configured
    if let Ok(cfg) = app.load_sync_config()
        && cfg.enabled
        && cfg.auto_sync
    {
        let prev_rev = app.sync.current_revision();
        if app.sync_now().await.is_ok() && app.sync.current_revision() != prev_rev {
            on_applied();
        }
    }

    let mut poll_interval = tokio::time::interval(Duration::from_secs(60));
    poll_interval.tick().await;

    while !stop_flag.load(Ordering::Relaxed) {
        let cfg = app.load_sync_config().unwrap_or_default();
        if cfg.enabled && cfg.auto_sync {
            let needs_spawn = ws_handle.as_ref().is_none_or(|h| h.is_finished());
            if needs_spawn {
                let ws_app = Arc::clone(&app);
                let ws_stop = Arc::clone(&stop_flag);
                let ws_applied = Arc::clone(&on_applied);
                let delay = backoff_secs;
                ws_handle = Some(tokio::spawn(async move {
                    run_ws_listener_once(ws_app, ws_stop, ws_applied, delay).await;
                }));
                backoff_secs = (backoff_secs * 2).min(60);
            }
        } else if let Some(h) = ws_handle.take() {
            h.abort();
            backoff_secs = 2;
        }

        let debounce = Duration::from_secs(cfg.batch_debounce_secs.clamp(2, 120));
        let max_batch = Duration::from_secs(30);

        let flush_deadline = match (dirty_since, last_change_at) {
            (Some(first), Some(last)) => {
                let d1 = last + debounce;
                let d2 = first + max_batch;
                Some(d1.min(d2))
            }
            _ => None,
        };

        tokio::select! {
            ev = local_rx.recv() => {
                if let Ok(event) = ev {
                    // Ignore notifications emitted by our own remote pull/merge
                    if !(event.source == RecordSource::Import && event.entity == "remote_sync") {
                        let now = Instant::now();
                        if dirty_since.is_none() {
                            dirty_since = Some(now);
                        }
                        last_change_at = Some(now);
                    }
                }
            }
            _ = wake.notified() => {
                backoff_secs = 2;
            }
            _ = poll_interval.tick() => {
                if let Ok(c) = app.load_sync_config() && c.enabled && c.auto_sync {
                    let prev_rev = app.sync.current_revision();
                    if app.sync_now().await.is_ok() {
                        backoff_secs = 2;
                        if app.sync.current_revision() != prev_rev {
                            on_applied();
                        }
                        dirty_since = None;
                        last_change_at = None;
                    }
                }
            }
            _ = async {
                if let Some(dl) = flush_deadline {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(dl)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                dirty_since = None;
                last_change_at = None;
                if let Ok(c) = app.load_sync_config() && c.enabled && c.auto_sync {
                    let prev_rev = app.sync.current_revision();
                    if app.sync_now().await.is_ok() && app.sync.current_revision() != prev_rev {
                        on_applied();
                    }
                }
            }
        }
    }

    if let Some(h) = ws_handle.take() {
        h.abort();
    }
}

async fn run_ws_listener_once(
    app: Arc<AppServices>,
    stop_flag: Arc<AtomicBool>,
    on_applied: Arc<dyn Fn() + Send + Sync>,
    initial_backoff_secs: u64,
) {
    let Ok(cfg) = app.load_sync_config() else {
        return;
    };
    if !cfg.enabled || !cfg.auto_sync {
        return;
    }

    let Ok(http) = build_http_client() else {
        return;
    };
    let Ok(token) = app.get_valid_token(&http, &cfg).await else {
        tokio::time::sleep(Duration::from_secs(initial_backoff_secs)).await;
        return;
    };

    let ws_url = to_ws_url(&cfg.server_url);
    let Ok(mut req) = ws_url.into_client_request() else {
        return;
    };
    if let Ok(hv) = HeaderValue::from_str(&format!("Bearer {token}")) {
        req.headers_mut().insert("Authorization", hv);
    }

    let Ok((mut stream, _)) = tokio_tungstenite::connect_async(req).await else {
        tokio::time::sleep(Duration::from_secs(initial_backoff_secs)).await;
        return;
    };

    while !stop_flag.load(Ordering::Relaxed) {
        let Some(msg) = stream.next().await else {
            break;
        };
        let Ok(msg) = msg else {
            break;
        };
        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt)
        {
            let ev_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ev_type == "head_updated" {
                let remote_dev = val.get("device_id").and_then(|v| v.as_str()).unwrap_or("");
                let remote_seq = val.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
                let latest_cfg = app.load_sync_config().unwrap_or_default();
                if remote_dev != latest_cfg.device_id && remote_seq > latest_cfg.last_synced_seq {
                    let prev_rev = app.sync.current_revision();
                    if app.sync_now().await.is_ok() && app.sync.current_revision() != prev_rev {
                        on_applied();
                    }
                }
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
}

fn to_ws_url(http_url: &str) -> String {
    let trimmed = http_url.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}/v1/ws")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}/v1/ws")
    } else {
        format!("{trimmed}/v1/ws")
    }
}

fn build_http_client() -> AppResult<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Sync(format!("Ошибка инициализации HTTP-клиента: {e}")))
}

fn normalize_server_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(AppError::Sync(
            "URL ноды должен начинаться с https:// или http://".into(),
        ));
    }
    Ok(trimmed)
}

fn sanitize_device_name(raw: &str) -> String {
    let clean: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '|')
        .take(64)
        .collect();
    if clean.is_empty() {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Oxidium-Device".into())
    } else {
        clean
    }
}

fn resolve_or_store_vmk(
    keys: &SyncKeyStore,
    optional_vault_code: Option<&str>,
) -> AppResult<VaultMasterKey> {
    if let Some(raw) = optional_vault_code
        && !raw.trim().is_empty()
    {
        let parsed = VaultMasterKey::from_recovery_code(raw)?;
        keys.save_vault_key(&parsed)?;
        return Ok(parsed);
    }
    keys.load_or_generate_vault_key()
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
) -> AppResult<T> {
    let status = resp.status();
    if !status.is_success() {
        return Err(parse_error_response(status, resp).await);
    }
    resp.json::<T>()
        .await
        .map_err(|e| AppError::Sync(format!("Некорректный JSON-ответ ноды: {e}")))
}

async fn parse_error_response(status: reqwest::StatusCode, resp: reqwest::Response) -> AppError {
    let body = resp.text().await.unwrap_or_default();
    if let Ok(err_obj) = serde_json::from_str::<NodeErrorPayload>(&body) {
        return AppError::Sync(format!(
            "Нода вернула HTTP {}: {}",
            status.as_u16(),
            err_obj.message
        ));
    }
    AppError::Sync(format!("Нода вернула HTTP {}", status.as_u16()))
}

// Keep SyncCoordinator type linked cleanly even when called generically
#[allow(dead_code)]
fn _touch_coordinator(_c: &SyncCoordinator) {}
