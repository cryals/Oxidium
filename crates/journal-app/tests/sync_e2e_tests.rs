use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as AxumPath, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use chrono::{NaiveDate, Utc};
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use journal_app::sync_crypto::{
    Oxe1Aad, VaultMasterKey, canonical_client_auth_message, canonical_pair_ack_message,
    canonical_pair_message, canonical_server_challenge_message, decrypt_snapshot_blob,
    encrypt_snapshot_blob_with_nonce, format_pub_key, key_fingerprint, parse_pub_key,
    random_b64url, sha256_hex, verify_signature,
};
use journal_app::{
    AddGradeInput, AppError, AppServices, CreateBoardInput, CreateColumnInput, CreateSubjectInput,
    CreateTaskInput, UpdateSubjectInput, redact_secrets,
};
use journal_domain::{RecordSource, TaskPriority, WorkType};
use journal_storage::SearchFilter;
use serde::Deserialize;
use sha2_10::Sha256;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;
use uuid::Uuid;

// ============================================================================
// Minimal In-Process Production-Fidelity Mock of `oxidium-node` for E2E Tests
// ============================================================================

#[derive(Clone)]
struct MockDevice {
    id: String,
    user_id: String,
    name: String,
    public_key: String,
    fingerprint: String,
    revoked_at: Option<String>,
}

#[derive(Clone)]
struct MockTicket {
    user_id: String,
    expires_at: i64,
    used: bool,
}

#[derive(Clone)]
struct MockChallenge {
    device_id: String,
    server_nonce: String,
    client_nonce: String,
    used: bool,
}

#[derive(Clone)]
struct MockRevision {
    seq: i64,
    vault_id: String,
    device_id: String,
    device_name: String,
    blob: Vec<u8>,
    blob_sha256: String,
    created_at: String,
}

struct MockNodeInner {
    devices: HashMap<String, MockDevice>,
    tickets: HashMap<String, MockTicket>,
    challenges: HashMap<String, MockChallenge>,
    revisions: HashMap<String, Vec<MockRevision>>, // per user_id; retains retention_count=1
}

struct MockNodeState {
    server_signing: SigningKey,
    server_pub: String,
    server_fp: String,
    hmac_key: [u8; 32],
    retention_count: usize,
    inner: Mutex<MockNodeInner>,
}

impl MockNodeState {
    fn new(seed: [u8; 32], retention_count: usize) -> Self {
        let server_signing = SigningKey::from_bytes(&seed);
        let vk = server_signing.verifying_key();
        let hk = Hkdf::<Sha256>::new(Some(b"oxsync-v1-session"), &seed);
        let mut hmac_key = [0u8; 32];
        hk.expand(b"hmac-key", &mut hmac_key).unwrap();
        Self {
            server_pub: format_pub_key(&vk),
            server_fp: key_fingerprint(&vk),
            server_signing,
            hmac_key,
            retention_count: retention_count.max(1),
            inner: Mutex::new(MockNodeInner {
                devices: HashMap::new(),
                tickets: HashMap::new(),
                challenges: HashMap::new(),
                revisions: HashMap::new(),
            }),
        }
    }

    fn issue_invite(&self, user_id: &str, base_url: &str, raw_ticket: [u8; 16]) -> String {
        let hash = sha256_hex(&raw_ticket);
        let exp = Utc::now().timestamp() + 900;
        self.inner.lock().unwrap().tickets.insert(
            hash,
            MockTicket {
                user_id: user_id.to_string(),
                expires_at: exp,
                used: false,
            },
        );
        let payload = serde_json::json!({
            "v": 1,
            "u": base_url,
            "k": STANDARD_NO_PAD.encode(self.server_signing.verifying_key().to_bytes()),
            "t": URL_SAFE_NO_PAD.encode(raw_ticket),
            "e": exp,
        });
        format!(
            "oxsync-v1:{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap())
        )
    }

