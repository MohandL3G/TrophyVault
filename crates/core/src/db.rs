//! SQLite persistent storage engine and migration subsystem for TrophyVault.
//!
//! Provides schema creation, atomic transactions, serialization of `LibraryDatabase`
//! to/from SQLite tables (`games`, `trophies`, `preferences`, `manual_games`),
//! and seamless migration from existing `library_database.json` files.

use crate::storage::{
    DatabasePreferences, LibraryDatabase, ManualGameSetting, SavedGameEntry, SavedTrophy,
};
use rusqlite::{params, Connection, Result as SqliteResult};
use std::collections::HashMap;
use std::path::Path;

/// Error type for database and migration operations.
#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    Migration(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "SQLite error: {}", e),
            DbError::Io(e) => write!(f, "IO error: {}", e),
            DbError::Json(e) => write!(f, "JSON error: {}", e),
            DbError::Migration(s) => write!(f, "Migration error: {}", s),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DbError::Sqlite(e) => Some(e),
            DbError::Io(e) => Some(e),
            DbError::Json(e) => Some(e),
            DbError::Migration(_) => None,
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}

impl From<serde_json::Error> for DbError {
    fn from(e: serde_json::Error) -> Self {
        DbError::Json(e)
    }
}

/// Initializes all required tables and indexes in the SQLite database.
pub fn init_db_schema(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS games (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            platform TEXT NOT NULL,
            appid TEXT,
            unlocked INTEGER NOT NULL DEFAULT 0,
            total INTEGER NOT NULL DEFAULT 0,
            pct INTEGER NOT NULL DEFAULT 0,
            platinum INTEGER NOT NULL DEFAULT 0,
            base_playtime_minutes INTEGER NOT NULL DEFAULT 0,
            playtime_minutes INTEGER NOT NULL DEFAULT 0,
            last_played INTEGER,
            source TEXT NOT NULL DEFAULT '',
            last_scanned INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS trophies (
            game_id TEXT NOT NULL,
            id TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            grade TEXT NOT NULL DEFAULT '',
            unlocked INTEGER NOT NULL DEFAULT 0,
            unlocked_time INTEGER,
            PRIMARY KEY (game_id, id)
        );

        CREATE TABLE IF NOT EXISTS preferences (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS manual_games (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            platform TEXT NOT NULL DEFAULT '',
            parser_type TEXT NOT NULL DEFAULT '',
            user_dir TEXT NOT NULL DEFAULT '',
            non_steam_appid TEXT,
            enabled INTEGER NOT NULL DEFAULT 1
        );

        CREATE INDEX IF NOT EXISTS idx_trophies_game_id ON trophies(game_id);
        ",
    )?;

    // Backward-compatibility schema upgrades for existing SQLite databases
    let _ = conn.execute("ALTER TABLE games ADD COLUMN base_playtime_minutes INTEGER NOT NULL DEFAULT 0;", []);
    let _ = conn.execute("ALTER TABLE trophies ADD COLUMN unlocked INTEGER NOT NULL DEFAULT 0;", []);

    Ok(())
}

/// Opens or creates the SQLite database at `path`, enabling WAL mode and setting up the schema.
pub fn open_or_create_db(path: &Path) -> Result<Connection, rusqlite::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
    )?;
    init_db_schema(&conn)?;
    Ok(conn)
}

/// Creates an in-memory SQLite database initialized with schema (useful for testing).
pub fn open_in_memory_db() -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    init_db_schema(&conn)?;
    Ok(conn)
}

/// Checks if the SQLite database is currently empty (0 games and 0 preferences).
pub fn is_db_empty(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let game_count: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
    let pref_count: i64 = conn.query_row("SELECT COUNT(*) FROM preferences", [], |r| r.get(0))?;
    Ok(game_count == 0 && pref_count == 0)
}

/// Returns the total number of games currently stored in the SQLite database.
pub fn get_game_count(conn: &Connection) -> Result<usize, rusqlite::Error> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
    Ok(count as usize)
}

/// Purges all games, trophies, and manual game entries from SQLite tables within a transaction.
pub fn purge_sqlite_database(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM trophies;", [])?;
    tx.execute("DELETE FROM games;", [])?;
    tx.execute("DELETE FROM manual_games;", [])?;
    tx.commit()?;
    Ok(())
}

