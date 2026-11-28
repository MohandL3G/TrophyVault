use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::RwLock;
use trophyvault_core::db::{
    apply_metadata_batch, get_all_metadata, is_db_empty, load_library_database_from_sqlite,
    migrate_json_to_sqlite_if_needed, open_or_create_db, purge_sqlite_database,
    save_library_database_to_sqlite,
};
use trophyvault_core::storage::{DatabasePreferences, LibraryDatabase};

pub const DEFAULT_STEAM_ID: &str = "76561198423209668";
pub const DEFAULT_STEAM_SYNC_COOLDOWN_SECS: u64 = 300;
pub const DEFAULT_STEAM_API_BASE_URL: &str = "https://api.steampowered.com";

pub mod meta_keys {
    pub const STEAM_API_KEY: &str = "steam_api_key";
    pub const STEAM_ID: &str = "steam_id";
    pub const STEAM_SYNC_COOLDOWN_SECS: &str = "steam_sync_cooldown_secs";
    pub const CLOUDREDIRECT_API_URL: &str = "cloudredirect_api_url";
    pub const CLOUDREDIRECT_API_KEY: &str = "cloudredirect_api_key";
    pub const LAST_ENV_STEAM_API_KEY: &str = "last_env_steam_api_key";
    pub const LAST_ENV_STEAM_ID: &str = "last_env_steam_id";
    pub const LAST_ENV_STEAM_SYNC_COOLDOWN_SECS: &str = "last_env_steam_sync_cooldown_secs";
    pub const LAST_ENV_CLOUDREDIRECT_API_URL: &str = "last_env_cloudredirect_api_url";
    pub const LAST_ENV_CLOUDREDIRECT_API_KEY: &str = "last_env_cloudredirect_api_key";
    pub const LAST_STEAM_SYNC_AT: &str = "last_steam_sync_at";
    pub const LAST_STEAM_SYNC_STATUS: &str = "last_steam_sync_status";
    pub const LAST_STEAM_SYNC_MESSAGE: &str = "last_steam_sync_message";
    pub const STEAM_PERSONA_NAME: &str = "steam_persona_name";
    pub const STEAM_AVATAR_URL: &str = "steam_avatar_url";
}

/// Returns the current Unix timestamp in seconds.
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolves a DB path to its SQLite `.db` path if it has `.json` extension.
pub fn resolve_sqlite_path(path: &Path) -> PathBuf {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        path.with_extension("db")
    } else {
        path.to_path_buf()
    }
}

