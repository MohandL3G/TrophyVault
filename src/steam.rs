//! Steam integration: paths detection, localconfig.vdf playtime parser,
//! binary shortcuts.vdf parser, and LuaTools / stplug-in reader.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct SteamAppUsage {
    pub playtime_minutes: u32,
    pub last_played: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct NonSteamShortcut {
    pub appname: String,
    pub exe: String,
    pub start_dir: String,
    pub appid: u32,
}

#[derive(Debug, Clone)]
pub struct LuaToolsGame {
    pub appid: String,
    pub name: String,
    pub file_path: PathBuf,
}

/// Detect the root directory of the Steam installation.
pub fn find_steam_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("STEAM_PATH") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }

    let candidates = [
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
        dirs::home_dir().map(|h| h.join(".steam").join("steam")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join(".local").join("share").join("Steam")).unwrap_or_default(),
    ];

    for path in &candidates {
        if path.exists() && path.join("userdata").exists() {
            return Some(path.clone());
        }
    }

    None
}

/// Detect the active Steam user ID from the `userdata` directory.
pub fn find_active_steam_id(steam_dir: &Path) -> Option<String> {
    let userdata = steam_dir.join("userdata");
    if !userdata.exists() {
        return None;
    }

    let mut candidate_ids: Vec<(String, std::time::SystemTime)> = Vec::new();

    if let Ok(entries) = fs::read_dir(&userdata) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Check if numeric
                    if name.chars().all(|c| c.is_ascii_digit()) && name != "0" {
                        let modified = fs::metadata(&path)
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        candidate_ids.push((name.to_string(), modified));
                    }
                }
            }
        }
    }

    // Sort by most recently modified
    candidate_ids.sort_by(|a, b| b.1.cmp(&a.1));

    // Prefer 462943940 if present, else most recent
    if let Some((id, _)) = candidate_ids.iter().find(|(id, _)| id == "462943940") {
        Some(id.clone())
    } else {
        candidate_ids.first().map(|(id, _)| id.clone())
    }
}

/// Parse playtime and last played timestamps from Steam's `localconfig.vdf`.
/// Returns a map of AppID (or signed AppID) -> SteamAppUsage.
pub fn parse_localconfig_playtimes(steam_dir: &Path, steam_id: &str) -> HashMap<String, SteamAppUsage> {
    let mut results: HashMap<String, SteamAppUsage> = HashMap::new();
    let localconfig_path = steam_dir.join("userdata").join(steam_id).join("config").join("localconfig.vdf");

    if !localconfig_path.exists() {
        return results;
    }

    let content = match fs::read_to_string(&localconfig_path) {
        Ok(c) => c,
        Err(_) => return results,
    };

    let mut current_appid: Option<String> = None;
    let mut current_usage = SteamAppUsage::default();
    let mut in_apps_section = false;
    let mut brace_depth = 0;
    let mut apps_depth = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "{" {
            brace_depth += 1;
            continue;
        } else if trimmed == "}" {
            if let Some(appid) = current_appid.take() {
                if current_usage.playtime_minutes > 0 || current_usage.last_played.is_some() {
                    results.insert(appid, current_usage);
                }
                current_usage = SteamAppUsage::default();
            }
            brace_depth -= 1;
            if in_apps_section && brace_depth < apps_depth {
                in_apps_section = false;
            }
            continue;
        }

        if !in_apps_section {
            if trimmed.eq_ignore_ascii_case(r#""apps""#) {
                in_apps_section = true;
                apps_depth = brace_depth + 1;
            }
            continue;
        }

        // Inside "apps"
        // Check for new AppID key: e.g. "105600" or "-1809813461"
        if trimmed.starts_with('"') && !trimmed.contains('\t') && !trimmed[1..].contains('"') {
            // Continuation or invalid
            continue;
        }

        let tokens: Vec<&str> = trimmed
            .split('"')
            .filter(|s| !s.trim().is_empty())
            .collect();

        if tokens.len() == 1 && brace_depth == apps_depth {
            // New app entry: e.g. "105600" or "2485153835" or "-1809813461"
            let id = tokens[0].trim();
            if id.parse::<i64>().is_ok() {
                current_appid = Some(id.to_string());
                current_usage = SteamAppUsage::default();
            }
        } else if tokens.len() >= 2 {
            let key = tokens[0].trim();
            let val = tokens[1].trim();

            if key.eq_ignore_ascii_case("Playtime") {
                if let Ok(mins) = val.parse::<u32>() {
                    current_usage.playtime_minutes = mins;
                }
            } else if key.eq_ignore_ascii_case("LastPlayed") {
                if let Ok(ts) = val.parse::<u64>() {
                    current_usage.last_played = Some(ts);
                }
            }
        }
    }

    results
}