    fn add_manual_device(&self, user_id: &str, name: &str, pub_str: &str) -> MockDevice {
        let vk = parse_pub_key(pub_str).unwrap();
        let fp = key_fingerprint(&vk);
        let dev = MockDevice {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            public_key: pub_str.to_string(),
            fingerprint: fp,
            revoked_at: None,
        };
        self.inner
            .lock()
            .unwrap()
            .devices
            .insert(dev.id.clone(), dev.clone());
        dev
    }

    fn sign(&self, msg: &[u8]) -> String {
        STANDARD_NO_PAD.encode(self.server_signing.sign(msg).to_bytes())
    }

    fn issue_token(&self, user_id: &str, device_id: &str, fp: &str, now: i64) -> String {
        let claims = serde_json::json!({
            "v": 1, "u": user_id, "d": device_id, "f": fp, "exp": now + 900, "jti": random_b64url(8),
        });
        let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.hmac_key).unwrap();
        mac.update(p.as_bytes());
        let t = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("oxtok1.{p}.{t}")
    }

    fn verify_auth(
        &self,
        headers: &HeaderMap,
    ) -> Result<MockDevice, (StatusCode, Json<serde_json::Value>)> {
        let tok = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error":"unauthorized","message":"missing bearer"})),
                )
            })?;
        let rest = tok.strip_prefix("oxtok1.").unwrap();
        let (p, sig) = rest.split_once('.').unwrap();
        let sig_bytes = URL_SAFE_NO_PAD.decode(sig).unwrap();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.hmac_key).unwrap();
        mac.update(p.as_bytes());
        assert_eq!(
            mac.finalize()
                .into_bytes()
                .as_slice()
                .ct_eq(&sig_bytes)
                .unwrap_u8(),
            1
        );
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).unwrap()).unwrap();
        let dev_id = claims["d"].as_str().unwrap();
        let guard = self.inner.lock().unwrap();
        let dev = guard.devices.get(dev_id).cloned().ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error":"forbidden","message":"missing device"})),
            )
        })?;
        if dev.revoked_at.is_some() {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error":"forbidden","message":"device revoked"})),
            ));
        }
        Ok(dev)
    }

    fn stored_revisions(&self, user_id: &str) -> Vec<MockRevision> {
        self.inner
            .lock()
            .unwrap()
            .revisions
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }
}

async fn spawn_mock_node(
    seed: [u8; 32],
    retention_count: usize,
) -> (String, Arc<MockNodeState>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(MockNodeState::new(seed, retention_count));
    let app = Router::new()
        .route("/v1/info", get(mock_info))
        .route("/v1/pair", post(mock_pair))
        .route("/v1/auth/challenge", post(mock_challenge))
        .route("/v1/auth/verify", post(mock_verify))
        .route("/v1/vault/head", get(mock_head))
        .route("/v1/vault/snapshots", post(mock_push))
        .route("/v1/vault/snapshots/latest", get(mock_latest))
        .route("/v1/devices", get(mock_devices))
        .route("/v1/devices/{id}", delete(mock_revoke))
        .with_state(Arc::clone(&state))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (url, state, h)
}

async fn mock_info(State(st): State<Arc<MockNodeState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "protocol": "oxsync-v1",
        "node_pub": st.server_pub,
        "fingerprint": st.server_fp,
    }))
}

#[derive(Deserialize)]
struct PairReq {
    ticket_b64: String,
    device_name: String,
    client_pub: String,
    timestamp: i64,
    client_sig: String,
}

