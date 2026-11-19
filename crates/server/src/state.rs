use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use trophyvault_core::storage::{DatabasePreferences, LibraryDatabase};

/// Returns the current Unix timestamp in seconds.
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub api_key: String,
    pub admin_password: Option<String>,
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

        let admin_password = std::env::var("ADMIN_PASSWORD")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

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

        let db_path = match std::env::var("DATABASE_PATH") {
            Ok(p) => PathBuf::from(p),
            Err(_) => {
                let default_container_data = PathBuf::from("/data");
                if default_container_data.exists() && default_container_data.is_dir() {
                    default_container_data.join("library_database.json")
                } else {
                    PathBuf::from("./data/library_database.json")
                }
            }
        };

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
    pub admin_password: Option<String>,
    pub username_override: Option<String>,
    pub slogan_override: Option<String>,
    pub db_path: PathBuf,
    pub avatar_path: PathBuf,
    pub update_tx: tokio::sync::broadcast::Sender<()>,
}

pub type SharedState = Arc<RwLock<AppState>>;

impl AppState {
    pub fn new(config: &ServerConfig) -> Self {
        let (database, last_modified) = Self::load_initial_db(&config.db_path);
        let (update_tx, _) = tokio::sync::broadcast::channel(100);
        Self {
            database,
            last_modified,
            api_key: config.api_key.clone(),
            admin_password: config.admin_password.clone(),
            username_override: config.username.clone(),
            slogan_override: config.slogan.clone(),
            db_path: config.db_path.clone(),
            avatar_path: config.avatar_path.clone(),
            update_tx,
        }
    }

    pub fn with_database(config: &ServerConfig, database: LibraryDatabase) -> Self {
        let (update_tx, _) = tokio::sync::broadcast::channel(100);
        Self {
            database,
            last_modified: SystemTime::now(),
            api_key: config.api_key.clone(),
            admin_password: config.admin_password.clone(),
            username_override: config.username.clone(),
            slogan_override: config.slogan.clone(),
            db_path: config.db_path.clone(),
            avatar_path: config.avatar_path.clone(),
            update_tx,
        }
    }

    /// Broadcasts an update notification to all connected SSE clients.
    pub fn broadcast_update(&self) {
        let _ = self.update_tx.send(());
    }

    /// Verifies if a request is authorized with admin privileges via:
    /// 1. Bearer token matching `api_key` or `admin_password`.
    /// 2. Header `X-Admin-Password` matching `admin_password` or `api_key`.
    pub fn is_admin_authorized(&self, headers: &axum::http::HeaderMap) -> bool {
        // 1. Check Bearer token
        if let Some(auth_header) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if auth_header.starts_with("Bearer ") {
                let token = auth_header.trim_start_matches("Bearer ").trim();
                if token == self.api_key {
                    return true;
                }
                if let Some(ref admin_pass) = self.admin_password {
                    if token == admin_pass {
                        return true;
                    }
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
            if let Some(ref admin_pass) = self.admin_password {
                if pass == admin_pass {
                    return true;
                }
            }
            if pass == self.api_key {
                return true;
            }
        }

        false
    }

    fn load_initial_db(path: &PathBuf) -> (LibraryDatabase, SystemTime) {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(db) = serde_json::from_str::<LibraryDatabase>(&content) {
                    tracing::info!(
                        "Loaded existing library database from {} ({} games)",
                        path.display(),
                        db.games.len()
                    );
                    let mtime = std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .unwrap_or_else(|_| SystemTime::now());
                    return (db, mtime);
                } else {
                    tracing::warn!(
                        "Failed to deserialize database at {}, initializing default",
                        path.display()
                    );
                }
            }
        }

        tracing::info!(
            "No existing database found at {}, starting with default",
            path.display()
        );
        let default_db = LibraryDatabase {
            version: 1,
            last_updated: current_timestamp(),
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: std::collections::HashMap::new(),
        };
        (default_db, SystemTime::now())
    }

    /// Atomically writes the database to disk using a temp file and rename.
    pub fn save_db_atomic(&self, db: &LibraryDatabase) -> Result<(), std::io::Error> {
        let json_str = serde_json::to_string_pretty(db)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = self.db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let temp_filename = format!(
            ".{}.tmp.{}",
            self.db_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("database"),
            std::process::id()
        );
        let temp_path = match self.db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(temp_filename),
            _ => PathBuf::from(temp_filename),
        };

        use std::io::Write;
        {
            let mut file = std::fs::File::create(&temp_path)?;
            file.write_all(json_str.as_bytes())?;
            file.sync_all()?;
        }

        std::fs::rename(&temp_path, &self.db_path)?;
        Ok(())
    }
}