/// Saves the entire `LibraryDatabase` snapshot into SQLite tables atomically inside a transaction.
pub fn save_library_database_to_sqlite(
    conn: &mut Connection,
    db: &LibraryDatabase,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;

    // 1. Preferences
    tx.execute("DELETE FROM preferences;", [])?;
    {
        let mut pref_stmt =
            tx.prepare("INSERT OR REPLACE INTO preferences (key, value) VALUES (?1, ?2);")?;

        let sync_settings_str = db
            .preferences
            .sync_settings
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok());

        let prefs_to_save: [(&str, Option<String>); 10] = [
            ("version", Some(db.version.to_string())),
            ("last_updated", Some(db.last_updated.to_string())),
            ("username", Some(db.preferences.username.clone())),
            ("title_line", Some(db.preferences.title_line.clone())),
            ("theme", Some(db.preferences.theme.clone())),
            ("steam_path", db.preferences.steam_path.clone()),
            ("steam_id", db.preferences.steam_id.clone()),
            ("export_path", db.preferences.export_path.clone()),
            ("auto_sync", Some(db.preferences.auto_sync.to_string())),
            ("sync_settings", sync_settings_str),
        ];

        for (key, val_opt) in &prefs_to_save {
            if let Some(val) = val_opt {
                pref_stmt.execute(params![key, val])?;
            }
        }
    }

    // 2. Manual games
    tx.execute("DELETE FROM manual_games;", [])?;
    {
        let mut manual_stmt = tx.prepare(
            "INSERT OR REPLACE INTO manual_games (id, name, platform, parser_type, user_dir, non_steam_appid, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);"
        )?;

        for m in &db.manual_games {
            manual_stmt.execute(params![
                m.id,
                m.name,
                m.platform,
                m.parser_type,
                m.user_dir,
                m.non_steam_appid,
                if m.enabled { 1i64 } else { 0i64 },
            ])?;
        }
    }

    // 3. Games & Trophies
    tx.execute("DELETE FROM trophies;", [])?;
    tx.execute("DELETE FROM games;", [])?;
    {
        let mut game_stmt = tx.prepare(
            "INSERT OR REPLACE INTO games (id, name, platform, appid, unlocked, total, pct, platinum, playtime_minutes, last_played, source, last_scanned, base_playtime_minutes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);"
        )?;

        let mut trophy_stmt = tx.prepare(
            "INSERT OR REPLACE INTO trophies (game_id, id, name, description, grade, unlocked, unlocked_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);"
        )?;

        for game in db.games.values() {
            game_stmt.execute(params![
                game.id,
                game.name,
                game.platform,
                game.appid,
                game.unlocked as i64,
                game.total as i64,
                game.pct as i64,
                if game.platinum { 1i64 } else { 0i64 },
                game.playtime_minutes as i64,
                game.last_played.map(|v| v as i64),
                game.source,
                game.last_scanned as i64,
                game.base_playtime_minutes as i64,
            ])?;

            for trophy in &game.trophies {
                trophy_stmt.execute(params![
                    game.id,
                    trophy.id,
                    trophy.name,
                    trophy.description,
                    trophy.grade,
                    if trophy.unlocked { 1i64 } else { 0i64 },
                    trophy.unlocked_time,
                ])?;
            }
        }
    }

    tx.commit()?;
    Ok(())
}