async fn mock_pair(State(st): State<Arc<MockNodeState>>, Json(req): Json<PairReq>) -> Response {
    let raw_t = URL_SAFE_NO_PAD.decode(&req.ticket_b64).unwrap();
    let t_hash = sha256_hex(&raw_t);
    let client_vk = parse_pub_key(&req.client_pub).unwrap();
    let client_fp = key_fingerprint(&client_vk);
    let msg = canonical_pair_message(
        &t_hash,
        &st.server_fp,
        &req.client_pub,
        &req.device_name,
        req.timestamp,
    );
    if verify_signature(&client_vk, msg.as_bytes(), &req.client_sig).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized","message":"bad sig"})),
        )
            .into_response();
    }
    let mut guard = st.inner.lock().unwrap();
    let now = Utc::now().timestamp();
    let Some(t) = guard.tickets.get_mut(&t_hash) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized","message":"bad ticket"})),
        )
            .into_response();
    };
    if t.used || now > t.expires_at {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized","message":"ticket used or expired"})),
        )
            .into_response();
    }
    t.used = true;
    let uid = t.user_id.clone();
    let dev = MockDevice {
        id: Uuid::new_v4().to_string(),
        user_id: uid,
        name: req.device_name,
        public_key: req.client_pub,
        fingerprint: client_fp.clone(),
        revoked_at: None,
    };
    guard.devices.insert(dev.id.clone(), dev.clone());
    drop(guard);

    let ts = Utc::now().timestamp();
    let ack = canonical_pair_ack_message(&dev.id, &st.server_fp, &client_fp, ts);
    let server_sig = st.sign(ack.as_bytes());
    Json(serde_json::json!({
        "device_id": dev.id,
        "server_pub": st.server_pub,
        "server_fingerprint": st.server_fp,
        "timestamp": ts,
        "server_sig": server_sig,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct ChReq {
    client_pub: String,
    client_nonce: String,
}

async fn mock_challenge(State(st): State<Arc<MockNodeState>>, Json(req): Json<ChReq>) -> Response {
    let client_vk = parse_pub_key(&req.client_pub).unwrap();
    let client_fp = key_fingerprint(&client_vk);
    let mut guard = st.inner.lock().unwrap();
    let dev = guard
        .devices
        .values()
        .find(|d| d.public_key == req.client_pub)
        .cloned();
    let Some(dev) = dev else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized","message":"unknown key"})),
        )
            .into_response();
    };
    if dev.revoked_at.is_some() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"forbidden","message":"revoked"})),
        )
            .into_response();
    }
    let ch_id = Uuid::new_v4().to_string();
    let sn = random_b64url(32);
    let exp = Utc::now().timestamp() + 60;
    guard.challenges.insert(
        ch_id.clone(),
        MockChallenge {
            device_id: dev.id.clone(),
            server_nonce: sn.clone(),
            client_nonce: req.client_nonce.clone(),
            used: false,
        },
    );
    drop(guard);
    let msg = canonical_server_challenge_message(&ch_id, &sn, &req.client_nonce, &client_fp, exp);
    let server_sig = st.sign(msg.as_bytes());
    Json(serde_json::json!({
        "challenge_id": ch_id,
        "server_nonce": sn,
        "expires_at": exp,
        "server_sig": server_sig,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct VerReq {
    challenge_id: String,
    client_sig: String,
}

async fn mock_verify(State(st): State<Arc<MockNodeState>>, Json(req): Json<VerReq>) -> Response {
    let mut guard = st.inner.lock().unwrap();
    let (ch_dev_id, ch_sn, ch_cn) = {
        let ch = guard.challenges.get_mut(&req.challenge_id).unwrap();
        assert!(!ch.used);
        ch.used = true;
        (
            ch.device_id.clone(),
            ch.server_nonce.clone(),
            ch.client_nonce.clone(),
        )
    };
    let dev = guard.devices.get(&ch_dev_id).unwrap().clone();
    drop(guard);

    let vk = parse_pub_key(&dev.public_key).unwrap();
    let msg = canonical_client_auth_message(
        &req.challenge_id,
        &ch_sn,
        &ch_cn,
        &st.server_fp,
        &dev.fingerprint,
    );
    verify_signature(&vk, msg.as_bytes(), &req.client_sig).unwrap();
    let now = Utc::now().timestamp();
    let token = st.issue_token(&dev.user_id, &dev.id, &dev.fingerprint, now);
    Json(serde_json::json!({
        "token": token,
        "expires_at": now + 900,
        "device_id": dev.id,
    }))
    .into_response()
}

async fn mock_head(State(st): State<Arc<MockNodeState>>, headers: HeaderMap) -> Response {
    let dev = match st.verify_auth(&headers) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    let revs = st.stored_revisions(&dev.user_id);
    if let Some(last) = revs.last() {
        Json(serde_json::json!({
            "seq": last.seq,
            "vault_id": last.vault_id,
            "device_id": last.device_id,
            "device_name": last.device_name,
            "updated_at": last.created_at,
        }))
        .into_response()
    } else {
        Json(serde_json::json!({ "seq": 0 })).into_response()
    }
}

async fn mock_push(
    State(st): State<Arc<MockNodeState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let dev = match st.verify_auth(&headers) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    let expected: i64 = headers
        .get("x-oxsync-expected-seq")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(&body[..4], b"OXE1");
    let aad_len = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
    let aad: Oxe1Aad = serde_json::from_slice(&body[8..8 + aad_len]).unwrap();

    let mut guard = st.inner.lock().unwrap();
    let list = guard.revisions.entry(dev.user_id.clone()).or_default();
    let cur_seq = list.last().map(|r| r.seq).unwrap_or(0);
    if expected != cur_seq {
        let last = list.last().unwrap();
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "conflict",
                "message": "expected_seq mismatch",
                "current_seq": last.seq,
                "current_device_id": last.device_id,
                "current_device_name": last.device_name,
                "current_updated_at": last.created_at,
                "current_blob_sha256": last.blob_sha256,
            })),
        )
            .into_response();
    }
    let next_seq = cur_seq + 1;
    let now = Utc::now().to_rfc3339();
    list.push(MockRevision {
        seq: next_seq,
        vault_id: aad.vault_id,
        device_id: dev.id.clone(),
        device_name: dev.name.clone(),
        blob_sha256: sha256_hex(&body),
        blob: body.to_vec(),
        created_at: now.clone(),
    });
    if list.len() > st.retention_count {
        let excess = list.len() - st.retention_count;
        list.drain(0..excess);
    }
    Json(serde_json::json!({ "seq": next_seq, "created_at": now })).into_response()
}

