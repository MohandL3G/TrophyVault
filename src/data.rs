//! Data structures and reader for Achievements application files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub version: Option<u32>,
    #[serde(default)]
    pub entries: HashMap<String, DashboardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEntry {
    pub platform: Option<String>,
    pub appid: Option<String>,
    #[serde(default)]
    pub pct: u32,
    #[serde(default)]
    pub unlocked: u32,
    #[serde(default)]
    pub total: u32,
    pub verified: Option<bool>,
    pub platinum: Option<bool>,
    pub updated: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    #[serde(rename = "steamOfficialSteamId")]
    pub steam_official_steam_id: Option<String>,
    #[serde(rename = "watchedFolders", default)]
    pub watched_folders: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GameItem {
    pub name: String,
    pub platform: String,
    pub appid: Option<String>,
    pub unlocked: u32,
    pub total: u32,
    pub pct: u32,
    pub platinum: bool,
}

#[derive(Debug, Clone)]
pub struct LibraryStats {
    pub total_configs: u32,
    pub active_configs: u32,
    pub perfect_games: u32,
    pub in_progress_games: u32,
    pub not_started_games: u32,
    pub total_achievements: u32,
    pub unlocked_achievements: u32,
    pub locked_achievements: u32,
    pub completion_rate: f64,
    pub ovr_score: u32,

    pub steam_official_games: u32,
    pub steam_official_unlocked: u32,
    pub steam_official_total: u32,

    pub emulator_games: u32,
    pub emulator_unlocked: u32,
    pub emulator_total: u32,
}

#[derive(Debug, Clone)]
pub struct AchievementsData {
    pub base_path: PathBuf,
    pub preferences: Preferences,
    pub username: String,
    pub stats: LibraryStats,
    pub games: Vec<GameItem>,
}

/// Detect the default directory for the Achievements app.
#[allow(dead_code)]
pub fn default_achievements_dir() -> PathBuf {
    // 1. Check APPDATA environment variable on Windows
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("Achievements");
        if p.exists() {
            return p;
        }
    }

    // 2. Check XDG / Linux home directory
    if let Some(config_dir) = dirs::config_dir() {
        let p = config_dir.join("Achievements");
        if p.exists() {
            return p;
        }
    }

    // 3. Fallback to local sample or working directory
    let local = PathBuf::from("Achievements");
    if local.exists() {
        return local;
    }

    PathBuf::from(".")
}

/// Attempt to read the Steam persona name from loginusers.vdf
fn resolve_steam_persona(steam_id: &str) -> Option<String> {
    let candidates = [
        PathBuf::from(r"C:\Program Files (x86)\Steam\config\loginusers.vdf"),
        PathBuf::from(r"C:\Program Files\Steam\config\loginusers.vdf"),
    ];

    for path in &candidates {
        if let Ok(content) = fs::read_to_string(path) {
            // Find the block for this steam_id
            if let Some(id_pos) = content.find(steam_id) {
                let slice = &content[id_pos..];
                if let Some(persona_pos) = slice.find(r#""PersonaName""#) {
                    let after = &slice[persona_pos + 13..];
                    let parts: Vec<&str> = after.split('"').collect();
                    if parts.len() >= 2 {
                        let name = parts[1].trim();
                        if !name.is_empty() {
                            return Some(name.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Load and parse Achievements data from the specified folder.
#[allow(dead_code)]
pub fn load_from_dir(dir: &Path) -> Result<AchievementsData, String> {
    let summary_path = dir.join("dashboard-summary.json");
    if !summary_path.exists() {
        return Err(format!(
            "dashboard-summary.json not found in '{}'",
            dir.display()
        ));
    }

    let summary_raw = fs::read_to_string(&summary_path)
        .map_err(|e| format!("Failed to read dashboard-summary.json: {}", e))?;

    let summary: DashboardSummary = serde_json::from_str(&summary_raw)
        .map_err(|e| format!("Failed to parse dashboard-summary.json: {}", e))?;

    // Load preferences if available
    let pref_path = dir.join("preferences.json");
    let preferences: Preferences = if pref_path.exists() {
        fs::read_to_string(&pref_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        Preferences::default()
    };

    // Resolve Persona Name or default to MohandL3G
    let username = preferences
        .steam_official_steam_id
        .as_deref()
        .and_then(resolve_steam_persona)
        .unwrap_or_else(|| "MOHANDL3G".to_string());

    // Aggregate statistics
    let mut perfect_games = 0;
    let mut in_progress_games = 0;
    let mut not_started_games = 0;
    let mut total_achievements = 0;
    let mut unlocked_achievements = 0;

    let mut steam_official_games = 0;
    let mut steam_official_unlocked = 0;
    let mut steam_official_total = 0;

    let mut emulator_games = 0;
    let mut emulator_unlocked = 0;
    let mut emulator_total = 0;

    let mut games: Vec<GameItem> = Vec::with_capacity(summary.entries.len());

    for (name, entry) in summary.entries {
        let platform = entry.platform.unwrap_or_else(|| "unknown".to_string());
        let unlocked = entry.unlocked;
        let total = entry.total;
        let pct = if total > 0 {
            entry.pct.max((unlocked * 100) / total)
        } else {
            0
        };

        let platinum = entry.platinum.unwrap_or(false) || (total > 0 && unlocked == total);

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

            if platform == "steam-official" {
                steam_official_games += 1;
                steam_official_unlocked += unlocked;
                steam_official_total += total;
            } else {
                emulator_games += 1;
                emulator_unlocked += unlocked;
                emulator_total += total;
            }
        } else {
            not_started_games += 1;
        }

        games.push(GameItem {
            name,
            platform,
            appid: entry.appid,
            unlocked,
            total,
            pct,
            platinum,
        });
    }

    // Sort games: in-progress & perfect first, then by unlocked count descending
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
        emulator_games,
        emulator_unlocked,
        emulator_total,
    };

    Ok(AchievementsData {
        base_path: dir.to_path_buf(),
        preferences,
        username,
        stats,
        games,
    })
}