/// Loads the entire `LibraryDatabase` from SQLite tables.
pub fn load_library_database_from_sqlite(
    conn: &Connection,
) -> Result<LibraryDatabase, rusqlite::Error> {
    // 1. Load preferences
    let mut pref_map = HashMap::new();
    {
        let mut pref_stmt = conn.prepare("SELECT key, value FROM preferences;")?;
        let rows = pref_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for r in rows {
            let (k, v) = r?;
            pref_map.insert(k, v);
        }
    }

    let version = pref_map
        .get("version")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    let last_updated = pref_map
        .get("last_updated")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let username = pref_map
        .get("username")
        .cloned()
        .unwrap_or_else(|| "MOHANDL3G".to_string());
    let title_line = pref_map
        .get("title_line")
        .cloned()
        .unwrap_or_else(|| "Hunter of Yharnam   •   Trophy Hunter".to_string());
    let theme = pref_map
        .get("theme")
        .cloned()
        .unwrap_or_else(|| "CyberpunkCyan".to_string());
    let steam_path = pref_map
        .get("steam_path")
        .cloned()
        .filter(|s| !s.is_empty());
    let steam_id = pref_map
        .get("steam_id")
        .cloned()
        .filter(|s| !s.is_empty());
    let export_path = pref_map
        .get("export_path")
        .cloned()
        .filter(|s| !s.is_empty());
    let auto_sync = pref_map
        .get("auto_sync")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);
    let sync_settings = pref_map
        .get("sync_settings")
        .and_then(|v| serde_json::from_str(v).ok());

    let preferences = DatabasePreferences {
        username,
        title_line,
        theme,
        steam_path,
        steam_id,
        export_path,
        auto_sync,
        sync_settings,
    };

    // 2. Load manual games
    let mut manual_games = Vec::new();
    {
        let mut manual_stmt = conn.prepare(
            "SELECT id, name, platform, parser_type, user_dir, non_steam_appid, enabled FROM manual_games;"
        )?;
        let rows = manual_stmt.query_map([], |row| {
            Ok(ManualGameSetting {
                id: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                parser_type: row.get(3)?,
                user_dir: row.get(4)?,
                non_steam_appid: row.get(5)?,
                enabled: row.get::<_, i64>(6)? != 0,
            })
        })?;
        for r in rows {
            manual_games.push(r?);
        }
    }

    // 3. Load games
    let mut games: HashMap<String, SavedGameEntry> = HashMap::new();
    {
        let mut game_stmt = conn.prepare(
            "SELECT id, name, platform, appid, unlocked, total, pct, platinum, playtime_minutes, last_played, source, last_scanned, base_playtime_minutes FROM games;"
        )?;
        let rows = game_stmt.query_map([], |row| {
            Ok(SavedGameEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                appid: row.get(3)?,
                unlocked: row.get::<_, i64>(4)? as u32,
                total: row.get::<_, i64>(5)? as u32,
                pct: row.get::<_, i64>(6)? as u32,
                platinum: row.get::<_, i64>(7)? != 0,
                playtime_minutes: row.get::<_, i64>(8)? as u32,
                last_played: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                source: row.get(10)?,
                last_scanned: row.get::<_, i64>(11)? as u64,
                base_playtime_minutes: row.get::<_, Option<i64>>(12)?.unwrap_or(0) as u32,
                trophies: Vec::new(),
            })
        })?;
        for r in rows {
            let entry = r?;
            games.insert(entry.id.clone(), entry);
        }
    }

    // 4. Load trophies
    {
        let mut trophy_stmt = conn.prepare(
            "SELECT game_id, id, name, description, grade, unlocked, unlocked_time FROM trophies ORDER BY game_id, id;"
        )?;
        let rows = trophy_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SavedTrophy {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    grade: row.get(4)?,
                    unlocked: row.get::<_, i64>(5)? != 0,
                    unlocked_time: row.get(6)?,
                },
            ))
        })?;
        for r in rows {
            let (game_id, trophy) = r?;
            if let Some(game) = games.get_mut(&game_id) {
                game.trophies.push(trophy);
            }
        }
    }

    Ok(LibraryDatabase {
        version,
        last_updated,
        preferences,
        manual_games,
        games,
    })
}

/// Query trophies for a specific game, ordered by unlocked desc, then id asc.
pub fn get_trophies_for_game(conn: &Connection, game_id: &str) -> SqliteResult<Vec<SavedTrophy>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, grade, unlocked, unlocked_time FROM trophies WHERE game_id = ?1 ORDER BY unlocked DESC, id ASC;"
    )?;
    let rows = stmt.query_map(params![game_id], |row| {
        Ok(SavedTrophy {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            grade: row.get(3)?,
            unlocked: row.get::<_, i64>(4)? != 0,
            unlocked_time: row.get(5)?,
        })
    })?;
    let mut trophies = Vec::new();
    for r in rows {
        trophies.push(r?);
    }
    Ok(trophies)
}

/// Persists a list of trophies for a specific game.
pub fn save_trophies_for_game(conn: &Connection, game_id: &str, trophies: &[SavedTrophy]) -> SqliteResult<()> {
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO trophies (game_id, id, name, description, grade, unlocked, unlocked_time)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);"
    )?;
    for t in trophies {
        stmt.execute(params![
            game_id,
            t.id,
            t.name,
            t.description,
            t.grade,
            if t.unlocked { 1i64 } else { 0i64 },
            t.unlocked_time,
        ])?;
    }
    Ok(())
}