async fn mock_latest(State(st): State<Arc<MockNodeState>>, headers: HeaderMap) -> Response {
    let dev = match st.verify_auth(&headers) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    let revs = st.stored_revisions(&dev.user_id);
    let last = revs.last().unwrap().clone();
    let mut resp = (StatusCode::OK, last.blob).into_response();
    resp.headers_mut().insert(
        HeaderName::from_static("x-oxsync-seq"),
        HeaderValue::from_str(&last.seq.to_string()).unwrap(),
    );
    resp
}

async fn mock_devices(State(st): State<Arc<MockNodeState>>, headers: HeaderMap) -> Response {
    let dev = match st.verify_auth(&headers) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    let guard = st.inner.lock().unwrap();
    let list: Vec<_> = guard
        .devices
        .values()
        .filter(|d| d.user_id == dev.user_id)
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "user_id": d.user_id,
                "name": d.name,
                "public_key": d.public_key,
                "fingerprint": d.fingerprint,
                "created_at": "2026-11-27T00:00:00Z",
                "last_seen_at": Option::<String>::None,
                "revoked_at": d.revoked_at,
            })
        })
        .collect();
    Json(list).into_response()
}

async fn mock_revoke(
    State(st): State<Arc<MockNodeState>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    let dev = match st.verify_auth(&headers) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };
    let mut guard = st.inner.lock().unwrap();
    let target = guard.devices.get_mut(&id).unwrap();
    assert_eq!(target.user_id, dev.user_id);
    target.revoked_at = Some(Utc::now().to_rfc3339());
    Json(serde_json::json!({
        "id": target.id,
        "user_id": target.user_id,
        "name": target.name,
        "public_key": target.public_key,
        "fingerprint": target.fingerprint,
        "created_at": "2026-11-27T00:00:00Z",
        "last_seen_at": Option::<String>::None,
        "revoked_at": target.revoked_at,
    }))
    .into_response()
}

// ============================================================================
// Tests
// ============================================================================

