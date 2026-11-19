//! Persistent JSON database and synchronization engine.
//! Stored in `%APPDATA%\TrophyVault\library_database.json`.

use crate::bloodborne::parse_bloodborne_user_dir;
use crate::data::{AchievementsData, GameItem, LibraryStats, Preferences};
use crate::steam::{
    find_active_steam_id, find_steam_dir, lookup_playtime, parse_localconfig_playtimes,
    parse_non_steam_shortcuts, scan_luatools_games,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDatabase {
    pub version: u32,
    pub last_updated: u64,
    pub preferences: DatabasePreferences,
    #[serde(default)]
    pub manual_games: Vec<ManualGameSetting>,
    #[serde(default)]
    pub games: HashMap<String, SavedGameEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SyncMode {
    #[default]
    Updates,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub server_url: String, // e.g. "https://vault.mohandl3g.ly"
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub auto_sync_on_exit: bool,
    #[serde(default)]
    pub last_sync_timestamp: Option<u64>,
    #[serde(default)]
    pub last_sync_status: Option<String>,
    #[serde(default)]
    pub sync_mode: SyncMode,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: String::new(),
            api_key: String::new(),
            auto_sync_on_exit: false,
            last_sync_timestamp: None,
            last_sync_status: None,
            sync_mode: SyncMode::default(),
        }
    }
}

impl SyncSettings {
    pub fn is_configured(&self) -> bool {
        !self.server_url.trim().is_empty() && !self.api_key.trim().is_empty()
    }

