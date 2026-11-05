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

/// Base constant used to convert between Steam64 (Community ID) and Steam32 (Account ID).
pub const STEAM64_BASE: u64 = 76561197960265728;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SteamUserAccount {
    pub steam_id: String,           // 32-bit Account ID (matches userdata/<id>)
    pub steam64_id: Option<String>,  // 64-bit Community ID
    pub persona_name: String,       // Player display name
    pub account_name: Option<String>,
    pub is_most_recent: bool,
    pub last_login_time: Option<u64>,
}

/// Convert Steam64 Community ID to Steam32 Account ID.
pub fn steam64_to_steam32(steam64: u64) -> Option<u32> {
    if steam64 > STEAM64_BASE {
        Some((steam64 - STEAM64_BASE) as u32)
    } else {
        None
    }
}

/// Convert Steam32 Account ID to Steam64 Community ID.
pub fn steam32_to_steam64(steam32: u32) -> u64 {
    STEAM64_BASE + (steam32 as u64)
}

/// Parse `loginusers.vdf` content to extract all user accounts registered with Steam.
pub fn parse_loginusers_vdf(content: &str) -> Vec<SteamUserAccount> {
    let mut accounts = Vec::new();
    let mut current_steam64: Option<String> = None;
    let mut current_persona: Option<String> = None;
    let mut current_account: Option<String> = None;
    let mut current_most_recent = false;
    let mut current_timestamp: Option<u64> = None;
    let mut in_users_section = false;
    let mut depth: usize = 0;
    let mut user_depth: usize = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "{" {
            depth += 1;
            continue;
        } else if trimmed == "}" {
            if let Some(s64) = current_steam64.take() {
                if depth == user_depth {
                    if let Ok(s64_num) = s64.parse::<u64>() {
                        if let Some(s32_num) = steam64_to_steam32(s64_num) {
                            let persona = current_persona
                                .take()
                                .filter(|p| !p.trim().is_empty())
                                .unwrap_or_else(|| format!("Steam User {}", s32_num));

                            accounts.push(SteamUserAccount {
                                steam_id: s32_num.to_string(),
                                steam64_id: Some(s64),
                                persona_name: persona,
                                account_name: current_account.take(),
                                is_most_recent: current_most_recent,
                                last_login_time: current_timestamp.take(),
                            });
                        }
                    }
                    current_most_recent = false;
                }
            }
            depth = depth.saturating_sub(1);
            if depth == 0 {
                in_users_section = false;
            }
            continue;
        }

        if !in_users_section {
            if trimmed.eq_ignore_ascii_case(r#""users""#) {
                in_users_section = true;
            }
            continue;
        }

        // Inside "users" block: A user block header is a numeric 64-bit ID e.g. "76561198423209668"
        if depth == 1 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            let key = trimmed.trim_matches('"');
            if key.chars().all(|c| c.is_ascii_digit()) && key.len() >= 16 {
                current_steam64 = Some(key.to_string());
                user_depth = 2;
                current_persona = None;
                current_account = None;
                current_most_recent = false;
                current_timestamp = None;
            }
            continue;
        }

        // Inside a user object
        if current_steam64.is_some() {
            let tokens: Vec<&str> = trimmed
                .split('"')
                .filter(|s| !s.trim().is_empty())
                .collect();

            if tokens.len() >= 2 {
                let k = tokens[0].trim();
                let v = tokens[1].trim();

                if k.eq_ignore_ascii_case("PersonaName") {
                    current_persona = Some(v.to_string());
                } else if k.eq_ignore_ascii_case("AccountName") {
                    current_account = Some(v.to_string());
                } else if k.eq_ignore_ascii_case("MostRecent") {
                    current_most_recent = v == "1";
                } else if k.eq_ignore_ascii_case("Timestamp") {
                    current_timestamp = v.parse::<u64>().ok();
                }
            } else if tokens.len() == 1 && trimmed.starts_with('"') {
                let k = tokens[0].trim();
                if let Some(second_quote_idx) = trimmed[1..].find('"') {
                    let remainder = trimmed[second_quote_idx + 2..].trim();
                    if !remainder.is_empty() {
                        if k.eq_ignore_ascii_case("MostRecent") {
                            current_most_recent = remainder == "1";
                        } else if k.eq_ignore_ascii_case("Timestamp") {
                            current_timestamp = remainder.parse::<u64>().ok();
                        }
                    }
                }
            }
        }
    }

    accounts
}