#[derive(Deserialize)]
struct GoldenVectors {
    server_seed_hex: String,
    server_pub: String,
    server_fingerprint: String,
    client_seed_hex: String,
    client_pub: String,
    client_fingerprint: String,
    vmk_hex: String,
    vmk_recovery_code: String,
    vault_id: String,
    invite_code: String,
    ticket_sha256_hex: String,
    device_id: String,
    device_name: String,
    timestamp_unix: i64,
    pair_message: String,
    pair_client_sig: String,
    pair_ack_message: String,
    pair_server_sig: String,
    challenge_id: String,
    server_nonce_b64: String,
    client_nonce_b64: String,
    challenge_expires_at: i64,
    server_challenge_message: String,
    server_challenge_sig: String,
    client_auth_message: String,
    client_auth_sig: String,
    oxe1_plaintext_utf8: String,
    oxe1_salt_hex: String,
    oxe1_nonce_hex: String,
    oxe1_created_at: String,
    oxe1_envelope_b64: String,
}

#[test]
fn golden_vectors_match_oxidium_sync_node_bit_for_bit() {
    let vec_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sync-v1-vectors.json");
    let raw = std::fs::read_to_string(vec_path).unwrap();
    let v: GoldenVectors = serde_json::from_str(&raw).unwrap();

    let server_seed: [u8; 32] = hex::decode(&v.server_seed_hex).unwrap().try_into().unwrap();
    let server_sk = SigningKey::from_bytes(&server_seed);
    assert_eq!(format_pub_key(&server_sk.verifying_key()), v.server_pub);
    assert_eq!(
        key_fingerprint(&server_sk.verifying_key()),
        v.server_fingerprint
    );

    let client_seed: [u8; 32] = hex::decode(&v.client_seed_hex).unwrap().try_into().unwrap();
    let client_kp = journal_app::sync_crypto::DeviceKeypair::from_secret_bytes(client_seed);
    assert_eq!(client_kp.public_key_str, v.client_pub);
    assert_eq!(client_kp.fingerprint, v.client_fingerprint);

    let invite = journal_app::sync_crypto::InvitePayload::decode(&v.invite_code).unwrap();
    assert_eq!(invite.server_pub_str().unwrap(), v.server_pub);
    assert_eq!(invite.server_fingerprint().unwrap(), v.server_fingerprint);
    assert_eq!(invite.ticket_sha256_hex().unwrap(), v.ticket_sha256_hex);

    let vmk_bytes: [u8; 32] = hex::decode(&v.vmk_hex).unwrap().try_into().unwrap();
    let vmk = VaultMasterKey::from_bytes(vmk_bytes);
    assert_eq!(&*vmk.to_recovery_code(), &v.vmk_recovery_code);
    assert_eq!(vmk.vault_id(), v.vault_id);
    assert_eq!(
        VaultMasterKey::from_recovery_code(&v.vmk_recovery_code)
            .unwrap()
            .as_bytes(),
        &vmk_bytes
    );

    // Corrupted checksum fails cleanly
    let mut bad_vmk = v.vmk_recovery_code.clone();
    bad_vmk.pop();
    bad_vmk.push('A');
    assert!(VaultMasterKey::from_recovery_code(&bad_vmk).is_err());

    let p_msg = canonical_pair_message(
        &v.ticket_sha256_hex,
        &v.server_fingerprint,
        &v.client_pub,
        &v.device_name,
        v.timestamp_unix,
    );
    assert_eq!(p_msg, v.pair_message);
    assert_eq!(client_kp.sign(p_msg.as_bytes()), v.pair_client_sig);

    verify_signature(
        &server_sk.verifying_key(),
        v.pair_ack_message.as_bytes(),
        &v.pair_server_sig,
    )
    .unwrap();
    verify_signature(
        &server_sk.verifying_key(),
        v.server_challenge_message.as_bytes(),
        &v.server_challenge_sig,
    )
    .unwrap();
    assert_eq!(
        canonical_server_challenge_message(
            &v.challenge_id,
            &v.server_nonce_b64,
            &v.client_nonce_b64,
            &v.client_fingerprint,
            v.challenge_expires_at,
        ),
        v.server_challenge_message
    );
    assert_eq!(
        canonical_client_auth_message(
            &v.challenge_id,
            &v.server_nonce_b64,
            &v.client_nonce_b64,
            &v.server_fingerprint,
            &v.client_fingerprint,
        ),
        v.client_auth_message
    );
    assert_eq!(
        client_kp.sign(v.client_auth_message.as_bytes()),
        v.client_auth_sig
    );

    let blob = STANDARD_NO_PAD.decode(&v.oxe1_envelope_b64).unwrap();
    let (aad, plain) = decrypt_snapshot_blob(&vmk, &blob).unwrap();
    assert_eq!(aad.vault_id, v.vault_id);
    assert_eq!(String::from_utf8(plain).unwrap(), v.oxe1_plaintext_utf8);

    // Re-seal + re-open matches plaintext cleanly
    let salt: [u8; 32] = hex::decode(&v.oxe1_salt_hex).unwrap().try_into().unwrap();
    let nonce: [u8; 24] = hex::decode(&v.oxe1_nonce_hex).unwrap().try_into().unwrap();
    let sealed = encrypt_snapshot_blob_with_nonce(
        &vmk,
        1,
        &v.device_id,
        4,
        &v.oxe1_created_at,
        v.oxe1_plaintext_utf8.as_bytes(),
        salt,
        nonce,
    )
    .unwrap();
    let (_, decoded_back) = decrypt_snapshot_blob(&vmk, &sealed).unwrap();
    assert_eq!(
        String::from_utf8(decoded_back).unwrap(),
        v.oxe1_plaintext_utf8
    );

    // Wrong VMK raises VaultKeyMismatch
    let wrong = VaultMasterKey::from_bytes([0x99u8; 32]);
    assert!(matches!(
        decrypt_snapshot_blob(&wrong, &blob),
        Err(AppError::VaultKeyMismatch(_))
    ));
}

