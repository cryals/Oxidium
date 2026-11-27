pub mod error;
pub mod i18n;
pub mod secret;
pub mod services;
pub mod sync;
pub mod sync_client;
pub mod sync_crypto;
pub mod undo;
pub mod updater;

pub use error::{AppError, AppResult};
pub use secret::{TokenStore, redact_secrets};
pub use services::*;
pub use sync::{SyncCoordinator, SyncEvent};
pub use sync_client::{SyncPairingSummary, run_remote_sync_worker};
pub use sync_crypto::{SyncKeyStore, VaultMasterKey};
pub use undo::{UndoCommand, UndoStack};
pub use updater::{
    CURRENT_APP_VERSION, GITHUB_RELEASES_API, GITHUB_RELEASES_PAGE, ReleaseAssetInfo, UpdateInfo,
    check_for_updates, check_for_updates_from_url, download_and_verify_update, format_bytes,
    is_newer_version, is_safe_release_html_url, is_windows_self_updating_installer,
    launch_update_artifact, open_url_in_browser, parse_sha256sums, select_platform_asset,
    select_platform_asset_for,
};