    pub fn formatted_base_url(&self) -> Result<String, String> {
        let base_url = self.server_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Err("Server URL cannot be empty".to_string());
        }
        let url = if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            format!("https://{}", base_url)
        } else {
            base_url.to_string()
        };
        Ok(url)
    }

    pub fn formatted_api_url(&self) -> Result<String, String> {
        let base = self.formatted_base_url()?;
        Ok(format!("{}/api/sync", base))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabasePreferences {
    #[serde(default = "default_username")]
    pub username: String,
    #[serde(default = "default_title")]
    pub title_line: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    pub steam_path: Option<String>,
    pub steam_id: Option<String>,
    pub export_path: Option<String>,
    #[serde(default = "default_true")]
    pub auto_sync: bool,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,
}

fn default_username() -> String {
    "MOHANDL3G".to_string()
}
fn default_title() -> String {
    "Hunter of Yharnam   •   Trophy Hunter".to_string()
}
fn default_theme() -> String {
    "CyberpunkCyan".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for DatabasePreferences {
    fn default() -> Self {
        Self {
            username: default_username(),
            title_line: default_title(),
            theme: default_theme(),
            steam_path: None,
            steam_id: None,
            export_path: None,
            auto_sync: true,
            sync_settings: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualGameSetting {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub parser_type: String, // "bbport"
    pub user_dir: String,
    pub non_steam_appid: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedGameEntry {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub appid: Option<String>,
    pub unlocked: u32,
    pub total: u32,
    pub pct: u32,
    pub platinum: bool,
    #[serde(default)]
    pub playtime_minutes: u32,
    pub last_played: Option<u64>,
    pub source: String, // "manual", "steam", "luatools", "legacy"
    pub last_scanned: u64,
    #[serde(default)]
    pub trophies: Vec<SavedTrophy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTrophy {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub grade: String,
    pub unlocked_time: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub manual_scanned: u32,
    pub steam_updated: u32,
    pub luatools_found: u32,
    pub total_games: usize,
    pub details: Vec<String>,
}

/// Base directory for TrophyVault application data in `%APPDATA%\TrophyVault`.
pub fn get_app_data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("TrophyVault");
    }

    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("TrophyVault");
    }

    if let Some(data_dir) = dirs::data_dir() {
        return data_dir.join("TrophyVault");
    }

    dirs::home_dir()
        .map(|h| h.join(".trophyvault"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Legacy directory used by earlier versions (`%APPDATA%\AchievementsPassport`).
pub fn get_legacy_app_data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("AchievementsPassport");
    }

    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("AchievementsPassport");
    }

    if let Some(data_dir) = dirs::data_dir() {
        return data_dir.join("AchievementsPassport");
    }

    dirs::home_dir()
        .map(|h| h.join(".achievementspassport"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolves the absolute path to `library_database.json` for a specific Steam user account.
/// E.g. `%APPDATA%\TrophyVault\<steam_id>\library_database.json`.
/// When `steam_id` is None, points to `%APPDATA%\TrophyVault\library_database.json`.
pub fn get_database_path_for_user(steam_id: Option<&str>) -> PathBuf {
    let app_dir = get_app_data_dir();
    if let Some(id) = steam_id {
        let trimmed = id.trim();
        // Restrict to alphanumeric, underscore, or hyphen to prevent directory traversal
        if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return app_dir.join(trimmed).join("library_database.json");
        }
    }
    app_dir.join("library_database.json")
}

/// Resolves the root or fallback database path.
pub fn get_database_path() -> PathBuf {
    get_database_path_for_user(None)
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Sanitize and normalize database entries. Returns true if modified.
fn sanitize_database(db: &mut LibraryDatabase) -> bool {
    let mut changed = false;

    // 1. Purge legacy cracked / emulator games (where platform == "steam" and source == "legacy")
    let before_len = db.games.len();
    db.games.retain(|_, g| {
        !(g.source == "legacy" && g.platform.to_lowercase() == "steam")
    });
    if db.games.len() != before_len {
        changed = true;
    }

    // 2. Normalize all Steam entries (official, LuaTools, HubCapTools, Steamidra) to platform: "Steam", source: "steam"
    for g in db.games.values_mut() {
        let plat_lower = g.platform.to_lowercase();
        if plat_lower.contains("steam")
            && !plat_lower.contains("port")
            && !plat_lower.contains("decomp")
            && !plat_lower.contains("recomp")
        {
            if g.platform != "Steam" || g.source != "steam" {
                g.platform = "Steam".to_string();
                g.source = "steam".to_string();
                changed = true;
            }
        }
    }

    changed
}

/// Load the persistent database for a specific Steam account, migrating from legacy root DB if needed.
pub fn load_or_create_database_for_user(steam_id: Option<&str>) -> LibraryDatabase {
    let db_path = get_database_path_for_user(steam_id);

    // 1. If user-specific database already exists, load it
    if db_path.exists() {
        match fs::read_to_string(&db_path) {
            Ok(content) => match serde_json::from_str::<LibraryDatabase>(&content) {
                Ok(mut db) => {
                    if sanitize_database(&mut db) {
                        let _ = save_database_for_user(&db, steam_id);
                    }
                    return db;
                }
                Err(err) => {
                    eprintln!(
                        "Corrupted database detected at '{}': {}. Quarantining file.",
                        db_path.display(),
                        err
                    );
                    // Quarantine corrupted file to prevent permanent data loss
                    let corrupted_path = db_path.with_extension("json.corrupted");
                    let _ = fs::copy(&db_path, &corrupted_path);

                    // Attempt restore from backup
                    let bak_path = db_path.with_extension("json.bak");
                    if bak_path.exists() {
                        if let Ok(bak_content) = fs::read_to_string(&bak_path) {
                            if let Ok(mut db) = serde_json::from_str::<LibraryDatabase>(&bak_content) {
                                eprintln!("Successfully recovered database from '{}'", bak_path.display());
                                if sanitize_database(&mut db) {
                                    let _ = save_database_for_user(&db, steam_id);
                                }
                                return db;
                            }
                        }
                    }
                }
            },
            Err(e) => {
                eprintln!("Failed to read database at '{}': {}", db_path.display(), e);
            }
        }
    }

    // 2. If user-specific DB does not exist, check for existing databases to migrate
    let migration_candidates = [
        steam_id.map(|id| get_legacy_app_data_dir().join(id).join("library_database.json")),
        Some(get_app_data_dir().join("library_database.json")),
        Some(get_legacy_app_data_dir().join("library_database.json")),
    ];

    for candidate_opt in &migration_candidates {
        if let Some(candidate) = candidate_opt {
            if candidate.exists() {
                if let Ok(content) = fs::read_to_string(candidate) {
                    if let Ok(mut db) = serde_json::from_str::<LibraryDatabase>(&content) {
                        sanitize_database(&mut db);
                        if let Some(id) = steam_id {
                            db.preferences.steam_id = Some(id.to_string());
                        }
                        let _ = save_database_for_user(&db, steam_id);
                        return db;
                    }
                }
            }
        }
    }

    // 3. Initialize fresh isolated database for this user
    let default_user_dir = if cfg!(windows) {
        r"C:\Games\Decomps\Bloodborne\user".to_string()
    } else {
        dirs::home_dir()
            .map(|h| h.join("Games").join("Bloodborne").join("user").to_string_lossy().to_string())
            .unwrap_or_else(|| "Games/Bloodborne/user".to_string())
    };

    let mut db = LibraryDatabase {
        version: 1,
        last_updated: current_timestamp(),
        preferences: DatabasePreferences {
            steam_id: steam_id.map(str::to_string),
            ..DatabasePreferences::default()
        },
        manual_games: vec![
            ManualGameSetting {
                id: "bloodborne".to_string(),
                name: "Bloodborne (PC Recomp)".to_string(),
                platform: "PC Port / Recomp".to_string(),
                parser_type: "bbport".to_string(),
                user_dir: default_user_dir,
                non_steam_appid: Some("2485153835".to_string()),
                enabled: true,
            },
        ],
        games: HashMap::new(),
    };

    // Attempt to seed from legacy Achievements app if dashboard-summary.json exists
    seed_from_legacy_achievements(&mut db);

    // Save initial DB for this user
    let _ = save_database_for_user(&db, steam_id);
    db
}

/// Load the persistent database from disk or initialize it with default configs.
pub fn load_or_create_database() -> LibraryDatabase {
    load_or_create_database_for_user(None)
}

/// Seed database with existing games from PSerban93's Achievements app if present.
fn seed_from_legacy_achievements(db: &mut LibraryDatabase) {
    let legacy_paths = [
        dirs::config_dir().map(|c| c.join("Achievements").join("dashboard-summary.json")),
        std::env::var("APPDATA").ok().map(|a| PathBuf::from(a).join("Achievements").join("dashboard-summary.json")),
    ];

    for path_opt in &legacy_paths {
        if let Some(path) = path_opt {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    #[derive(Deserialize)]
                    struct LegacySummary {
                        #[serde(default)]
                        entries: HashMap<String, LegacyEntry>,
                    }
                    #[derive(Deserialize)]
                    struct LegacyEntry {
                        platform: Option<String>,
                        appid: Option<String>,
                        #[serde(default)]
                        pct: u32,
                        #[serde(default)]
                        unlocked: u32,
                        #[serde(default)]
                        total: u32,
                        platinum: Option<bool>,
                        updated: Option<u64>,
                    }

                    if let Ok(summary) = serde_json::from_str::<LegacySummary>(&content) {
                        for (name, entry) in summary.entries {
                            // Only seed games verified from official Steam; ignore cracked/emulator tags (e.g. GSE, CODEX, OnlineFix)
                            let raw_platform = entry.platform.unwrap_or_default();
                            if raw_platform != "steam-official" {
                                continue;
                            }
                            let id = entry.appid.clone().unwrap_or_else(|| name.clone());
                            let pct = if entry.total > 0 {
                                entry.pct.max((entry.unlocked * 100) / entry.total)
                            } else {
                                0
                            };
                            let platinum = entry.platinum.unwrap_or(false)
                                || (entry.total > 0 && entry.unlocked == entry.total);

                            db.games.insert(
                                id.clone(),
                                SavedGameEntry {
                                    id,
                                    name,
                                    platform: "Steam".to_string(),
                                    appid: entry.appid,
                                    unlocked: entry.unlocked,
                                    total: entry.total,
                                    pct,
                                    platinum,
                                    playtime_minutes: 0,
                                    last_played: None,
                                    source: "steam".to_string(),
                                    last_scanned: entry.updated.unwrap_or_else(current_timestamp),
                                    trophies: Vec::new(),
                                },
                            );
                        }
                    }
                }
                break;
            }
        }
    }
}

/// Save the database for a specific Steam user account using atomic replace.
pub fn save_database_for_user(db: &LibraryDatabase, steam_id: Option<&str>) -> Result<(), String> {
    let target_id = steam_id.or(db.preferences.steam_id.as_deref());
    let db_path = get_database_path_for_user(target_id);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
    }

    let json_str = serde_json::to_string_pretty(db)
        .map_err(|e| format!("Failed to serialize database: {}", e))?;

    let tmp_path = db_path.with_extension("json.tmp");
    let bak_path = db_path.with_extension("json.bak");

    // 1. Write to temporary file with explicit flush and sync_all
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| format!("Failed to create temporary file '{}': {}", tmp_path.display(), e))?;
        file.write_all(json_str.as_bytes())
            .map_err(|e| format!("Failed to write temporary file '{}': {}", tmp_path.display(), e))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync temporary file '{}': {}", tmp_path.display(), e))?;
    }

    // 2. Backup existing valid database file if present
    if db_path.exists() {
        let _ = fs::copy(&db_path, &bak_path);
    }

    // 3. Atomically replace database file
    fs::rename(&tmp_path, &db_path)
        .map_err(|e| format!("Failed to rename '{}' to '{}': {}", tmp_path.display(), db_path.display(), e))?;

    Ok(())
}

/// Save the database to the appropriate profile storage path based on `db.preferences.steam_id`.
pub fn save_database(db: &LibraryDatabase) -> Result<(), String> {
    save_database_for_user(db, db.preferences.steam_id.as_deref())
}

/// Run full library sync: scans configured manual games (Bloodborne),
/// queries Steam localconfig.vdf for playtimes, discovers LuaTools and Non-Steam shortcuts.
///
/// **Crucial Retention Guarantee**:
/// Previously recorded games are NEVER deleted during sync. Even if game folders or lua files
/// are removed from disk, their database entries are retained.
pub fn sync_library(db: &mut LibraryDatabase) -> SyncReport {
    let mut report = SyncReport::default();
    let now = current_timestamp();

    // 1. Locate Steam directory and active account ID
    let steam_dir = db
        .preferences
        .steam_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(find_steam_dir);

    let steam_id = db
        .preferences
        .steam_id
        .clone()
        .or_else(|| steam_dir.as_ref().and_then(|d| find_active_steam_id(d)));

    // Parse playtime map from localconfig.vdf
    let playtimes = if let (Some(s_dir), Some(s_id)) = (&steam_dir, &steam_id) {
        parse_localconfig_playtimes(s_dir, s_id)
    } else {
        HashMap::new()
    };

    // 2. Discover Non-Steam shortcuts early from shortcuts.vdf
    let shortcuts = if let (Some(s_dir), Some(s_id)) = (&steam_dir, &steam_id) {
        parse_non_steam_shortcuts(s_dir, s_id)
    } else {
        Vec::new()
    };

    // Auto-link manual games with Non-Steam shortcuts (e.g. Bloodborne)
    for sc in &shortcuts {
        let sc_appid_str = sc.appid.to_string();
        let sc_name_lower = sc.appname.to_lowercase();
        for manual in &mut db.manual_games {
            if manual.non_steam_appid.is_none() {
                let id_lower = manual.id.to_lowercase();
                let name_lower = manual.name.to_lowercase();
                if sc_name_lower.contains(&id_lower)
                    || name_lower.contains(&sc_name_lower)
                    || sc_name_lower.contains(&name_lower)
                {
                    manual.non_steam_appid = Some(sc_appid_str.clone());
                }
            }
        }
    }

    // 3. Scan configured Manual Games (e.g. Bloodborne)
    let manual_configs = db.manual_games.clone();
    for manual_cfg in manual_configs {
        if !manual_cfg.enabled {
            continue;
        }

        if manual_cfg.parser_type == "bbport" {
            let user_path = Path::new(&manual_cfg.user_dir);
            match parse_bloodborne_user_dir(user_path) {
                Ok(res) => {
                    report.manual_scanned += 1;
                    let mut playtime = 0;
                    let mut last_played = None;

                    // Lookup playtime via Non-Steam AppID if specified
                    if let Some(appid) = &manual_cfg.non_steam_appid {
                        if let Some(usage) = lookup_playtime(&playtimes, appid) {
                            playtime = usage.playtime_minutes;
                            last_played = usage.last_played;
                        }
                    }

                    let trophies = res
                        .trophies
                        .into_iter()
                        .map(|t| SavedTrophy {
                            id: t.id,
                            name: t.name,
                            description: t.description,
                            grade: t.grade,
                            unlocked_time: t.unlock_time,
                        })
                        .collect();

                    let entry = SavedGameEntry {
                        id: manual_cfg.id.clone(),
                        name: manual_cfg.name.clone(),
                        platform: manual_cfg.platform.clone(),
                        appid: manual_cfg.non_steam_appid.clone(),
                        unlocked: res.unlocked,
                        total: res.total,
                        pct: res.pct,
                        platinum: res.platinum,
                        playtime_minutes: playtime,
                        last_played,
                        source: "manual".to_string(),
                        last_scanned: now,
                        trophies,
                    };

                    report.details.push(format!(
                        "Synced {}: {}/{} ({}%) • Playtime: {} hrs",
                        manual_cfg.name,
                        res.unlocked,
                        res.total,
                        res.pct,
                        playtime / 60
                    ));

                    db.games.insert(manual_cfg.id.clone(), entry);
                }
                Err(err) => {
                    report.details.push(format!("Manual game {}: {}", manual_cfg.name, err));
                }
            }
        }
    }

    // 4. Scan LuaTools games from stplug-in
    if let Some(s_dir) = &steam_dir {
        let luatools_list = scan_luatools_games(s_dir);
        report.luatools_found = luatools_list.len() as u32;

        for lua_game in luatools_list {
            // All Steam games (official or added via LuaTools / unlockers) are unified under Steam
            if let Some(existing) = db.games.get_mut(&lua_game.appid) {
                existing.platform = "Steam".to_string();
                existing.source = "steam".to_string();
                if existing.name.chars().all(|c| c.is_ascii_digit()) && !lua_game.name.is_empty() {
                    existing.name = lua_game.name.clone();
                }
            } else {
                db.games.insert(
                    lua_game.appid.clone(),
                    SavedGameEntry {
                        id: lua_game.appid.clone(),
                        name: lua_game.name.clone(),
                        platform: "Steam".to_string(),
                        appid: Some(lua_game.appid.clone()),
                        unlocked: 0,
                        total: 0,
                        pct: 0,
                        platinum: false,
                        playtime_minutes: 0,
                        last_played: None,
                        source: "steam".to_string(),
                        last_scanned: 0,
                        trophies: Vec::new(),
                    },
                );
            }
        }
    }

    // 5. Discover Non-Steam shortcuts into persistent db.games
    for sc in shortcuts {
        let sc_appid_str = sc.appid.to_string();
        let is_manual = db.manual_games.iter().any(|m| {
            m.non_steam_appid.as_deref() == Some(&sc_appid_str)
                || m.name.eq_ignore_ascii_case(&sc.appname)
        });

        if !is_manual {
            let mut playtime = 0;
            let mut last_played = None;
            if let Some(usage) = lookup_playtime(&playtimes, &sc_appid_str) {
                playtime = usage.playtime_minutes;
                last_played = usage.last_played;
            }

            if let Some(existing) = db.games.get_mut(&sc_appid_str) {
                if existing.name.is_empty() {
                    existing.name = sc.appname.clone();
                }
                if playtime > existing.playtime_minutes {
                    existing.playtime_minutes = playtime;
                }
                if last_played.is_some() {
                    existing.last_played = last_played;
                }
            } else {
                db.games.insert(
                    sc_appid_str.clone(),
                    SavedGameEntry {
                        id: sc_appid_str.clone(),
                        name: sc.appname.clone(),
                        platform: "Non-Steam".to_string(),
                        appid: Some(sc_appid_str.clone()),
                        unlocked: 0,
                        total: 0,
                        pct: 0,
                        platinum: false,
                        playtime_minutes: playtime,
                        last_played,
                        source: "shortcut".to_string(),
                        last_scanned: now,
                        trophies: Vec::new(),
                    },
                );
            }
        }
    }

    // 6. Universal sweep: Update playtimes and timestamps for all games matching localconfig.vdf
    for entry in db.games.values_mut() {
        if let Some(appid) = &entry.appid {
            if let Some(usage) = lookup_playtime(&playtimes, appid) {
                if usage.playtime_minutes > entry.playtime_minutes {
                    entry.playtime_minutes = usage.playtime_minutes;
                }
                if usage.last_played.is_some() {
                    entry.last_played = usage.last_played;
                }
                report.steam_updated += 1;
            }
        }
    }

    db.last_updated = now;
    let _ = save_database(db);

    report.total_games = db.games.len();
    report
}

/// Convert database entries into `AchievementsData` for SVG generation and preview.
pub fn database_to_achievements_data(db: &LibraryDatabase) -> AchievementsData {
    let mut perfect_games = 0;
    let mut in_progress_games = 0;
    let mut not_started_games = 0;
    let mut total_achievements = 0;
    let mut unlocked_achievements = 0;

    let mut steam_official_games = 0;
    let mut steam_official_unlocked = 0;
    let mut steam_official_total = 0;

    let mut custom_games = 0;
    let mut custom_unlocked = 0;
    let mut custom_total = 0;
    let mut total_playtime_minutes: u64 = 0;

    let mut games: Vec<GameItem> = Vec::with_capacity(db.games.len());

    for entry in db.games.values() {
        total_playtime_minutes += entry.playtime_minutes as u64;
        let unlocked = entry.unlocked;
        let total = entry.total;
        let pct = if total > 0 {
            entry.pct.max((unlocked * 100) / total)
        } else {
            0
        };

        let platinum = entry.platinum || (total > 0 && unlocked == total);

        if total > 0 {
            total_achievements += total;
            unlocked_achievements += unlocked;

            if unlocked == total {
                perfect_games += 1;
            } else if unlocked > 0 {
                in_progress_games += 1;
            } else {
                not_started_games += 1;
            }

            let is_decomp_or_port = entry.source == "manual"
                || entry.platform.to_lowercase().contains("port")
                || entry.platform.to_lowercase().contains("recomp")
                || entry.platform.to_lowercase().contains("decomp");

            if is_decomp_or_port {
                custom_games += 1;
                custom_unlocked += unlocked;
                custom_total += total;
            } else {
                steam_official_games += 1;
                steam_official_unlocked += unlocked;
                steam_official_total += total;
            }
        } else {
            not_started_games += 1;
        }

        games.push(GameItem {
            name: entry.name.clone(),
            platform: entry.platform.clone(),
            appid: entry.appid.clone(),
            unlocked,
            total,
            pct,
            platinum,
            playtime_minutes: entry.playtime_minutes,
            last_played: entry.last_played,
        });
    }

    // Sort: manual/in-progress/perfect first, then by unlocked count descending
    games.sort_by(|a, b| {
        b.pct
            .cmp(&a.pct)
            .then_with(|| b.unlocked.cmp(&a.unlocked))
            .then_with(|| a.name.cmp(&b.name))
    });

    let total_configs = games.len() as u32;
    let active_configs = total_configs;
    let locked_achievements = total_achievements.saturating_sub(unlocked_achievements);

    let completion_rate = if total_achievements > 0 {
        (unlocked_achievements as f64 / total_achievements as f64) * 100.0
    } else {
        0.0
    };

    let perfect_score = (perfect_games as f64 / 46.0).min(1.0) * 40.0;
    let volume_score = (unlocked_achievements as f64 / 3500.0).min(1.0) * 35.0;
    let rate_score = (completion_rate / 15.0).min(1.0) * 25.0;
    let ovr_score = (perfect_score + volume_score + rate_score).clamp(10.0, 99.0).round() as u32;

    let stats = LibraryStats {
        total_configs,
        active_configs,
        perfect_games,
        in_progress_games,
        not_started_games,
        total_achievements,
        unlocked_achievements,
        locked_achievements,
        completion_rate,
        ovr_score,
        steam_official_games,
        steam_official_unlocked,
        steam_official_total,
        emulator_games: custom_games,
        emulator_unlocked: custom_unlocked,
        emulator_total: custom_total,
        total_playtime_minutes,
    };

    let username = if db.preferences.username.trim().is_empty() {
        "MOHANDL3G".to_string()
    } else {
        db.preferences.username.clone()
    };

    AchievementsData {
        base_path: get_database_path_for_user(db.preferences.steam_id.as_deref()),
        preferences: Preferences {
            steam_official_steam_id: db.preferences.steam_id.clone(),
            watched_folders: Vec::new(),
            sync_settings: db.preferences.sync_settings.clone(),
        },
        username,
        stats,
        games,
    }
}

/// Sends the local database to the remote TrophyVault server via POST `/api/sync?mode={update|full}`.
pub fn sync_to_remote_with_mode(
    db: &LibraryDatabase,
    settings: &SyncSettings,
    mode: SyncMode,
) -> Result<String, String> {
    if settings.api_key.trim().is_empty() {
        return Err("API key is not configured".to_string());
    }

    let mode_param = match mode {
        SyncMode::Updates => "update",
        SyncMode::Full => "full",
    };
    let base_sync_url = settings.formatted_api_url()?;
    let url = format!("{}?mode={}", base_sync_url, mode_param);

    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", settings.api_key.trim()))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(15))
        .send_json(db);

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.into_string().unwrap_or_default();
            Ok(format!("HTTP {}: {}", status, body))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!("Server returned HTTP {}: {}", code, body))
        }
        Err(ureq::Error::Transport(err)) => {
            Err(format!("Connection error: {}", err))
        }
    }
}