/// Detect all Steam user accounts present in the Steam installation.
/// Reads `config/loginusers.vdf` and inspects `userdata/` directory.
pub fn detect_steam_users(steam_dir: &Path) -> Vec<SteamUserAccount> {
    let mut accounts: Vec<SteamUserAccount> = Vec::new();
    let mut known_steam32: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 1. Try to read config/loginusers.vdf
    let loginusers_paths = [
        steam_dir.join("config").join("loginusers.vdf"),
        steam_dir.join("loginusers.vdf"),
    ];

    for path in &loginusers_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                let parsed = parse_loginusers_vdf(&content);
                for acc in parsed {
                    if !known_steam32.contains(&acc.steam_id) {
                        known_steam32.insert(acc.steam_id.clone());
                        accounts.push(acc);
                    }
                }
            }
            break;
        }
    }

    // 2. Scan userdata/ directories to catch any additional accounts
    let userdata = steam_dir.join("userdata");
    if userdata.exists() {
        if let Ok(entries) = fs::read_dir(&userdata) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    if let Some(folder_name) = p.file_name().and_then(|n| n.to_str()) {
                        if folder_name.chars().all(|c| c.is_ascii_digit()) && folder_name != "0" {
                            if !known_steam32.contains(folder_name) {
                                let steam32_val = folder_name.parse::<u32>().ok();
                                let steam64_str = steam32_val.map(|s| steam32_to_steam64(s).to_string());
                                let modified_sec = fs::metadata(&p)
                                    .and_then(|m| m.modified())
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs());

                                accounts.push(SteamUserAccount {
                                    steam_id: folder_name.to_string(),
                                    steam64_id: steam64_str,
                                    persona_name: format!("Steam User {}", folder_name),
                                    account_name: None,
                                    is_most_recent: false,
                                    last_login_time: modified_sec,
                                });
                                known_steam32.insert(folder_name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Sort: MostRecent == true first, then last login descending, then persona name
    accounts.sort_by(|a, b| {
        b.is_most_recent
            .cmp(&a.is_most_recent)
            .then_with(|| b.last_login_time.cmp(&a.last_login_time))
            .then_with(|| a.persona_name.cmp(&b.persona_name))
    });

    accounts
}

/// Detect the active Steam user ID from `loginusers.vdf` or `userdata/`.
pub fn find_active_steam_id(steam_dir: &Path) -> Option<String> {
    let users = detect_steam_users(steam_dir);
    // Prefer the active/most recent account, with fallback to 462943940 if present
    if let Some(user) = users.iter().find(|u| u.is_most_recent) {
        Some(user.steam_id.clone())
    } else if let Some(user) = users.iter().find(|u| u.steam_id == "462943940") {
        Some(user.steam_id.clone())
    } else {
        users.first().map(|u| u.steam_id.clone())
    }
}

fn insert_dual_keyed(results: &mut HashMap<String, SteamAppUsage>, appid: String, usage: SteamAppUsage) {
    // 1. Direct key
    results.insert(appid.clone(), usage.clone());

    // 2. If it parses as signed i64 and is negative, index its unsigned 32-bit equivalent
    if let Ok(signed_val) = appid.parse::<i64>() {
        if signed_val < 0 {
            let unsigned_32 = (signed_val as i32) as u32;
            results.insert(unsigned_32.to_string(), usage.clone());
        }
    }

    // 3. If it parses as unsigned u64, index its signed 32-bit and lower-32 equivalents
    if let Ok(unsigned_val) = appid.parse::<u64>() {
        if unsigned_val > i32::MAX as u64 && unsigned_val <= u32::MAX as u64 {
            let signed_32 = (unsigned_val as u32) as i32;
            results.insert(signed_32.to_string(), usage.clone());
        } else if unsigned_val > u32::MAX as u64 {
            let lower_32 = (unsigned_val & 0xFFFF_FFFF) as u32;
            let signed_32 = lower_32 as i32;
            results.insert(lower_32.to_string(), usage.clone());
            results.insert(signed_32.to_string(), usage);
        }
    }
}

/// Parse playtime and last played timestamps from Steam's `localconfig.vdf`.
/// Returns a map of AppID (or signed AppID) -> SteamAppUsage.
pub fn parse_localconfig_playtimes(steam_dir: &Path, steam_id: &str) -> HashMap<String, SteamAppUsage> {
    let localconfig_path = steam_dir
        .join("userdata")
        .join(steam_id)
        .join("config")
        .join("localconfig.vdf");

    if !localconfig_path.exists() {
        return HashMap::new();
    }

    let content = match fs::read_to_string(&localconfig_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    parse_localconfig_str(&content)
}

/// Parse playtime and last played timestamps from a `localconfig.vdf` string.
/// Robust against inner sub-blocks (e.g. cloud, autocloud) that otherwise clear AppIDs prematurely.
pub fn parse_localconfig_str(content: &str) -> HashMap<String, SteamAppUsage> {
    let mut results: HashMap<String, SteamAppUsage> = HashMap::new();
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
            // App block is rooted strictly at apps_depth + 1
            if in_apps_section && brace_depth == apps_depth + 1 && current_appid.is_some() {
                if current_usage.playtime_minutes > 0 || current_usage.last_played.is_some() {
                    let appid = current_appid.take().unwrap();
                    insert_dual_keyed(&mut results, appid, current_usage);
                }
                current_appid = None;
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
        let tokens: Vec<&str> = trimmed
            .split('"')
            .filter(|s| !s.trim().is_empty())
            .collect();

        if tokens.len() == 1 && brace_depth == apps_depth {
            // New app entry: e.g. "760" or "2485153835" or "-1809813461"
            let id = tokens[0].trim();
            if id.parse::<i64>().is_ok() {
                if let Some(prev_id) = current_appid.take() {
                    if current_usage.playtime_minutes > 0 || current_usage.last_played.is_some() {
                        insert_dual_keyed(&mut results, prev_id, current_usage);
                    }
                }
                current_appid = Some(id.to_string());
                current_usage = SteamAppUsage::default();
            }
        } else if tokens.len() >= 2 && current_appid.is_some() {
            let key = tokens[0].trim();
            let val = tokens[1].trim();

            if key.eq_ignore_ascii_case("Playtime") {
                if let Ok(mins) = val.parse::<u32>() {
                    if mins > current_usage.playtime_minutes {
                        current_usage.playtime_minutes = mins;
                    }
                }
            } else if key.eq_ignore_ascii_case("PlaytimeDisconnected") {
                if let Ok(mins) = val.parse::<u32>() {
                    if current_usage.playtime_minutes == 0 {
                        current_usage.playtime_minutes = mins;
                    }
                }
            } else if key.eq_ignore_ascii_case("LastPlayed") {
                if let Ok(ts) = val.parse::<u64>() {
                    current_usage.last_played = Some(ts);
                }
            }
        }
    }

    // Flush any pending app at EOF
    if let Some(id) = current_appid.take() {
        if current_usage.playtime_minutes > 0 || current_usage.last_played.is_some() {
            insert_dual_keyed(&mut results, id, current_usage);
        }
    }

    results
}

/// Lookup playtime for a given AppID, handling signed/unsigned 32-bit conversion.
pub fn lookup_playtime(playtime_map: &HashMap<String, SteamAppUsage>, appid: &str) -> Option<SteamAppUsage> {
    // 1. Direct match (covers dual-indexed signed and unsigned keys)
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

        // Lower 32 bits (for 64-bit Steam shortcut IDs)
        let lower_32 = (unsigned_id & 0xFFFF_FFFF) as u32;
        let lower_str = lower_32.to_string();
        if let Some(usage) = playtime_map.get(&lower_str) {
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

    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_localconfig_str_with_nested_blocks() {
        let vdf = r#"
"UserLocalConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"apps"
				{
					"760"
					{
						"cloud"
						{
							"quota_bytes"		"20000000000"
							"quota_files"		"50000"
							"last_sync_state"	"synchronized"
						}
						"LastPlayed"		"1781159675"
						"autocloud"
						{
							"lastexit"		"1781155969"
						}
						"Playtime"		"1"
					}
					"-1809813461"
					{
						"LastPlayed"		"1793898983"
						"Playtime"		"1605"
					}
					"2280"
					{
						"Playtime"		"13613"
						"PlaytimeDisconnected"	"121"
					}
				}
			}
		}
	}
}
"#;

        let results = parse_localconfig_str(vdf);

        // App 760 with nested cloud and autocloud blocks
        assert!(results.contains_key("760"), "App 760 must not be cleared by inner blocks");
        let app_760 = results.get("760").unwrap();
        assert_eq!(app_760.playtime_minutes, 1);
        assert_eq!(app_760.last_played, Some(1781159675));

        // Negative signed Non-Steam AppID (-1809813461 <-> 2485153835)
        assert!(results.contains_key("-1809813461"), "Negative signed AppID must exist");
        assert!(results.contains_key("2485153835"), "Unsigned equivalent must be dual-indexed");
        let bb_signed = results.get("-1809813461").unwrap();
        let bb_unsigned = results.get("2485153835").unwrap();
        assert_eq!(bb_signed.playtime_minutes, 1605);
        assert_eq!(bb_unsigned.playtime_minutes, 1605);

        // App 2280 (DOOM)
        let doom = results.get("2280").unwrap();
        assert_eq!(doom.playtime_minutes, 13613);

        // Lookup helper bidirectional tests
        let lookup_by_unsigned = lookup_playtime(&results, "2485153835");
        assert!(lookup_by_unsigned.is_some());
        assert_eq!(lookup_by_unsigned.unwrap().playtime_minutes, 1605);

        let lookup_by_signed = lookup_playtime(&results, "-1809813461");
        assert!(lookup_by_signed.is_some());
        assert_eq!(lookup_by_signed.unwrap().playtime_minutes, 1605);
    }

    #[test]
    fn test_lookup_playtime_64bit_fallback() {
        let mut map = HashMap::new();
        let usage = SteamAppUsage {
            playtime_minutes: 350,
            last_played: Some(1700000000),
        };
        // Insert as unsigned 32-bit (e.g. 2485153835)
        map.insert("2485153835".to_string(), usage.clone());

        // Test querying with signed 32-bit "-1809813461"
        let found = lookup_playtime(&map, "-1809813461");
        assert!(found.is_some());
        assert_eq!(found.unwrap().playtime_minutes, 350);

        // Test querying with 64-bit Steam ID containing 2485153835 in lower 32 bits
        let full_64: u64 = (0x80000000u64 << 32) | 2485153835u64;
        let found_64 = lookup_playtime(&map, &full_64.to_string());
        assert!(found_64.is_some());
        assert_eq!(found_64.unwrap().playtime_minutes, 350);
    }
}