#[test]
fn redact_secrets_masks_all_sync_tokens_and_keys() {
    let raw = "tok=oxd_0123456789abcdef0123456789abcdef invite=oxsync-v1:eyJ2IjoxLCJ1IjoiYWFhIn0 key=oxvault-v1-GMZT-GMZT-GMZT sess=oxtok1.eyJ2IjoxfQ.abcdef123456 priv=oxd-ed25519-priv-v1:MTExMTExMTExMQ";
    let clean = redact_secrets(raw);
    assert!(!clean.contains("0123456789abcdef"));
    assert!(!clean.contains("eyJ2Ijox"));
    assert!(!clean.contains("GMZT-GMZT"));
    assert!(!clean.contains("abcdef123456"));
    assert!(!clean.contains("MTExMTExMTExMQ"));
    assert!(clean.contains("[REDACTED_TOKEN]"));
    assert!(clean.contains("[REDACTED_SYNC_INVITE]"));
    assert!(clean.contains("[REDACTED_VAULT_KEY]"));
    assert!(clean.contains("[REDACTED_SYNC_SESSION]"));
    assert!(clean.contains("[REDACTED_SYNC_PRIVKEY]"));
}

#[tokio::test]
async fn two_devices_e2ee_sync_3way_auto_merge_and_single_snapshot_retention() {
    let (url, node, srv_handle) = spawn_mock_node([0x42u8; 32], 1).await;

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let app_a = AppServices::open_file(dir_a.path().join("oxidium.db")).unwrap();
    let app_b = AppServices::open_file(dir_b.path().join("oxidium.db")).unwrap();

    // Device A authors initial academic data (Math + Physics + G1 + G2)
    let prof_a = app_a.get_active_profile().unwrap();
    let math = app_a
        .create_subject(
            CreateSubjectInput {
                profile_id: prof_a.id,
                name: "Математика".into(),
                short_name: Some("Мат".into()),
                teacher: Some("Петров".into()),
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let physics = app_a
        .create_subject(
            CreateSubjectInput {
                profile_id: prof_a.id,
                name: "Физика".into(),
                short_name: Some("Физ".into()),
                teacher: Some("Сидоров".into()),
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let d1 = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
    let g1 = app_a
        .add_grade(
            AddGradeInput {
                profile_id: prof_a.id,
                subject_id: math.id,
                date: d1,
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(3),
                weight: None,
                work_type: Some(WorkType::ClassAnswer),
                topic: Some("G1-to-delete".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let _g2 = app_a
        .add_grade(
            AddGradeInput {
                profile_id: prof_a.id,
                subject_id: math.id,
                date: d1,
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(5),
                weight: None,
                work_type: Some(WorkType::Exam),
                topic: Some("G2-keep".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    // 1. Pair Device A via one-time invite
    let invite = node.issue_invite("default", &url, [0x7Eu8; 16]);
    let cfg_a = app_a
        .pair_with_invite(&invite, "Device-A", None)
        .await
        .unwrap();
    assert_eq!(cfg_a.last_synced_seq, 1);

    // Server stores ZERO plaintext SQLite/grades; only OXE1 encrypted ciphertext
    let stored_revs = node.stored_revisions("default");
    assert_eq!(stored_revs.len(), 1);
    let raw_blob = &stored_revs[0].blob;
    assert_eq!(&raw_blob[..4], b"OXE1");
    assert!(!raw_blob.windows(15).any(|w| w == b"SQLite format 3"));
    let blob_str = String::from_utf8_lossy(raw_blob);
    assert!(!blob_str.contains("Математика"));
    assert!(!blob_str.contains("G1-to-delete"));

    // Invite ticket cannot be reused
    assert!(
        app_b
            .pair_with_invite(&invite, "Attacker", None)
            .await
            .is_err()
    );

    // 2. Connect Device B via manual SSH public key + fingerprint pinning
    let (pub_b, _) = app_b.sync_device_identity().unwrap();
    node.add_manual_device("default", "Device-B", &pub_b);
    let vault_code = app_a.sync_vault_recovery_code().unwrap().unwrap();

    // Wrong fingerprint is rejected (MITM guard)
    assert!(
        app_b
            .connect_manual_node(&url, "SHA256:WRONG", "Device-B", Some(&vault_code))
            .await
            .is_err()
    );

    // Wrong VMK raises VaultKeyMismatch when decrypting seq=1
    let wrong_code = VaultMasterKey::from_bytes([0x12u8; 32]).to_recovery_code();
    assert!(matches!(
        app_b
            .connect_manual_node(&url, &node.server_fp, "Device-B", Some(&wrong_code))
            .await,
        Err(AppError::VaultKeyMismatch(_))
    ));

    // Correct fingerprint + VMK pulls cleanly onto empty Device B without creating duplicate profiles
    let cfg_b = app_b
        .connect_manual_node(&url, &node.server_fp, "Device-B", Some(&vault_code))
        .await
        .unwrap();
    assert_eq!(cfg_b.last_synced_seq, 1);
    let profs_b = app_b.list_profiles(false).unwrap();
    assert_eq!(profs_b.len(), 1);
    assert_eq!(profs_b[0].id, prof_a.id);
    assert_eq!(app_b.list_subjects(prof_a.id, false).unwrap().len(), 2);

    // 3. Offline concurrent divergence on A and B:
    // Device A: adds G3, updates Physics teacher -> "Иванов", deletes G1
    let d2 = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    app_a
        .add_grade(
            AddGradeInput {
                profile_id: prof_a.id,
                subject_id: math.id,
                date: d2,
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(4),
                weight: None,
                work_type: Some(WorkType::Homework),
                topic: Some("G3-from-A".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    app_a
        .update_subject(
            physics.id,
            UpdateSubjectInput {
                teacher: Some("Иванов".into()),
                ..Default::default()
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    app_a.delete_grade(g1.id, RecordSource::Gui, None).unwrap();

    // Device B (before seeing A's changes!): adds G4 + Kanban Board/Column/Task T1 + Tag
    app_b
        .add_grade(
            AddGradeInput {
                profile_id: prof_a.id,
                subject_id: math.id,
                date: d2,
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(5),
                weight: None,
                work_type: Some(WorkType::Test),
                topic: Some("Алгебра коллоквиум".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let board = app_b
        .create_board(
            CreateBoardInput {
                profile_id: prof_a.id,
                name: "Сессия".into(),
                description: None,
                color: None,
                with_default_columns: Some(false),
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let col = app_b
        .create_column(
            CreateColumnInput {
                board_id: board.id,
                name: "В работе".into(),
                color: None,
                wip_limit: None,
                is_completed_column: Some(false),
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    app_b
        .create_task(
            CreateTaskInput {
                profile_id: prof_a.id,
                board_id: Some(board.id),
                column_id: Some(col.id),
                subject_id: Some(math.id),
                title: "Решить задачи".into(),
                description: Some("Билеты 1-10".into()),
                priority: Some(TaskPriority::High),
                color: None,
                start_date: None,
                due_at: None,
                notes: None,
                tag_names: vec!["важно".into()],
                badge_names: vec![],
                checklist_titles: vec!["Проверить ответы".into()],
                reminder_offsets: vec![],
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    // Device A syncs first -> seq=2
    let cfg_a2 = app_a.sync_now().await.unwrap();
    assert_eq!(cfg_a2.last_synced_seq, 2);

    // Device B syncs -> detects divergence, creates pre-sync-*.db backup, merges A+B, pushes seq=3
    let cfg_b3 = app_b.sync_now().await.unwrap();
    assert_eq!(cfg_b3.last_synced_seq, 3);
    assert!(cfg_b3.last_merge_summary.is_some());

    // Device A pulls merged seq=3
    let cfg_a3 = app_a.sync_now().await.unwrap();
    assert_eq!(cfg_a3.last_synced_seq, 3);

    // Assert BOTH devices converged identically!
    for app in [&app_a, &app_b] {
        assert!(app.db.check_integrity().unwrap());
        let grades = app
            .list_grades(prof_a.id, Some(math.id), None, None, None, None, None, None)
            .unwrap();
        let topics: Vec<_> = grades.iter().map(|g| g.topic.as_str()).collect();
        assert!(
            !topics.contains(&"G1-to-delete"),
            "tombstone must not resurrect G1"
        );
        assert!(topics.contains(&"G2-keep"));
        assert!(topics.contains(&"G3-from-A"));
        assert!(topics.contains(&"Алгебра коллоквиум"));

        let subjs = app.list_subjects(prof_a.id, false).unwrap();
        let phys = subjs.iter().find(|s| s.id == physics.id).unwrap();
        assert_eq!(phys.teacher, "Иванов");

        let tasks = app
            .list_tasks(prof_a.id, Some(board.id), None, None, false)
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Решить задачи");

        let hits = app
            .search_global("коллоквиум", &SearchFilter::default())
            .unwrap();
        assert!(
            !hits.is_empty(),
            "FTS5 search index must be rebuilt after merge"
        );
    }

    // Check pre-sync safety backup was created on B
    let has_pre_sync = std::fs::read_dir(app_b.backups_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("pre-sync-") && n.ends_with(".db"))
        });
    assert!(has_pre_sync);

    // Node stored ONLY 1 latest merged snapshot (retention_count = 1)!
    let final_revs = node.stored_revisions("default");
    assert_eq!(final_revs.len(), 1);
    assert_eq!(final_revs[0].seq, 3);

    // 4. Device revocation blocks Device B cleanly without touching Device B's local data
    let devices = app_a.list_remote_devices().await.unwrap();
    let dev_b_info = devices.iter().find(|d| d.name == "Device-B").unwrap();
    app_a.revoke_remote_device(&dev_b_info.id).await.unwrap();

    // Force Device B session cache expire by calling list_remote_devices or sync after edit
    app_b
        .create_subject(
            CreateSubjectInput {
                profile_id: prof_a.id,
                name: "Локальная копия".into(),
                short_name: None,
                teacher: None,
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    assert!(app_b.sync_now().await.is_err());
    assert!(app_b.db.check_integrity().unwrap());

    srv_handle.abort();
}