/// Returns a map of all trophies grouped by game_id.
pub fn get_all_trophies(conn: &Connection) -> SqliteResult<HashMap<String, Vec<SavedTrophy>>> {
    let mut stmt = conn.prepare(
        "SELECT game_id, id, name, description, grade, unlocked, unlocked_time FROM trophies ORDER BY game_id, id ASC;"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SavedTrophy {
                id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                grade: row.get(4)?,
                unlocked: row.get::<_, i64>(5)? != 0,
                unlocked_time: row.get(6)?,
            },
        ))
    })?;
    let mut map: HashMap<String, Vec<SavedTrophy>> = HashMap::new();
    for r in rows {
        let (game_id, trophy) = r?;
        map.entry(game_id).or_default().push(trophy);
    }
    Ok(map)
}

/// Returns total trophy count and unlocked trophy count for a game.
pub fn get_trophy_stats_for_game(conn: &Connection, game_id: &str) -> SqliteResult<(usize, usize)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM trophies WHERE game_id = ?1;",
        params![game_id],
        |r| r.get(0),
    )?;
    let unlocked: i64 = conn.query_row(
        "SELECT COUNT(*) FROM trophies WHERE game_id = ?1 AND unlocked != 0;",
        params![game_id],
        |r| r.get(0),
    )?;
    Ok((total as usize, unlocked as usize))
}

