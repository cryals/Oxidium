use crate::error::{AppError, AppResult};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use hkdf::Hkdf;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2_10::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const PROTOCOL_ID: &str = "oxsync-v1";
pub const PUB_KEY_PREFIX: &str = "oxd-ed25519 ";
pub const PRIV_KEY_PREFIX: &str = "oxd-ed25519-priv-v1:";
pub const FINGERPRINT_PREFIX: &str = "SHA256:";
pub const INVITE_PREFIX: &str = "oxsync-v1:";
pub const VAULT_CODE_PREFIX: &str = "oxvault-v1-";
pub const OXE1_MAGIC: &[u8; 4] = b"OXE1";
pub const MAX_OXE1_AAD_BYTES: usize = 4096;
pub const MAX_DECOMPRESSED_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct DeviceKeypair {
    signing: SigningKey,
    pub public_key_str: String,
    pub fingerprint: String,
}

impl DeviceKeypair {
    pub fn from_secret_bytes(secret: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&secret);
        let vk = signing.verifying_key();
        Self {
            signing,
            public_key_str: format_pub_key(&vk),
            fingerprint: key_fingerprint(&vk),
        }
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    pub fn sign(&self, message: &[u8]) -> String {
        let sig: Signature = self.signing.sign(message);
        STANDARD_NO_PAD.encode(sig.to_bytes())
    }
}

#[derive(Clone)]
pub struct VaultMasterKey {
    bytes: Zeroizing<[u8; 32]>,
}