/// Sends the local database to the remote TrophyVault server via POST `/api/sync`.
/// Delegates to `sync_to_remote_with_mode(db, settings, settings.sync_mode)`.
pub fn sync_to_remote(db: &LibraryDatabase, settings: &SyncSettings) -> Result<String, String> {
    sync_to_remote_with_mode(db, settings, settings.sync_mode)
}

/// Purges the remote database on the TrophyVault server via POST `/api/purge`.
pub fn purge_remote_database(settings: &SyncSettings) -> Result<String, String> {
    if settings.api_key.trim().is_empty() {
        return Err("API key is not configured".to_string());
    }

    let base_url = settings.formatted_base_url()?;
    let url = format!("{}/api/purge", base_url);

    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", settings.api_key.trim()))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(15))
        .send_string("{}");

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.into_string().unwrap_or_default();
            Ok(format!("HTTP {}: {}", status, body))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!("Server returned HTTP {}: {}", code, body))
        }
        Err(ureq::Error::Transport(err)) => {
            Err(format!("Connection error: {}", err))
        }
    }
}

/// Loads or creates the database for the currently active Steam user account.
pub fn load_active_user_database() -> LibraryDatabase {
    let steam_dir = find_steam_dir();
    let detected_users = if let Some(s_dir) = &steam_dir {
        crate::steam::detect_steam_users(s_dir)
    } else {
        Vec::new()
    };

    let active_user = detected_users
        .iter()
        .find(|u| u.is_most_recent)
        .or_else(|| detected_users.iter().find(|u| u.steam_id == "462943940"))
        .or_else(|| detected_users.first());

    let initial_steam_id = active_user.map(|u| u.steam_id.as_str());
    let mut db = load_or_create_database_for_user(initial_steam_id);
    if db.preferences.steam_id.is_none() && initial_steam_id.is_some() {
        db.preferences.steam_id = initial_steam_id.map(|s| s.to_string());
    }
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_sanitization_prevents_traversal() {
        let p_traversal = get_database_path_for_user(Some("../../../etc/passwd"));
        assert!(!p_traversal.to_string_lossy().contains("etc/passwd"));
        assert!(p_traversal.ends_with("library_database.json"));

        let p_valid = get_database_path_for_user(Some("462943940"));
        assert!(p_valid.to_string_lossy().contains("462943940"));

        let p_invalid_chars = get_database_path_for_user(Some("user;rm -rf /"));
        assert!(!p_invalid_chars.to_string_lossy().contains("rm -rf"));
    }

    #[test]
    fn test_database_serialization_and_defaults() {
        let db = LibraryDatabase {
            version: 1,
            last_updated: 1700000000,
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };

        let json = serde_json::to_string(&db).expect("Serialization must succeed");
        let parsed: LibraryDatabase = serde_json::from_str(&json).expect("Deserialization must succeed");
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.last_updated, 1700000000);
        assert!(parsed.games.is_empty());
        assert!(parsed.preferences.sync_settings.is_none());
    }

    #[test]
    fn test_sync_settings_defaults_and_serialization() {
        let default_settings = SyncSettings::default();
        assert!(!default_settings.enabled);
        assert!(default_settings.server_url.is_empty());
        assert!(default_settings.api_key.is_empty());
        assert!(!default_settings.auto_sync_on_exit);
        assert!(default_settings.last_sync_timestamp.is_none());
        assert!(default_settings.last_sync_status.is_none());
        assert_eq!(default_settings.sync_mode, SyncMode::Updates);
        assert!(!default_settings.is_configured());

        let json = serde_json::to_string(&default_settings).expect("Must serialize");
        let parsed: SyncSettings = serde_json::from_str(&json).expect("Must deserialize");
        assert_eq!(parsed.enabled, default_settings.enabled);
        assert_eq!(parsed.server_url, default_settings.server_url);
        assert_eq!(parsed.sync_mode, SyncMode::Updates);

        // Deserializing empty JSON object should yield defaults
        let from_empty: SyncSettings = serde_json::from_str("{}").expect("Must deserialize empty");
        assert!(!from_empty.enabled);
        assert!(from_empty.server_url.is_empty());
        assert_eq!(from_empty.sync_mode, SyncMode::Updates);
    }

    #[test]
    fn test_sync_settings_url_formatting() {
        let mut s = SyncSettings {
            server_url: "https://vault.mohandl3g.ly".to_string(),
            api_key: "test-token".to_string(),
            ..Default::default()
        };
        assert!(s.is_configured());
        assert_eq!(s.formatted_api_url().unwrap(), "https://vault.mohandl3g.ly/api/sync");

        // Trailing slash
        s.server_url = "https://vault.mohandl3g.ly/".to_string();
        assert_eq!(s.formatted_api_url().unwrap(), "https://vault.mohandl3g.ly/api/sync");

        // Missing scheme defaults to https
        s.server_url = "vault.mohandl3g.ly".to_string();
        assert_eq!(s.formatted_api_url().unwrap(), "https://vault.mohandl3g.ly/api/sync");

        // HTTP scheme preserved
        s.server_url = "http://localhost:5050".to_string();
        assert_eq!(s.formatted_api_url().unwrap(), "http://localhost:5050/api/sync");

        // Empty URL returns Err
        s.server_url = "   ".to_string();
        assert!(!s.is_configured());
        assert!(s.formatted_api_url().is_err());
    }

    #[test]
    fn test_database_preferences_sync_settings_backwards_compatibility() {
        let legacy_json = r#"{
            "username": "Hunter",
            "title_line": "Trophy Hunter",
            "theme": "CyberpunkCyan",
            "auto_sync": true
        }"#;

        let prefs: DatabasePreferences = serde_json::from_str(legacy_json).expect("Must deserialize legacy");
        assert!(prefs.sync_settings.is_none());

        let mut prefs_with_sync = prefs;
        prefs_with_sync.sync_settings = Some(SyncSettings {
            enabled: true,
            server_url: "https://vault.mohandl3g.ly".to_string(),
            api_key: "secret".to_string(),
            auto_sync_on_exit: true,
            last_sync_timestamp: Some(1730000000),
            last_sync_status: Some("Success".to_string()),
            sync_mode: SyncMode::Updates,
        });

        let json = serde_json::to_string(&prefs_with_sync).expect("Must serialize");
        let parsed: DatabasePreferences = serde_json::from_str(&json).expect("Must deserialize");
        assert!(parsed.sync_settings.is_some());
        let s = parsed.sync_settings.unwrap();
        assert!(s.enabled);
        assert_eq!(s.server_url, "https://vault.mohandl3g.ly");
        assert_eq!(s.last_sync_timestamp, Some(1730000000));
    }

    #[test]
    fn test_sync_to_remote_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("Must bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let mut request = Vec::new();
                while let Ok(n) = stream.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&request);
                assert!(req_str.contains("POST /api/sync"));
                assert!(req_str.contains("Authorization: Bearer test-secret"));

                let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);

                // Drain any remaining incoming data so TCP socket doesn't send RST on Windows
                let mut drain = [0u8; 1024];
                while let Ok(n) = stream.read(&mut drain) {
                    if n == 0 {
                        break;
                    }
                }
            }
        });

        let db = LibraryDatabase {
            version: 1,
            last_updated: 1700000000,
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };

        let settings = SyncSettings {
            enabled: true,
            server_url: format!("http://127.0.0.1:{}", port),
            api_key: "test-secret".to_string(),
            ..Default::default()
        };

        let result = sync_to_remote(&db, &settings);
        assert!(result.is_ok(), "Expected Ok response, got: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("200"));
        assert!(msg.contains("{\"status\":\"ok\"}"));
    }

    #[test]
    fn test_sync_to_remote_unreachable_server_returns_error() {
        let db = LibraryDatabase {
            version: 1,
            last_updated: 1700000000,
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };

        let settings = SyncSettings {
            enabled: true,
            server_url: "http://127.0.0.1:59999".to_string(),
            api_key: "test-secret".to_string(),
            ..Default::default()
        };

        let result = sync_to_remote(&db, &settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Connection error"));
    }

    #[test]
    fn test_sync_to_remote_with_mode_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("Must bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            // Handle two connections: first for update, second for full
            for _ in 0..2 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buffer = [0u8; 4096];
                    let mut request = Vec::new();
                    while let Ok(n) = stream.read(&mut buffer) {
                        if n == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }

                    let req_str = String::from_utf8_lossy(&request);
                    assert!(req_str.contains("POST /api/sync?mode="));
                    assert!(req_str.contains("Authorization: Bearer test-secret"));

                    let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}";
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Write);

                    let mut drain = [0u8; 1024];
                    while let Ok(n) = stream.read(&mut drain) {
                        if n == 0 {
                            break;
                        }
                    }
                }
            }
        });

        let db = LibraryDatabase {
            version: 1,
            last_updated: 1700000000,
            preferences: DatabasePreferences::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };

        let settings = SyncSettings {
            enabled: true,
            server_url: format!("http://127.0.0.1:{}", port),
            api_key: "test-secret".to_string(),
            ..Default::default()
        };

        let res_updates = sync_to_remote_with_mode(&db, &settings, SyncMode::Updates);
        assert!(res_updates.is_ok(), "Updates mode failed: {:?}", res_updates);

        let res_full = sync_to_remote_with_mode(&db, &settings, SyncMode::Full);
        assert!(res_full.is_ok(), "Full mode failed: {:?}", res_full);
    }

    #[test]
    fn test_purge_remote_database_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("Must bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let mut request = Vec::new();
                while let Ok(n) = stream.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_str = String::from_utf8_lossy(&request);
                assert!(req_str.contains("POST /api/purge"));
                assert!(req_str.contains("Authorization: Bearer purge-secret"));

                let response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"purged\":true}";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);

                let mut drain = [0u8; 1024];
                while let Ok(n) = stream.read(&mut drain) {
                    if n == 0 {
                        break;
                    }
                }
            }
        });

        let settings = SyncSettings {
            enabled: true,
            server_url: format!("http://127.0.0.1:{}", port),
            api_key: "purge-secret".to_string(),
            ..Default::default()
        };

        let result = purge_remote_database(&settings);
        assert!(result.is_ok(), "Purge failed: {:?}", result);
        assert!(result.unwrap().contains("{\"purged\":true}"));
    }
}