/// Migrates data from an existing `library_database.json` file into the SQLite database at `db_path`.
/// If `json_path` does not exist, or the SQLite database already contains games, migration is skipped.
/// Zero data is lost: JSON data is preserved, and SQLite is populated seamlessly.
pub fn migrate_json_to_sqlite_if_needed(json_path: &Path, db_path: &Path) -> Result<(), DbError> {
    if !json_path.exists() {
        return Ok(());
    }

    if json_path == db_path {
        return Err(DbError::Migration(format!(
            "json_path and db_path point to the same file: {}",
            json_path.display()
        )));
    }

    let mut conn = open_or_create_db(db_path)?;

    // Check if SQLite database already contains games
    let game_count: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?;
    if game_count > 0 {
        return Ok(());
    }

    // Read and parse JSON database
    let content = std::fs::read_to_string(json_path)?;
    let db: LibraryDatabase = serde_json::from_str(&content)?;

    // Populate SQLite tables
    save_library_database_to_sqlite(&mut conn, &db)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SyncMode;

    fn sample_database() -> LibraryDatabase {
        let mut games = HashMap::new();
        games.insert(
            "1086940".to_string(),
            SavedGameEntry {
                id: "1086940".to_string(),
                name: "Baldur's Gate 3".to_string(),
                platform: "Steam".to_string(),
                appid: Some("1086940".to_string()),
                unlocked: 54,
                total: 54,
                pct: 100,
                platinum: true,
                base_playtime_minutes: 0,
                playtime_minutes: 14500,
                last_played: Some(1732000000),
                source: "steam".to_string(),
                last_scanned: 1732000100,
                trophies: vec![
                    SavedTrophy {
                        id: "1".to_string(),
                        name: "All's Well That Ends Well".to_string(),
                        description: "Complete the game".to_string(),
                        grade: "Gold".to_string(),
                        unlocked: true,
                        unlocked_time: Some(1732000050),
                    },
                    SavedTrophy {
                        id: "2".to_string(),
                        name: "Absolute Power".to_string(),
                        description: "Take control of the Netherbrain".to_string(),
                        grade: "Silver".to_string(),
                        unlocked: true,
                        unlocked_time: Some(1731999900),
                    },
                ],
            },
        );

        games.insert(
            "bloodborne".to_string(),
            SavedGameEntry {
                id: "bloodborne".to_string(),
                name: "Bloodborne".to_string(),
                platform: "PC Port / Recomp".to_string(),
                appid: Some("2485153835".to_string()),
                unlocked: 40,
                total: 40,
                pct: 100,
                platinum: true,
                base_playtime_minutes: 0,
                playtime_minutes: 8400,
                last_played: Some(1732100000),
                source: "manual".to_string(),
                last_scanned: 1732100500,
                trophies: vec![SavedTrophy {
                    id: "1".to_string(),
                    name: "Bloodborne".to_string(),
                    description: "All trophies acquired".to_string(),
                    grade: "Platinum".to_string(),
                    unlocked: true,
                    unlocked_time: Some(1732100400),
                }],
            },
        );

        LibraryDatabase {
            version: 2,
            last_updated: 1732100600,
            preferences: DatabasePreferences {
                username: "MohandL3G".to_string(),
                title_line: "Hunter of Yharnam • Master Achiever".to_string(),
                theme: "Gold".to_string(),
                steam_path: Some("/home/mohand/.steam/steam".to_string()),
                steam_id: Some("76561198000000000".to_string()),
                export_path: Some("/tmp/export".to_string()),
                auto_sync: true,
                sync_settings: Some(crate::storage::SyncSettings {
                    enabled: true,
                    server_url: "https://vault.mohandl3g.ly".to_string(),
                    api_key: "vault-secret".to_string(),
                    auto_sync_on_exit: true,
                    last_sync_timestamp: Some(1732100000),
                    last_sync_status: Some("Success".to_string()),
                    sync_mode: SyncMode::Full,
                }),
            },
            manual_games: vec![ManualGameSetting {
                id: "bloodborne".to_string(),
                name: "Bloodborne".to_string(),
                platform: "PC Port / Recomp".to_string(),
                parser_type: "bbport".to_string(),
                user_dir: "/path/to/bb/user".to_string(),
                non_steam_appid: Some("2485153835".to_string()),
                enabled: true,
            }],
            games,
        }
    }

    #[test]
    fn test_schema_initialization_in_memory() {
        let conn = open_in_memory_db().expect("in-memory db must initialize cleanly");
        assert!(is_db_empty(&conn).expect("new db should be empty"));
        assert_eq!(get_game_count(&conn).expect("game count must be 0"), 0);

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        assert!(tables.contains(&"games".to_string()));
        assert!(tables.contains(&"trophies".to_string()));
        assert!(tables.contains(&"preferences".to_string()));
        assert!(tables.contains(&"manual_games".to_string()));
    }

    #[test]
    fn test_save_and_load_library_database_roundtrip() {
        let mut conn = open_in_memory_db().expect("in-memory db open failed");
        let sample = sample_database();

        save_library_database_to_sqlite(&mut conn, &sample)
            .expect("saving library database must succeed");

        assert!(!is_db_empty(&conn).unwrap());
        assert_eq!(get_game_count(&conn).unwrap(), 2);

        let loaded = load_library_database_from_sqlite(&conn)
            .expect("loading library database must succeed");

        assert_eq!(loaded.version, sample.version);
        assert_eq!(loaded.last_updated, sample.last_updated);
        assert_eq!(loaded.preferences.username, sample.preferences.username);
        assert_eq!(loaded.preferences.title_line, sample.preferences.title_line);
        assert_eq!(loaded.preferences.theme, sample.preferences.theme);
        assert_eq!(loaded.preferences.steam_path, sample.preferences.steam_path);
        assert_eq!(loaded.preferences.steam_id, sample.preferences.steam_id);
        assert_eq!(
            loaded.preferences.export_path,
            sample.preferences.export_path
        );
        assert_eq!(loaded.preferences.auto_sync, sample.preferences.auto_sync);

        let sync_loaded = loaded.preferences.sync_settings.expect("sync_settings must be Some");
        let sync_sample = sample.preferences.sync_settings.unwrap();
        assert_eq!(sync_loaded.enabled, sync_sample.enabled);
        assert_eq!(sync_loaded.server_url, sync_sample.server_url);
        assert_eq!(sync_loaded.api_key, sync_sample.api_key);
        assert_eq!(sync_loaded.sync_mode, sync_sample.sync_mode);

        assert_eq!(loaded.manual_games.len(), sample.manual_games.len());
        assert_eq!(loaded.manual_games[0].id, sample.manual_games[0].id);
        assert_eq!(loaded.manual_games[0].name, sample.manual_games[0].name);

        assert_eq!(loaded.games.len(), sample.games.len());
        let bg3 = loaded.games.get("1086940").expect("BG3 must be present");
        assert_eq!(bg3.name, "Baldur's Gate 3");
        assert_eq!(bg3.unlocked, 54);
        assert_eq!(bg3.total, 54);
        assert!(bg3.platinum);
        assert_eq!(bg3.playtime_minutes, 14500);
        assert_eq!(bg3.trophies.len(), 2);
        assert_eq!(bg3.trophies[0].name, "All's Well That Ends Well");

        let bb = loaded.games.get("bloodborne").expect("BB must be present");
        assert_eq!(bb.name, "Bloodborne");
        assert_eq!(bb.unlocked, 40);
        assert_eq!(bb.trophies.len(), 1);
        assert_eq!(bb.trophies[0].grade, "Platinum");
    }

    #[test]
    fn test_purge_sqlite_database() {
        let mut conn = open_in_memory_db().unwrap();
        let sample = sample_database();

        save_library_database_to_sqlite(&mut conn, &sample).unwrap();
        assert_eq!(get_game_count(&conn).unwrap(), 2);

        purge_sqlite_database(&mut conn).unwrap();
        assert_eq!(get_game_count(&conn).unwrap(), 0);

        // Games and manual games cleared
        let loaded = load_library_database_from_sqlite(&conn).unwrap();
        assert!(loaded.games.is_empty());
        assert!(loaded.manual_games.is_empty());
        // Preferences retained
        assert_eq!(loaded.preferences.username, "MohandL3G");
    }

    #[test]
    fn test_migration_from_json_to_sqlite() {
        let temp_dir = std::env::temp_dir().join(format!(
            "trophyvault_test_mig_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let json_path = temp_dir.join("library_database.json");
        let db_path = temp_dir.join("library_database.db");

        let sample = sample_database();
        let json_content = serde_json::to_string_pretty(&sample).unwrap();
        std::fs::write(&json_path, json_content).unwrap();

        // 1. Run migration
        let result = migrate_json_to_sqlite_if_needed(&json_path, &db_path);
        assert!(result.is_ok(), "migration should succeed: {:?}", result);

        // 2. Open DB and verify content matches
        let conn = open_or_create_db(&db_path).unwrap();
        let loaded = load_library_database_from_sqlite(&conn).unwrap();
        assert_eq!(loaded.games.len(), 2);
        assert_eq!(loaded.preferences.username, "MohandL3G");
        assert_eq!(loaded.manual_games.len(), 1);

        // 3. Re-running migration is idempotent and does not corrupt or overwrite
        let result2 = migrate_json_to_sqlite_if_needed(&json_path, &db_path);
        assert!(result2.is_ok());
        let count = get_game_count(&conn).unwrap();
        assert_eq!(count, 2);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_migration_skipped_when_json_missing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "trophyvault_test_missing_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let json_path = temp_dir.join("non_existent.json");
        let db_path = temp_dir.join("library.db");

        let res = migrate_json_to_sqlite_if_needed(&json_path, &db_path);
        assert!(res.is_ok());
        assert!(!db_path.exists());
    }

    #[test]
    fn test_migration_does_not_overwrite_existing_sqlite_data() {
        let temp_dir = std::env::temp_dir().join(format!(
            "trophyvault_test_no_overwrite_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let json_path = temp_dir.join("library_database.json");
        let db_path = temp_dir.join("library_database.db");

        // Create SQLite database with 1 game
        let mut conn = open_or_create_db(&db_path).unwrap();
        let mut initial_db = sample_database();
        initial_db.games.remove("bloodborne"); // only BG3 in sqlite
        save_library_database_to_sqlite(&mut conn, &initial_db).unwrap();
        assert_eq!(get_game_count(&conn).unwrap(), 1);

        // Write JSON with 2 games
        let full_db = sample_database();
        std::fs::write(&json_path, serde_json::to_string(&full_db).unwrap()).unwrap();

        // Run migration - should be skipped because SQLite already has games
        let res = migrate_json_to_sqlite_if_needed(&json_path, &db_path);
        assert!(res.is_ok());

        // Verify count remains 1
        assert_eq!(get_game_count(&conn).unwrap(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_trophies_query_helpers() {
        let conn = open_in_memory_db().unwrap();
        let trophies = vec![
            SavedTrophy {
                id: "0".to_string(),
                name: "Bloodborne".to_string(),
                description: "All trophies acquired".to_string(),
                grade: "Platinum".to_string(),
                unlocked: true,
                unlocked_time: Some(1730000000),
            },
            SavedTrophy {
                id: "1".to_string(),
                name: "Yharnam Sunrise".to_string(),
                description: "You lived through the hunt".to_string(),
                grade: "Gold".to_string(),
                unlocked: false,
                unlocked_time: None,
            },
        ];

        save_trophies_for_game(&conn, "bloodborne", &trophies).unwrap();

        let queried = get_trophies_for_game(&conn, "bloodborne").unwrap();
        assert_eq!(queried.len(), 2);
        // Unlocked should be first due to ORDER BY unlocked DESC
        assert_eq!(queried[0].id, "0");
        assert!(queried[0].unlocked);
        assert_eq!(queried[1].id, "1");
        assert!(!queried[1].unlocked);

        let stats = get_trophy_stats_for_game(&conn, "bloodborne").unwrap();
        assert_eq!(stats, (2, 1)); // (total, unlocked)

        let all = get_all_trophies(&conn).unwrap();
        assert!(all.contains_key("bloodborne"));
        assert_eq!(all["bloodborne"].len(), 2);
    }
}
