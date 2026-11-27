pub mod journal;
pub mod kanban;
pub mod ops;
pub mod profiles_subjects;
pub mod statistics;

pub use journal::*;
pub use kanban::*;
pub use ops::*;
pub use profiles_subjects::*;
pub use statistics::*;

use directories::ProjectDirs;
use journal_domain::{AppSettings, Profile, RecordSource};
use journal_storage::Database;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::error::AppResult;
use crate::secret::TokenStore;
use crate::sync::SyncCoordinator;
use crate::sync_client::RemoteSyncRuntime;
use crate::undo::UndoStack;

/// Shared Application Services facade used identically by Slint GUI and MCP Server.
pub struct AppServices {
    pub db: Arc<Database>,
    pub data_dir: PathBuf,
    pub sync: Arc<SyncCoordinator>,
    pub remote_sync: Arc<RemoteSyncRuntime>,
    pub tokens: TokenStore,
    pub undo: UndoStack,
    pub(crate) stats_cache: Mutex<HashMap<String, (i64, StatisticsReport)>>,
}

impl AppServices {
    pub fn default_data_dir() -> PathBuf {
        if let Ok(env_path) = std::env::var("OXIDIUM_DB_PATH")
            && !env_path.trim().is_empty()
        {
            let p = PathBuf::from(env_path);
            return p
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
        }
        ProjectDirs::from("io", "oxidium", "Oxidium")
            .map(|d| d.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./oxidium-data"))
    }

    pub fn default_db_path() -> PathBuf {
        if let Ok(env_path) = std::env::var("OXIDIUM_DB_PATH")
            && !env_path.trim().is_empty()
        {
            return PathBuf::from(env_path);
        }
        Self::default_data_dir().join("oxidium.db")
    }

    pub fn open_file(db_path: impl AsRef<Path>) -> AppResult<Arc<Self>> {
        let db_path = db_path.as_ref().to_path_buf();
        let data_dir = db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let db = Arc::new(Database::open(&db_path)?);
        Self::from_db(db, data_dir)
    }

    pub fn open_in_memory(temp_dir: PathBuf) -> AppResult<Arc<Self>> {
        let db = Arc::new(Database::open_in_memory()?);
        Self::from_db(db, temp_dir)
    }

    fn from_db(db: Arc<Database>, data_dir: PathBuf) -> AppResult<Arc<Self>> {
        let sync = Arc::new(SyncCoordinator::new(Arc::clone(&db)));
        let tokens = TokenStore::for_db_dir(&data_dir);
        let services = Arc::new(Self {
            db,
            data_dir,
            sync,
            remote_sync: Arc::new(RemoteSyncRuntime::default()),
            tokens,
            undo: UndoStack::default(),
            stats_cache: Mutex::new(HashMap::new()),
        });
        services.ensure_initialized()?;
        Ok(services)
    }

    /// Ensures the application has at least one empty user profile on brand-new start
    /// (zero fake subjects/grades/tasks — strictly an empty workspace ready for user input).
    fn ensure_initialized(&self) -> AppResult<()> {
        let profiles = self.db.list_profiles(false)?;
        if profiles.is_empty() {
            let settings = self.db.load_settings()?;
            let default_name = match settings.language {
                journal_domain::Language::Ru => "Основной профиль",
                journal_domain::Language::En => "Main Profile",
            };
            let created = self.create_profile(
                CreateProfileInput {
                    name: default_name.into(),
                    icon: Some("school".into()),
                    color: Some("#445E91".into()),
                    institution: None,
                    group_name: None,
                    academic_year: None,
                    description: None,
                },
                RecordSource::Gui,
                None,
            )?;
            self.set_active_profile(created.id)?;
        } else {
            let settings = self.db.load_settings()?;
            let active_valid = settings
                .active_profile_id
                .is_some_and(|aid| profiles.iter().any(|p| p.id == aid));
            if !active_valid {
                self.set_active_profile(profiles[0].id)?;
            }
        }
        Ok(())
    }

    pub fn get_settings(&self) -> AppResult<AppSettings> {
        Ok(self.db.load_settings()?)
    }

    pub fn update_settings(&self, settings: &AppSettings, source: RecordSource) -> AppResult<()> {
        self.db.save_settings(settings, source)?;
        self.sync.notify_local_change(source, "settings");
        Ok(())
    }

    pub fn get_active_profile(&self) -> AppResult<Profile> {
        let settings = self.db.load_settings()?;
        if let Some(id) = settings.active_profile_id
            && let Some(p) = self.db.get_profile(id)?
            && !p.is_archived
        {
            return Ok(p);
        }
        let profiles = self.db.list_profiles(false)?;
        if let Some(first) = profiles.into_iter().next() {
            self.set_active_profile(first.id)?;
            return Ok(first);
        }
        self.ensure_initialized()?;
        self.get_active_profile()
    }

    pub fn set_active_profile(&self, profile_id: Uuid) -> AppResult<Profile> {
        let profile = self.db.get_profile(profile_id)?.ok_or_else(|| {
            journal_domain::DomainError::NotFound {
                entity: "profile".into(),
                id: profile_id.to_string(),
            }
        })?;
        let mut settings = self.db.load_settings()?;
        settings.active_profile_id = Some(profile.id);
        self.db.save_settings(&settings, RecordSource::Gui)?;
        self.sync
            .notify_local_change(RecordSource::Gui, "active_profile");
        Ok(profile)
    }

    pub fn invalidate_stats_cache(&self) {
        if let Ok(mut guard) = self.stats_cache.lock() {
            guard.clear();
        }
    }
}