/// Finds a potential JSON database file to migrate from if SQLite database is empty/new.
pub fn find_migration_json_candidate(original_path: &Path, sqlite_path: &Path) -> Option<PathBuf> {
    if original_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
        && original_path.exists()
    {
        return Some(original_path.to_path_buf());
    }

    let candidate = sqlite_path.with_extension("json");
    if candidate.exists() && candidate != sqlite_path {
        return Some(candidate);
    }

    if let Some(parent) = sqlite_path.parent() {
        let parent_json = if parent.as_os_str().is_empty() {
            PathBuf::from("library_database.json")
        } else {
            parent.join("library_database.json")
        };
        if parent_json.exists() && parent_json != sqlite_path {
            return Some(parent_json);
        }
    }

    None
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub api_key: String,
    pub admin_password: String,
    pub username: Option<String>,
    pub slogan: Option<String>,
    pub db_path: PathBuf,
    pub avatar_path: PathBuf,
    pub static_dir: PathBuf,
    // Setting 1: Steam Web API
    pub steam_api_key: Option<String>,
    pub steam_id: String,
    pub steam_sync_cooldown_secs: u64,
    pub steam_api_base_url: String,
    // Setting 2: CloudRedirect API Placeholder
    pub cloudredirect_api_url: Option<String>,
    pub cloudredirect_api_key: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 5050,
            api_key: "trophyvault-dev-token".to_string(),
            admin_password: "admin-pass-change-me".to_string(),
            username: None,
            slogan: None,
            db_path: PathBuf::from("./data/trophyvault.db"),
            avatar_path: PathBuf::from("./data/avatar.png"),
            static_dir: PathBuf::from("static"),
            steam_api_key: None,
            steam_id: DEFAULT_STEAM_ID.to_string(),
            steam_sync_cooldown_secs: DEFAULT_STEAM_SYNC_COOLDOWN_SECS,
            steam_api_base_url: DEFAULT_STEAM_API_BASE_URL.to_string(),
            cloudredirect_api_url: None,
            cloudredirect_api_key: None,
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(5050);

        let api_key = std::env::var("API_KEY")
            .unwrap_or_else(|_| "trophyvault-dev-token".to_string());

        let admin_password = match std::env::var("ADMIN_PASSWORD") {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                panic!("FATAL: ADMIN_PASSWORD environment variable is required and must not be empty. TrophyVault server requires an Admin Password to run.");
            }
        };

        let username = non_empty_env("PROFILE_USERNAME").or_else(|| non_empty_env("USERNAME"));
        let slogan = non_empty_env("PROFILE_SLOGAN").or_else(|| non_empty_env("SLOGAN"));

        let (db_path, candidate_json) = match std::env::var("DATABASE_PATH") {
            Ok(p) => {
                let raw_path = PathBuf::from(p);
                if raw_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
                {
                    let sqlite_path = raw_path.with_extension("db");
                    let json_opt = if raw_path.exists() {
                        Some(raw_path)
                    } else {
                        None
                    };
                    (sqlite_path, json_opt)
                } else {
                    let json_candidate = raw_path.with_extension("json");
                    let json_opt = if json_candidate.exists() {
                        Some(json_candidate)
                    } else if let Some(parent) = raw_path.parent() {
                        let parent_json = if parent.as_os_str().is_empty() {
                            PathBuf::from("library_database.json")
                        } else {
                            parent.join("library_database.json")
                        };
                        if parent_json.exists() {
                            Some(parent_json)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    (raw_path, json_opt)
                }
            }
            Err(_) => {
                let default_container_data = PathBuf::from("/data");
                if default_container_data.exists() && default_container_data.is_dir() {
                    let db = default_container_data.join("trophyvault.db");
                    let json = default_container_data.join("library_database.json");
                    (db, if json.exists() { Some(json) } else { None })
                } else {
                    let db = PathBuf::from("./data/trophyvault.db");
                    let json = PathBuf::from("./data/library_database.json");
                    (db, if json.exists() { Some(json) } else { None })
                }
            }
        };

        if let Some(ref json_path) = candidate_json {
            if let Err(e) = migrate_json_to_sqlite_if_needed(json_path, &db_path) {
                tracing::warn!(
                    "Migration from JSON ({}) to SQLite ({}) failed: {}",
                    json_path.display(),
                    db_path.display(),
                    e
                );
            }
        }

        let avatar_path = match std::env::var("AVATAR_PATH") {
            Ok(p) => PathBuf::from(p),
            Err(_) => {
                let default_container_data = PathBuf::from("/data");
                if default_container_data.exists() && default_container_data.is_dir() {
                    default_container_data.join("avatar.png")
                } else if let Some(parent) = db_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        parent.join("avatar.png")
                    } else {
                        PathBuf::from("./data/avatar.png")
                    }
                } else {
                    PathBuf::from("./data/avatar.png")
                }
            }
        };

        let static_dir = match std::env::var("STATIC_DIR") {
            Ok(s) => PathBuf::from(s),
            Err(_) => {
                if PathBuf::from("./static").exists() {
                    PathBuf::from("./static")
                } else if PathBuf::from("./crates/server/static").exists() {
                    PathBuf::from("./crates/server/static")
                } else {
                    PathBuf::from("static")
                }
            }
        };

        let steam_api_key = non_empty_env("STEAM_API_KEY");
        let steam_id = non_empty_env("STEAM_ID").unwrap_or_else(|| DEFAULT_STEAM_ID.to_string());
        let steam_sync_cooldown_secs = std::env::var("STEAM_SYNC_COOLDOWN_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_STEAM_SYNC_COOLDOWN_SECS);
        let steam_api_base_url = non_empty_env("STEAM_API_BASE_URL")
            .unwrap_or_else(|| DEFAULT_STEAM_API_BASE_URL.to_string());

        let cloudredirect_api_url = non_empty_env("CLOUDREDIRECT_API_URL");
        let cloudredirect_api_key = non_empty_env("CLOUDREDIRECT_API_KEY");

        Self {
            port,
            api_key,
            admin_password,
            username,
            slogan,
            db_path,
            avatar_path,
            static_dir,
            steam_api_key,
            steam_id,
            steam_sync_cooldown_secs,
            steam_api_base_url,
            cloudredirect_api_url,
            cloudredirect_api_key,
        }
    }
}