impl VaultMasterKey {
    pub fn generate() -> Self {
        let mut buf = [0u8; 32];
        rand::rng().fill(&mut buf);
        Self {
            bytes: Zeroizing::new(buf),
        }
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub fn vault_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"oxvault-id");
        hasher.update(*self.bytes);
        let digest = hasher.finalize();
        hex::encode(&digest[..8])
    }

    pub fn to_recovery_code(&self) -> Zeroizing<String> {
        let mut hasher = Sha256::new();
        hasher.update(b"oxvault-v1");
        hasher.update(*self.bytes);
        let check = hasher.finalize();

        let mut payload = Zeroizing::new([0u8; 36]);
        payload[..32].copy_from_slice(&*self.bytes);
        payload[32..36].copy_from_slice(&check[..4]);

        let encoded = BASE32_NOPAD.encode(&*payload);
        let grouped = encoded
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("-");
        Zeroizing::new(format!("{VAULT_CODE_PREFIX}{grouped}"))
    }

    pub fn from_recovery_code(code: &str) -> AppResult<Self> {
        let clean: String = code
            .trim()
            .strip_prefix(VAULT_CODE_PREFIX)
            .ok_or_else(|| AppError::Sync("Код ключа должен начинаться с oxvault-v1-".into()))?
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .flat_map(|c| c.to_uppercase())
            .collect();

        let payload = Zeroizing::new(
            BASE32_NOPAD
                .decode(clean.as_bytes())
                .map_err(|e| AppError::Sync(format!("Некорректный формат oxvault-v1: {e}")))?,
        );
        if payload.len() != 36 {
            return Err(AppError::Sync(
                "Некорректная длина ключа хранилища (ожидается 36 байт)".into(),
            ));
        }

        let mut hasher = Sha256::new();
        hasher.update(b"oxvault-v1");
        hasher.update(&payload[..32]);
        let expected = hasher.finalize();
        if payload[32..36] != expected[..4] {
            return Err(AppError::Sync(
                "Контрольная сумма ключа хранилища не совпадает (проверьте опечатку)".into(),
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&payload[..32]);
        Ok(Self::from_bytes(key))
    }
}

#[derive(Debug, Clone)]
pub struct SyncKeyStore {
    pub data_dir: PathBuf,
}

impl SyncKeyStore {
    pub fn for_data_dir(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn device_priv_path(&self) -> PathBuf {
        self.data_dir.join("sync-device.key")
    }

    pub fn device_pub_path(&self) -> PathBuf {
        self.data_dir.join("sync-device.pub")
    }

    pub fn vault_key_path(&self) -> PathBuf {
        self.data_dir.join("sync-vault.key")
    }

    pub fn load_or_generate_device_key(&self) -> AppResult<DeviceKeypair> {
        let priv_path = self.device_priv_path();
        let pub_path = self.device_pub_path();

        if priv_path.exists() {
            let raw = std::fs::read_to_string(&priv_path)?;
            let secret = parse_priv_key(&raw)?;
            let kp = DeviceKeypair::from_secret_bytes(*secret);
            if !pub_path.exists() {
                std::fs::write(&pub_path, format!("{}\n", kp.public_key_str))?;
            }
            return Ok(kp);
        }

        let mut secret = Zeroizing::new([0u8; 32]);
        rand::rng().fill(&mut *secret);
        let kp = DeviceKeypair::from_secret_bytes(*secret);
        let encoded_priv = Zeroizing::new(format!(
            "{PRIV_KEY_PREFIX}{}\n",
            STANDARD_NO_PAD.encode(*secret)
        ));
        write_secret_file(&priv_path, encoded_priv.as_bytes())?;
        std::fs::write(&pub_path, format!("{}\n", kp.public_key_str))?;
        Ok(kp)
    }

    pub fn load_vault_key(&self) -> AppResult<Option<VaultMasterKey>> {
        let path = self.vault_key_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = Zeroizing::new(std::fs::read_to_string(&path)?);
        let vmk = VaultMasterKey::from_recovery_code(&raw)?;
        Ok(Some(vmk))
    }

    pub fn load_or_generate_vault_key(&self) -> AppResult<VaultMasterKey> {
        if let Some(existing) = self.load_vault_key()? {
            return Ok(existing);
        }
        let generated = VaultMasterKey::generate();
        self.save_vault_key(&generated)?;
        Ok(generated)
    }

    pub fn save_vault_key(&self, vmk: &VaultMasterKey) -> AppResult<()> {
        let code = vmk.to_recovery_code();
        let content = Zeroizing::new(format!("{}\n", *code));
        write_secret_file(&self.vault_key_path(), content.as_bytes())?;
        Ok(())
    }
}

pub fn format_pub_key(vk: &VerifyingKey) -> String {
    format!("{PUB_KEY_PREFIX}{}", STANDARD_NO_PAD.encode(vk.to_bytes()))
}

pub fn parse_pub_key(input: &str) -> AppResult<VerifyingKey> {
    let encoded = input
        .trim()
        .strip_prefix(PUB_KEY_PREFIX)
        .ok_or_else(|| AppError::Sync("Публичный ключ должен начинаться с 'oxd-ed25519 '".into()))?
        .trim();
    let bytes = STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|e| AppError::Sync(format!("Некорректный base64 публичного ключа: {e}")))?;
    let arr: [u8; 32] = bytes.try_into().map_err(|_| {
        AppError::Sync("Длина публичного ключа Ed25519 должна быть 32 байта".into())
    })?;
    VerifyingKey::from_bytes(&arr)
        .map_err(|e| AppError::Sync(format!("Недопустимая точка Ed25519: {e}")))
}

pub fn parse_priv_key(input: &str) -> AppResult<Zeroizing<[u8; 32]>> {
    let encoded = input
        .trim()
        .strip_prefix(PRIV_KEY_PREFIX)
        .ok_or_else(|| {
            AppError::Sync("Приватный ключ должен начинаться с oxd-ed25519-priv-v1:".into())
        })?
        .trim();
    let bytes = Zeroizing::new(
        STANDARD_NO_PAD
            .decode(encoded)
            .map_err(|e| AppError::Sync(format!("Некорректный base64 приватного ключа: {e}")))?,
    );
    if bytes.len() != 32 {
        return Err(AppError::Sync(
            "Длина приватного ключа должна быть 32 байта".into(),
        ));
    }
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn key_fingerprint(vk: &VerifyingKey) -> String {
    let digest = Sha256::digest(vk.to_bytes());
    format!("{FINGERPRINT_PREFIX}{}", STANDARD_NO_PAD.encode(digest))
}

pub fn key_fingerprint_from_pub_str(pub_str: &str) -> AppResult<String> {
    let vk = parse_pub_key(pub_str)?;
    Ok(key_fingerprint(&vk))
}

pub fn verify_signature(vk: &VerifyingKey, message: &[u8], sig_b64: &str) -> AppResult<()> {
    let raw = STANDARD_NO_PAD
        .decode(sig_b64.trim())
        .map_err(|e| AppError::Sync(format!("Некорректная base64 подпись: {e}")))?;
    let arr: [u8; 64] = raw
        .try_into()
        .map_err(|_| AppError::Sync("Подпись Ed25519 должна быть 64 байта".into()))?;
    let sig = Signature::from_bytes(&arr);
    vk.verify(message, &sig).map_err(|_| {
        AppError::Sync("Подпись Ed25519 сервера или устройства не прошла проверку".into())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn random_b64url(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand::rng().fill(buf.as_mut_slice());
    URL_SAFE_NO_PAD.encode(&buf)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitePayload {
    pub v: u8,
    pub u: String,
    pub k: String,
    pub t: String,
    pub e: i64,
}

impl InvitePayload {
    pub fn decode(code: &str) -> AppResult<Self> {
        let body = code
            .trim()
            .strip_prefix(INVITE_PREFIX)
            .ok_or_else(|| AppError::Sync("Инвайт-код должен начинаться с oxsync-v1:".into()))?;
        let raw = URL_SAFE_NO_PAD
            .decode(body.trim())
            .map_err(|e| AppError::Sync(format!("Некорректный base64url инвайта: {e}")))?;
        let payload: Self = serde_json::from_slice(&raw)
            .map_err(|e| AppError::Sync(format!("Некорректный JSON инвайта: {e}")))?;
        if payload.v != 1 {
            return Err(AppError::Sync(format!(
                "Неподдерживаемая версия инвайта: {}",
                payload.v
            )));
        }
        Ok(payload)
    }

    pub fn server_verifying_key(&self) -> AppResult<VerifyingKey> {
        let bytes = STANDARD_NO_PAD
            .decode(self.k.trim())
            .map_err(|e| AppError::Sync(format!("Некорректный ключ сервера в инвайте: {e}")))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| AppError::Sync("Публичный ключ ноды должен быть 32 байта".into()))?;
        VerifyingKey::from_bytes(&arr)
            .map_err(|e| AppError::Sync(format!("Некорректный Ed25519 ключ ноды: {e}")))
    }

    pub fn server_pub_str(&self) -> AppResult<String> {
        Ok(format_pub_key(&self.server_verifying_key()?))
    }

    pub fn server_fingerprint(&self) -> AppResult<String> {
        Ok(key_fingerprint(&self.server_verifying_key()?))
    }

    pub fn ticket_sha256_hex(&self) -> AppResult<String> {
        let raw = URL_SAFE_NO_PAD
            .decode(self.t.trim())
            .map_err(|e| AppError::Sync(format!("Некорректный тикет инвайта: {e}")))?;
        if raw.len() != 16 {
            return Err(AppError::Sync("Тикет инвайта должен быть 16 байт".into()));
        }
        Ok(sha256_hex(&raw))
    }
}

pub fn canonical_pair_message(
    ticket_sha256_hex: &str,
    server_fp: &str,
    client_pub: &str,
    device_name: &str,
    timestamp_unix: i64,
) -> String {
    format!(
        "{PROTOCOL_ID}|pair|{ticket_sha256_hex}|{server_fp}|{client_pub}|{device_name}|{timestamp_unix}"
    )
}

pub fn canonical_pair_ack_message(
    device_id: &str,
    server_fp: &str,
    client_fp: &str,
    timestamp_unix: i64,
) -> String {
    format!("{PROTOCOL_ID}|pair-ack|{device_id}|{server_fp}|{client_fp}|{timestamp_unix}")
}

pub fn canonical_server_challenge_message(
    challenge_id: &str,
    server_nonce_b64: &str,
    client_nonce_b64: &str,
    client_fp: &str,
    expires_at_unix: i64,
) -> String {
    format!(
        "{PROTOCOL_ID}|server-challenge|{challenge_id}|{server_nonce_b64}|{client_nonce_b64}|{client_fp}|{expires_at_unix}"
    )
}

pub fn canonical_client_auth_message(
    challenge_id: &str,
    server_nonce_b64: &str,
    client_nonce_b64: &str,
    server_fp: &str,
    client_fp: &str,
) -> String {
    format!(
        "{PROTOCOL_ID}|client-auth|{challenge_id}|{server_nonce_b64}|{client_nonce_b64}|{server_fp}|{client_fp}"
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Oxe1Aad {
    pub v: u8,
    pub vault_id: String,
    pub seq: i64,
    pub device_id: String,
    pub schema_version: i64,
    pub uncompressed_sha256: String,
    pub uncompressed_size: u64,
    pub created_at: String,
}

pub fn encrypt_snapshot_blob(
    vmk: &VaultMasterKey,
    seq: i64,
    device_id: &str,
    schema_version: i64,
    created_at: &str,
    sqlite_bytes: &[u8],
) -> AppResult<Vec<u8>> {
    let mut salt = [0u8; 32];
    let mut nonce = [0u8; 24];
    rand::rng().fill(&mut salt);
    rand::rng().fill(&mut nonce);
    encrypt_snapshot_blob_with_nonce(
        vmk,
        seq,
        device_id,
        schema_version,
        created_at,
        sqlite_bytes,
        salt,
        nonce,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn encrypt_snapshot_blob_with_nonce(
    vmk: &VaultMasterKey,
    seq: i64,
    device_id: &str,
    schema_version: i64,
    created_at: &str,
    sqlite_bytes: &[u8],
    salt: [u8; 32],
    nonce: [u8; 24],
) -> AppResult<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(sqlite_bytes)?;
    let gz = encoder.finish()?;

    let aad = Oxe1Aad {
        v: 1,
        vault_id: vmk.vault_id(),
        seq,
        device_id: device_id.to_string(),
        schema_version,
        uncompressed_sha256: sha256_hex(sqlite_bytes),
        uncompressed_size: sqlite_bytes.len() as u64,
        created_at: created_at.to_string(),
    };
    let aad_bytes = serde_json::to_vec(&aad)?;
    if aad_bytes.is_empty() || aad_bytes.len() > MAX_OXE1_AAD_BYTES {
        return Err(AppError::Sync("Недопустимый размер заголовка OXE1".into()));
    }
    let aad_len_be = (aad_bytes.len() as u32).to_be_bytes();

    let hk = Hkdf::<Sha256>::new(Some(&salt), vmk.as_bytes());
    let mut enc_key = Zeroizing::new([0u8; 32]);
    hk.expand(b"oxsync-v1-snapshot-xchacha20poly1305", &mut *enc_key)
        .map_err(|_| AppError::Sync("HKDF ошибка вывода ключа".into()))?;

    let mut aead_aad = Vec::with_capacity(8 + aad_bytes.len());
    aead_aad.extend_from_slice(OXE1_MAGIC);
    aead_aad.extend_from_slice(&aad_len_be);
    aead_aad.extend_from_slice(&aad_bytes);

    let cipher = XChaCha20Poly1305::new(Key::from_slice(&*enc_key));
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &gz,
                aad: &aead_aad,
            },
        )
        .map_err(|_| AppError::Sync("Ошибка AEAD шифрования XChaCha20-Poly1305".into()))?;

    let mut out = Vec::with_capacity(8 + aad_bytes.len() + 32 + 24 + ct.len());
    out.extend_from_slice(OXE1_MAGIC);
    out.extend_from_slice(&aad_len_be);
    out.extend_from_slice(&aad_bytes);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt_snapshot_blob(vmk: &VaultMasterKey, blob: &[u8]) -> AppResult<(Oxe1Aad, Vec<u8>)> {
    if blob.len() < 8 || &blob[..4] != OXE1_MAGIC {
        return Err(AppError::Sync("Некорректный заголовок снимка OXE1".into()));
    }
    let aad_len = u32::from_be_bytes([blob[4], blob[5], blob[6], blob[7]]) as usize;
    if aad_len == 0 || aad_len > MAX_OXE1_AAD_BYTES {
        return Err(AppError::Sync("Недопустимая длина AAD в OXE1".into()));
    }
    let header_end = 8 + aad_len;
    if blob.len() < header_end + 32 + 24 + 16 {
        return Err(AppError::Sync(
            "Повреждённый или обрезанный OXE1-блоб".into(),
        ));
    }

    let aad: Oxe1Aad = serde_json::from_slice(&blob[8..header_end])?;
    if aad.v != 1 {
        return Err(AppError::Sync(format!(
            "Неподдерживаемая версия OXE1: {}",
            aad.v
        )));
    }
    if aad.vault_id != vmk.vault_id() {
        return Err(AppError::VaultKeyMismatch(format!(
            "Ключ хранилища ({}) не подходит к снимку на сервере ({})",
            vmk.vault_id(),
            aad.vault_id
        )));
    }

    let aead_aad = &blob[..header_end];
    let salt = &blob[header_end..header_end + 32];
    let nonce = &blob[header_end + 32..header_end + 56];
    let ct = &blob[header_end + 56..];

    let hk = Hkdf::<Sha256>::new(Some(salt), vmk.as_bytes());
    let mut enc_key = Zeroizing::new([0u8; 32]);
    hk.expand(b"oxsync-v1-snapshot-xchacha20poly1305", &mut *enc_key)
        .map_err(|_| AppError::Sync("HKDF ошибка вывода ключа".into()))?;

    let cipher = XChaCha20Poly1305::new(Key::from_slice(&*enc_key));
    let gz = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: aead_aad,
            },
        )
        .map_err(|_| {
            AppError::VaultKeyMismatch(
                "Не удалось расшифровать снимок: проверьте ключ oxvault-v1-... или целостность"
                    .into(),
            )
        })?;

    // Guard against gzip bombs on decompression
    let dec = GzDecoder::new(&gz[..]);
    let mut bounded = dec.take((MAX_DECOMPRESSED_SNAPSHOT_BYTES + 1) as u64);
    let mut plain = Vec::new();
    bounded.read_to_end(&mut plain)?;
    if plain.len() > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
        return Err(AppError::Sync(
            "Распакованный снимок превышает безопасный лимит 128 МБ".into(),
        ));
    }
    if sha256_hex(&plain) != aad.uncompressed_sha256 {
        return Err(AppError::Sync(
            "Контрольная сумма SHA-256 распакованной базы не совпала".into(),
        ));
    }
    Ok((aad, plain))
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
