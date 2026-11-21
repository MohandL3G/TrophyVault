use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::RwLock;
use trophyvault_core::db::{
    is_db_empty, load_library_database_from_sqlite, migrate_json_to_sqlite_if_needed,
    open_or_create_db, purge_sqlite_database, save_library_database_to_sqlite,
};
use trophyvault_core::storage::{DatabasePreferences, LibraryDatabase};

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

        let username = std::env::var("PROFILE_USERNAME")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let slogan = std::env::var("PROFILE_SLOGAN")
            .or_else(|_| std::env::var("SLOGAN"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

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

        Self {
            port,
            api_key,
            admin_password,
            username,
            slogan,
            db_path,
            avatar_path,
            static_dir,
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
}

pub type SharedState = Arc<RwLock<AppState>>;

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
        }
    }

    /// Broadcasts an update notification to all connected SSE clients.
    pub fn broadcast_update(&self) {
        let _ = self.update_tx.send(());
    }

    /// Verifies if a request is authorized with admin password via:
    /// 1. Bearer token matching `admin_password`.
    /// 2. Header `X-Admin-Password` matching `admin_password`.
    /// CRITICAL: It must NOT accept api_key!
    pub fn is_admin_password_authorized(&self, headers: &axum::http::HeaderMap) -> bool {
        // 1. Check Bearer token
        if let Some(auth_header) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if auth_header.starts_with("Bearer ") {
                let token = auth_header.trim_start_matches("Bearer ").trim();
                if token == self.admin_password {
                    return true;
                }
            }
        }

        // 2. Check X-Admin-Password header
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
        // 1. Check Bearer token
        if let Some(auth_header) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if auth_header.starts_with("Bearer ") {
                let token = auth_header.trim_start_matches("Bearer ").trim();
                if token == self.api_key || token == self.admin_password {
                    return true;
                }
            }
        }

        // 2. Check X-Admin-Password header
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
            games: std::collections::HashMap::new(),
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