/// Lookup playtime for a given AppID, handling signed/unsigned 32-bit conversion.
pub fn lookup_playtime(playtime_map: &HashMap<String, SteamAppUsage>, appid: &str) -> Option<SteamAppUsage> {
    // 1. Direct match
    if let Some(usage) = playtime_map.get(appid) {
        return Some(usage.clone());
    }

    // 2. Check signed 32-bit integer conversion (e.g. 2485153835 -> -1809813461)
    if let Ok(unsigned_id) = appid.parse::<u64>() {
        let signed_32 = (unsigned_id as u32) as i32;
        let signed_str = signed_32.to_string();
        if let Some(usage) = playtime_map.get(&signed_str) {
            return Some(usage.clone());
        }
    }

    // 3. Check unsigned 32-bit integer conversion (e.g. -1809813461 -> 2485153835)
    if let Ok(signed_id) = appid.parse::<i32>() {
        let unsigned_32 = signed_id as u32;
        let unsigned_str = unsigned_32.to_string();
        if let Some(usage) = playtime_map.get(&unsigned_str) {
            return Some(usage.clone());
        }
    }

    None
}

/// Parse Steam's binary `shortcuts.vdf` to discover Non-Steam games.
pub fn parse_non_steam_shortcuts(steam_dir: &Path, steam_id: &str) -> Vec<NonSteamShortcut> {
    let mut list = Vec::new();
    let shortcuts_path = steam_dir
        .join("userdata")
        .join(steam_id)
        .join("config")
        .join("shortcuts.vdf");

    if !shortcuts_path.exists() {
        return list;
    }

    let bytes = match fs::read(&shortcuts_path) {
        Ok(b) => b,
        Err(_) => return list,
    };

    let len = bytes.len();

    // Helper closure to read null-terminated ASCII string
    let read_cstring = |cur: &mut usize| -> Option<String> {
        let start = *cur;
        while *cur < len && bytes[*cur] != 0 {
            *cur += 1;
        }
        if *cur < len {
            let s = String::from_utf8_lossy(&bytes[start..*cur]).to_string();
            *cur += 1; // skip null
            Some(s)
        } else {
            None
        }
    };

    // Expect header 0x00 "shortcuts" \0
    if len < 12 || bytes[0] != 0 {
        return list;
    }
    let mut cursor = 1;
    let header = read_cstring(&mut cursor);
    if header.as_deref() != Some("shortcuts") {
        return list;
    }

    while cursor < len {
        let tag = bytes[cursor];
        cursor += 1;

        if tag == 0x08 {
            // End of section
            break;
        }

        if tag == 0x00 {
            // New shortcut entry (index: "0", "1", etc.)
            let _index = read_cstring(&mut cursor);

            let mut appname = String::new();
            let mut exe = String::new();
            let mut start_dir = String::new();
            let mut appid: u32 = 0;

            // Read key-value pairs until 0x08
            while cursor < len {
                let kv_tag = bytes[cursor];
                cursor += 1;

                if kv_tag == 0x08 {
                    break;
                }

                let key = read_cstring(&mut cursor).unwrap_or_default();

                match kv_tag {
                    0x01 => {
                        // String value
                        let val = read_cstring(&mut cursor).unwrap_or_default();
                        if key.eq_ignore_ascii_case("AppName") {
                            appname = val;
                        } else if key.eq_ignore_ascii_case("Exe") {
                            exe = val;
                        } else if key.eq_ignore_ascii_case("StartDir") {
                            start_dir = val;
                        }
                    }
                    0x02 => {
                        // 32-bit integer
                        if cursor + 4 <= len {
                            let val = u32::from_le_bytes([
                                bytes[cursor],
                                bytes[cursor + 1],
                                bytes[cursor + 2],
                                bytes[cursor + 3],
                            ]);
                            cursor += 4;
                            if key.eq_ignore_ascii_case("appid") {
                                appid = val;
                            }
                        }
                    }
                    0x00 => {
                        // Sub-object (skip until 0x08)
                        let mut depth = 1;
                        while cursor < len && depth > 0 {
                            if bytes[cursor] == 0x00 {
                                depth += 1;
                            } else if bytes[cursor] == 0x08 {
                                depth -= 1;
                            }
                            cursor += 1;
                        }
                    }
                    _ => {
                        // Skip unknown
                    }
                }
            }

            if !appname.is_empty() {
                list.push(NonSteamShortcut {
                    appname,
                    exe,
                    start_dir,
                    appid,
                });
            }
        }
    }

    list
}

/// Scan `config/stplug-in` directory for games managed by LuaTools.
pub fn scan_luatools_games(steam_dir: &Path) -> Vec<LuaToolsGame> {
    let mut list = Vec::new();
    let stplugin_dir = steam_dir.join("config").join("stplug-in");

    if !stplugin_dir.exists() {
        return list;
    }

    if let Ok(entries) = fs::read_dir(&stplugin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                if file_stem.chars().all(|c| c.is_ascii_digit()) {
                    let mut game_name = file_stem.to_string();

                    // Parse name from comment header in lua file
                    if let Ok(content) = fs::read_to_string(&path) {
                        for line in content.lines().take(15) {
                            let trimmed = line.trim();
                            if trimmed.starts_with("--") && !trimmed.contains("Created by") && !trimmed.contains("Depots:") && !trimmed.contains("Website:") {
                                let stripped = trimmed.trim_start_matches('-').trim();
                                if !stripped.is_empty() && !stripped.contains("Lua and Manifest") && !stripped.contains("MAIN") {
                                    game_name = stripped.to_string();
                                    break;
                                }
                            }
                        }
                    }

                    list.push(LuaToolsGame {
                        appid: file_stem.to_string(),
                        name: game_name,
                        file_path: path,
                    });
                }
            }
        }
    }

    list
}
