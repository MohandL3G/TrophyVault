//! Persistent JSON database and synchronization engine.
//! Stored in `%APPDATA%\AchievementsPassport\library_database.json`.

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
    #[serde(default = "default_true")]
    pub auto_sync: bool,
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
            auto_sync: true,
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

/// Resolves the absolute path to `library_database.json` in `%APPDATA%\AchievementsPassport`.
pub fn get_database_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata)
            .join("AchievementsPassport")
            .join("library_database.json");
    }

    if let Some(config_dir) = dirs::config_dir() {
        return config_dir
            .join("AchievementsPassport")
            .join("library_database.json");
    }

    PathBuf::from("AchievementsPassport").join("library_database.json")
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Load the persistent database from disk or initialize it with default configs.
pub fn load_or_create_database() -> LibraryDatabase {
    let db_path = get_database_path();

    if db_path.exists() {
        if let Ok(content) = fs::read_to_string(&db_path) {
            if let Ok(mut db) = serde_json::from_str::<LibraryDatabase>(&content) {
                // Sanitize and normalize database entries
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

                if changed {
                    let _ = save_database(&db);
                }
                return db;
            }
        }
    }

    // Initialize fresh database
    let mut db = LibraryDatabase {
        version: 1,
        last_updated: current_timestamp(),
        preferences: DatabasePreferences::default(),
        manual_games: vec![
            ManualGameSetting {
                id: "bloodborne".to_string(),
                name: "Bloodborne (PC Recomp)".to_string(),
                platform: "PC Port / Recomp".to_string(),
                parser_type: "bbport".to_string(),
                user_dir: r"C:\Games\Decomps\Bloodborne\user".to_string(),
                non_steam_appid: Some("2485153835".to_string()),
                enabled: true,
            },
        ],
        games: HashMap::new(),
    };

    // Attempt to seed from legacy Achievements app if dashboard-summary.json exists
    seed_from_legacy_achievements(&mut db);

    // Save initial DB
    let _ = save_database(&db);
    db
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

/// Save the database to `%APPDATA%\AchievementsPassport\library_database.json`.
pub fn save_database(db: &LibraryDatabase) -> Result<(), String> {
    let db_path = get_database_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
    }

    let json_str = serde_json::to_string_pretty(db)
        .map_err(|e| format!("Failed to serialize database: {}", e))?;

    fs::write(&db_path, json_str)
        .map_err(|e| format!("Failed to write database file '{}': {}", db_path.display(), e))?;

    Ok(())
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

    // 2. Scan configured Manual Games (e.g. Bloodborne)
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

    // 3. Scan LuaTools games from stplug-in
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

    // 4. Update playtimes and timestamps for all games matching localconfig.vdf
    for entry in db.games.values_mut() {
        if let Some(appid) = &entry.appid {
            if let Some(usage) = lookup_playtime(&playtimes, appid) {
                if usage.playtime_minutes > 0 {
                    entry.playtime_minutes = usage.playtime_minutes;
                }
                if usage.last_played.is_some() {
                    entry.last_played = usage.last_played;
                }
                report.steam_updated += 1;
            }
        }
    }

    // 5. Update shortcuts discovery
    if let (Some(s_dir), Some(s_id)) = (&steam_dir, &steam_id) {
        let shortcuts = parse_non_steam_shortcuts(s_dir, s_id);
        for sc in shortcuts {
            // Check if this shortcut matches any manual game
            let sc_appid_str = sc.appid.to_string();
            for manual in &mut db.manual_games {
                if manual.non_steam_appid.is_none() && sc.appname.to_lowercase().contains(&manual.id) {
                    manual.non_steam_appid = Some(sc_appid_str.clone());
                }
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

    let mut games: Vec<GameItem> = Vec::with_capacity(db.games.len());

    for entry in db.games.values() {
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
    };

    let username = if db.preferences.username.trim().is_empty() {
        "MOHANDL3G".to_string()
    } else {
        db.preferences.username.clone()
    };

    AchievementsData {
        base_path: get_database_path(),
        preferences: Preferences {
            steam_official_steam_id: db.preferences.steam_id.clone(),
            watched_folders: Vec::new(),
        },
        username,
        stats,
        games,
    }
}