/// Active runtime settings spanning Setting 1 (Steam API) and Setting 2 (CloudRedirect API).
#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub steam_api_key: Option<String>,
    pub steam_id: String,
    pub steam_sync_cooldown_secs: u64,
    pub steam_api_base_url: String,
    pub cloudredirect_api_url: Option<String>,
    pub cloudredirect_api_key: Option<String>,
}

impl RuntimeSettings {
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            steam_api_key: config.steam_api_key.clone(),
            steam_id: config.steam_id.clone(),
            steam_sync_cooldown_secs: config.steam_sync_cooldown_secs,
            steam_api_base_url: config.steam_api_base_url.clone(),
            cloudredirect_api_url: config.cloudredirect_api_url.clone(),
            cloudredirect_api_key: config.cloudredirect_api_key.clone(),
        }
    }
}

pub struct AppState {
    pub database: LibraryDatabase,
    pub last_modified: SystemTime,
    pub api_key: String,
    pub admin_password: String,
    pub username_override: Option<String>,
    pub slogan_override: Option<String>,
    pub db_path: PathBuf,
    pub avatar_path: PathBuf,
    pub update_tx: tokio::sync::broadcast::Sender<()>,
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub settings: RuntimeSettings,
    pub last_steam_sync_at: Option<u64>,
    pub last_steam_sync_status: Option<String>,
    pub last_steam_sync_message: Option<String>,
    pub steam_persona_name: Option<String>,
    pub steam_avatar_url: Option<String>,
    pub sync_in_progress: Arc<AtomicBool>,
    pub sync_lock: Arc<tokio::sync::Mutex<()>>,
}

pub type SharedState = Arc<RwLock<AppState>>;

fn resolve_setting_with_env_tracking(
    meta: &HashMap<String, String>,
    value_key: &str,
    last_env_key: &str,
    env_val: Option<&str>,
) -> (Option<String>, Option<String>) {
    let env_clean = env_val.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let stored_val = meta.get(value_key).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let last_env = meta.get(last_env_key).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    match env_clean {
        Some(ev) if last_env.as_deref() != Some(ev.as_str()) => {
            // Env changed (or newly set): apply env value and remember it
            (Some(ev.clone()), Some(ev))
        }
        Some(ev) => {
            // Env unchanged since last boot: keep DB value (preserves UI edits), or fallback to env
            (stored_val.or(Some(ev.clone())), Some(ev))
        }
        None => (stored_val, None),
    }
}

