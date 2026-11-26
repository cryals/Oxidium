use rand::RngExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

use crate::error::AppResult;

/// Manages the local MCP HTTP Bearer token in a dedicated secret file outside SQLite and Git.
/// Never logged or written into `activity_log`.
#[derive(Debug, Clone)]
pub struct TokenStore {
    secret_path: PathBuf,
}

impl TokenStore {
    pub fn for_db_dir(data_dir: &Path) -> Self {
        Self {
            secret_path: data_dir.join("mcp-token.secret"),
        }
    }

    pub fn secret_path(&self) -> &Path {
        &self.secret_path
    }

    /// Ensures a cryptographically random 256-bit Bearer token exists and returns it.
    pub fn load_or_create(&self) -> AppResult<String> {
        if self.secret_path.exists() {
            let content = std::fs::read_to_string(&self.secret_path)?;
            let trimmed = content.trim();
            if trimmed.len() >= 32 {
                return Ok(trimmed.to_string());
            }
        }
        self.rotate()
    }

    /// Generates a fresh cryptographically random Bearer token, overwriting any previous token.
    pub fn rotate(&self) -> AppResult<String> {
        if let Some(parent) = self.secret_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        let token = format!("oxd_{}", hex::encode(bytes));
        std::fs::write(&self.secret_path, &token)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.secret_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(token)
    }

    /// Revokes the active token (removes the file so HTTP auth rejects all previous tokens).
    pub fn revoke(&self) -> AppResult<()> {
        if self.secret_path.exists() {
            std::fs::remove_file(&self.secret_path)?;
        }
        Ok(())
    }

    /// Constant-time SHA-256 verification of a supplied Bearer token against the stored secret.
    pub fn verify_bearer(&self, candidate: &str) -> bool {
        let Ok(stored) = std::fs::read_to_string(&self.secret_path) else {
            return false;
        };
        let stored_trimmed = stored.trim();
        if stored_trimmed.is_empty() || candidate.is_empty() {
            return false;
        }
        let expected_hash = Sha256::digest(stored_trimmed.as_bytes());
        let provided_hash = Sha256::digest(candidate.trim().as_bytes());
        bool::from(expected_hash.ct_eq(&provided_hash))
    }

    /// Returns a masked display string (e.g., `oxd_a1b2••••••••f9e0`) safe for UI display.
    pub fn masked_preview(&self) -> Option<String> {
        let content = std::fs::read_to_string(&self.secret_path).ok()?;
        let t = content.trim();
        if t.len() < 12 {
            return None;
        }
        Some(format!("{}••••••••{}", &t[..8], &t[t.len() - 4..]))
    }
}

/// Redacts any occurrence of Oxidium Bearer tokens from log/audit strings.
pub fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find("oxd_") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 4..];
        let hex_len = after
            .bytes()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        if hex_len >= 16 {
            out.push_str("[REDACTED_TOKEN]");
            rest = &after[hex_len..];
        } else {
            out.push_str("oxd_");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}