impl AppState {
    pub fn new(config: &ServerConfig) -> Self {
        let db_path = resolve_sqlite_path(&config.db_path);
        if let Some(json_path) = find_migration_json_candidate(&config.db_path, &db_path) {
            if let Err(e) = migrate_json_to_sqlite_if_needed(&json_path, &db_path) {
                tracing::warn!(
                    "Migration from JSON ({}) to SQLite ({}) failed: {}",
                    json_path.display(),
                    db_path.display(),
                    e
                );
            }
        }

        let mut conn = open_or_create_db(&db_path)
            .unwrap_or_else(|e| panic!("Failed to open or create SQLite DB at {}: {}", db_path.display(), e));
        let (database, last_modified) = Self::load_initial_db(&mut conn, &db_path);
        let meta = get_all_metadata(&conn).unwrap_or_default();

        let (steam_api_key, tracked_env_key) = resolve_setting_with_env_tracking(
            &meta,
            meta_keys::STEAM_API_KEY,
            meta_keys::LAST_ENV_STEAM_API_KEY,
            config.steam_api_key.as_deref(),
        );
        let (steam_id_opt, tracked_env_steam_id) = resolve_setting_with_env_tracking(
            &meta,
            meta_keys::STEAM_ID,
            meta_keys::LAST_ENV_STEAM_ID,
            Some(config.steam_id.as_str()),
        );
        let steam_id = steam_id_opt.unwrap_or_else(|| DEFAULT_STEAM_ID.to_string());

        let env_cooldown_str = config.steam_sync_cooldown_secs.to_string();
        let (cooldown_opt, tracked_env_cooldown) = resolve_setting_with_env_tracking(
            &meta,
            meta_keys::STEAM_SYNC_COOLDOWN_SECS,
            meta_keys::LAST_ENV_STEAM_SYNC_COOLDOWN_SECS,
            Some(env_cooldown_str.as_str()),
        );
        let steam_sync_cooldown_secs = cooldown_opt
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(config.steam_sync_cooldown_secs);

        let (cloudredirect_api_url, tracked_env_cr_url) = resolve_setting_with_env_tracking(
            &meta,
            meta_keys::CLOUDREDIRECT_API_URL,
            meta_keys::LAST_ENV_CLOUDREDIRECT_API_URL,
            config.cloudredirect_api_url.as_deref(),
        );
        let (cloudredirect_api_key, tracked_env_cr_key) = resolve_setting_with_env_tracking(
            &meta,
            meta_keys::CLOUDREDIRECT_API_KEY,
            meta_keys::LAST_ENV_CLOUDREDIRECT_API_KEY,
            config.cloudredirect_api_key.as_deref(),
        );

        let _ = apply_metadata_batch(
            &mut conn,
            &[
                (meta_keys::STEAM_API_KEY, steam_api_key.as_deref()),
                (meta_keys::LAST_ENV_STEAM_API_KEY, tracked_env_key.as_deref()),
                (meta_keys::STEAM_ID, Some(steam_id.as_str())),
                (meta_keys::LAST_ENV_STEAM_ID, tracked_env_steam_id.as_deref()),
                (
                    meta_keys::STEAM_SYNC_COOLDOWN_SECS,
                    Some(steam_sync_cooldown_secs.to_string().as_str()),
                ),
                (
                    meta_keys::LAST_ENV_STEAM_SYNC_COOLDOWN_SECS,
                    tracked_env_cooldown.as_deref(),
                ),
                (
                    meta_keys::CLOUDREDIRECT_API_URL,
                    cloudredirect_api_url.as_deref(),
                ),
                (
                    meta_keys::LAST_ENV_CLOUDREDIRECT_API_URL,
                    tracked_env_cr_url.as_deref(),
                ),
                (
                    meta_keys::CLOUDREDIRECT_API_KEY,
                    cloudredirect_api_key.as_deref(),
                ),
                (
                    meta_keys::LAST_ENV_CLOUDREDIRECT_API_KEY,
                    tracked_env_cr_key.as_deref(),
                ),
            ],
        );

        let last_steam_sync_at = meta
            .get(meta_keys::LAST_STEAM_SYNC_AT)
            .and_then(|v| v.parse::<u64>().ok());
        let last_steam_sync_status = meta.get(meta_keys::LAST_STEAM_SYNC_STATUS).cloned();
        let last_steam_sync_message = meta.get(meta_keys::LAST_STEAM_SYNC_MESSAGE).cloned();
        let steam_persona_name = meta.get(meta_keys::STEAM_PERSONA_NAME).cloned();
        let steam_avatar_url = meta.get(meta_keys::STEAM_AVATAR_URL).cloned();

        let (update_tx, _) = tokio::sync::broadcast::channel(100);

        Self {
            database,
            last_modified,
            api_key: config.api_key.clone(),
            admin_password: config.admin_password.clone(),
            username_override: config.username.clone(),
            slogan_override: config.slogan.clone(),
            db_path,
            avatar_path: config.avatar_path.clone(),
            update_tx,
            conn: Arc::new(Mutex::new(conn)),
            settings: RuntimeSettings {
                steam_api_key,
                steam_id,
                steam_sync_cooldown_secs,
                steam_api_base_url: config.steam_api_base_url.clone(),
                cloudredirect_api_url,
                cloudredirect_api_key,
            },
            last_steam_sync_at,
            last_steam_sync_status,
            last_steam_sync_message,
            steam_persona_name,
            steam_avatar_url,
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_database(config: &ServerConfig, database: LibraryDatabase) -> Self {
        let (update_tx, _) = tokio::sync::broadcast::channel(100);
        let db_path = resolve_sqlite_path(&config.db_path);
        let mut conn = open_or_create_db(&db_path)
            .unwrap_or_else(|e| panic!("Failed to open or create SQLite DB at {}: {}", db_path.display(), e));
        let _ = save_library_database_to_sqlite(&mut conn, &database);

        Self {
            database,
            last_modified: SystemTime::now(),
            api_key: config.api_key.clone(),
            admin_password: config.admin_password.clone(),
            username_override: config.username.clone(),
            slogan_override: config.slogan.clone(),
            db_path,
            avatar_path: config.avatar_path.clone(),
            update_tx,
            conn: Arc::new(Mutex::new(conn)),
            settings: RuntimeSettings::from_config(config),
            last_steam_sync_at: None,
            last_steam_sync_status: None,
            last_steam_sync_message: None,
            steam_persona_name: None,
            steam_avatar_url: None,
            sync_in_progress: Arc::new(AtomicBool::new(false)),
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Broadcasts an update notification to all connected SSE clients.
    pub fn broadcast_update(&self) {
        let _ = self.update_tx.send(());
    }

    /// Returns remaining Steam sync cooldown seconds (0 if sync is allowed now).
    pub fn cooldown_remaining_secs(&self, now: u64) -> u64 {
        match self.last_steam_sync_at {
            Some(last) => {
                let elapsed = now.saturating_sub(last);
                self.settings.steam_sync_cooldown_secs.saturating_sub(elapsed)
            }
            None => 0,
        }
    }

    /// Persists the current Two Settings values into the SQLite `metadata` table.
    pub fn persist_settings_to_db(&self) -> Result<(), std::io::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let cooldown_str = self.settings.steam_sync_cooldown_secs.to_string();
        apply_metadata_batch(
            &mut conn,
            &[
                (meta_keys::STEAM_API_KEY, self.settings.steam_api_key.as_deref()),
                (meta_keys::STEAM_ID, Some(self.settings.steam_id.as_str())),
                (meta_keys::STEAM_SYNC_COOLDOWN_SECS, Some(cooldown_str.as_str())),
                (
                    meta_keys::CLOUDREDIRECT_API_URL,
                    self.settings.cloudredirect_api_url.as_deref(),
                ),
                (
                    meta_keys::CLOUDREDIRECT_API_KEY,
                    self.settings.cloudredirect_api_key.as_deref(),
                ),
            ],
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    /// Persists Steam sync telemetry and profile metadata into SQLite `metadata` table.
    pub fn persist_sync_metadata(&self) -> Result<(), std::io::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let sync_at_str = self.last_steam_sync_at.map(|v| v.to_string());
        apply_metadata_batch(
            &mut conn,
            &[
                (meta_keys::LAST_STEAM_SYNC_AT, sync_at_str.as_deref()),
                (
                    meta_keys::LAST_STEAM_SYNC_STATUS,
                    self.last_steam_sync_status.as_deref(),
                ),
                (
                    meta_keys::LAST_STEAM_SYNC_MESSAGE,
                    self.last_steam_sync_message.as_deref(),
                ),
                (
                    meta_keys::STEAM_PERSONA_NAME,
                    self.steam_persona_name.as_deref(),
                ),
                (
                    meta_keys::STEAM_AVATAR_URL,
                    self.steam_avatar_url.as_deref(),
                ),
            ],
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    /// Verifies if a request is authorized with admin password via:
    /// 1. Bearer token matching `admin_password`.
    /// 2. Header `X-Admin-Password` matching `admin_password`.
    /// CRITICAL: It must NOT accept api_key!
    pub fn is_admin_password_authorized(&self, headers: &axum::http::HeaderMap) -> bool {
        if let Some(auth_header) = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if auth_header.starts_with("Bearer ") {
                let token = auth_header.trim_start_matches("Bearer ").trim();
                if token == self.admin_password {
                    return true;
                }
            }
        }

        if let Some(admin_hdr) = headers
            .get("X-Admin-Password")
            .or_else(|| headers.get("x-admin-password"))
            .and_then(|v| v.to_str().ok())
        {
            let pass = admin_hdr.trim();
            if pass == self.admin_password {
                return true;
            }
        }

        false
    }

    /// Verifies if a request is authorized with admin privileges via:
    /// 1. Bearer token matching `api_key` or `admin_password`.
    /// 2. Header `X-Admin-Password` matching `admin_password` or `api_key`.
    pub fn is_admin_authorized(&self, headers: &axum::http::HeaderMap) -> bool {
        if let Some(auth_header) = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        {
            if auth_header.starts_with("Bearer ") {
                let token = auth_header.trim_start_matches("Bearer ").trim();
                if token == self.api_key || token == self.admin_password {
                    return true;
                }
            }
        }

        if let Some(admin_hdr) = headers
            .get("X-Admin-Password")
            .or_else(|| headers.get("x-admin-password"))
            .and_then(|v| v.to_str().ok())
        {
            let pass = admin_hdr.trim();
            if pass == self.admin_password || pass == self.api_key {
                return true;
            }
        }

        false
    }

    fn load_initial_db(conn: &mut rusqlite::Connection, path: &Path) -> (LibraryDatabase, SystemTime) {
        let is_empty = is_db_empty(conn).unwrap_or(true);
        if !is_empty {
            if let Ok(db) = load_library_database_from_sqlite(conn) {
                tracing::info!(
                    "Loaded existing library database from SQLite at {} ({} games)",
                    path.display(),
                    db.games.len()
                );
                let mtime = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                return (db, mtime);
            } else {
                tracing::warn!(
                    "Failed to deserialize SQLite database at {}, initializing default",
                    path.display()
                );
            }
        }

        tracing::info!(
            "No existing data in SQLite database at {}, starting with default",
            path.display()
        );
        let default_db = LibraryDatabase {
            version: 1,
            last_updated: current_timestamp(),
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };
        let _ = save_library_database_to_sqlite(conn, &default_db);
        (default_db, SystemTime::now())
    }

    /// Atomically persists the database snapshot to SQLite within a transaction.
    pub fn save_db_atomic(&self, db: &LibraryDatabase) -> Result<(), std::io::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        save_library_database_to_sqlite(&mut *conn, db)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(())
    }

    /// Purges all games from the SQLite database within a transaction.
    pub fn purge_db(&self) -> Result<(), std::io::Error> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        purge_sqlite_database(&mut *conn)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(())
    }
}
