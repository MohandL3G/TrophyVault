//! Steam Web API ingestion client and SQLite merge logic.
//!
//! Fetches owned games, player summary, game achievement schemas, and unlocked
//! achievements with timestamps directly from the Steam Web API, while
//! preserving monotonic high-watermark playtimes and existing PC Port/manual
//! entries (such as Bloodborne).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use trophyvault_core::storage::{LibraryDatabase, SavedGameEntry, SavedTrophy};

use crate::state::{current_timestamp, SharedState};

const STEAM_REQUEST_TIMEOUT_SECS: u64 = 15;
const STEAM_CONCURRENCY_LIMIT: usize = 6;

#[derive(Debug)]
pub enum SteamApiError {
    Http(String),
    RateLimited,
    Parse(String),
    InvalidConfig(String),
}

impl std::fmt::Display for SteamApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SteamApiError::Http(msg) => write!(f, "Steam API HTTP error: {}", msg),
            SteamApiError::RateLimited => write!(f, "Steam API rate limit (HTTP 429)"),
            SteamApiError::Parse(msg) => write!(f, "Steam API parse error: {}", msg),
            SteamApiError::InvalidConfig(msg) => write!(f, "Steam configuration error: {}", msg),
        }
    }
}

impl std::error::Error for SteamApiError {}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPlayerSummariesResponse {
    pub response: PlayerSummariesContainer,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerSummariesContainer {
    #[serde(default)]
    pub players: Vec<SteamPlayerSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamPlayerSummary {
    pub steamid: String,
    #[serde(default)]
    pub personaname: String,
    #[serde(default)]
    pub profileurl: String,
    #[serde(default)]
    pub avatarfull: String,
    #[serde(default)]
    pub avatarmedium: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetOwnedGamesResponse {
    pub response: OwnedGamesContainer,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OwnedGamesContainer {
    pub game_count: Option<u32>,
    #[serde(default)]
    pub games: Vec<SteamOwnedGame>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamOwnedGame {
    pub appid: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub playtime_forever: Option<u32>,
    #[serde(default)]
    pub rtime_last_played: Option<u64>,
    #[serde(default)]
    pub has_community_visible_stats: Option<bool>,
    #[serde(default)]
    pub img_icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetSchemaForGameResponse {
    pub game: Option<SteamGameSchema>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamGameSchema {
    #[serde(rename = "gameName")]
    pub game_name: Option<String>,
    #[serde(rename = "availableGameStats")]
    pub available_game_stats: Option<SteamAvailableGameStats>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamAvailableGameStats {
    #[serde(default)]
    pub achievements: Vec<SteamSchemaAchievement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamSchemaAchievement {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub hidden: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPlayerAchievementsResponse {
    pub playerstats: Option<SteamPlayerStats>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamPlayerStats {
    #[serde(rename = "steamID")]
    pub steam_id: Option<String>,
    #[serde(rename = "gameName")]
    pub game_name: Option<String>,
    #[serde(default)]
    pub success: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub achievements: Vec<SteamPlayerAchievement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamPlayerAchievement {
    pub apiname: String,
    #[serde(default)]
    pub achieved: u8,
    #[serde(default)]
    pub unlocktime: u64,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Async client for querying Valve's Steam Web API.
#[derive(Clone)]
pub struct SteamClient {
    http: reqwest::Client,
    base_url: String,
}

impl SteamClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, SteamApiError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(STEAM_REQUEST_TIMEOUT_SECS))
            .user_agent(concat!("TrophyVault/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| SteamApiError::Http(e.to_string()))?;

        let trimmed = base_url.into().trim().trim_end_matches('/').to_string();
        Ok(Self {
            http,
            base_url: trimmed,
        })
    }

    /// Fetches player profile summary from `ISteamUser/GetPlayerSummaries/v0002/`.
    pub async fn fetch_player_summary(
        &self,
        api_key: &str,
        steam_id: &str,
    ) -> Result<Option<SteamPlayerSummary>, SteamApiError> {
        let url = format!(
            "{}/ISteamUser/GetPlayerSummaries/v0002/?key={}&steamids={}&format=json",
            self.base_url, api_key, steam_id
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SteamApiError::Http(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SteamApiError::RateLimited);
        }
        if !resp.status().is_success() {
            return Err(SteamApiError::Http(format!(
                "GetPlayerSummaries returned HTTP {}",
                resp.status()
            )));
        }

        let parsed: GetPlayerSummariesResponse = resp
            .json()
            .await
            .map_err(|e| SteamApiError::Parse(e.to_string()))?;
        Ok(parsed.response.players.into_iter().next())
    }

    /// Fetches owned games and total playtimes from `IPlayerService/GetOwnedGames/v0001/`.
    pub async fn fetch_owned_games(
        &self,
        api_key: &str,
        steam_id: &str,
    ) -> Result<Vec<SteamOwnedGame>, SteamApiError> {
        let url = format!(
            "{}/IPlayerService/GetOwnedGames/v0001/?key={}&steamid={}&include_appinfo=1&include_played_free_games=1&format=json",
            self.base_url, api_key, steam_id
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SteamApiError::Http(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SteamApiError::RateLimited);
        }
        if !resp.status().is_success() {
            return Err(SteamApiError::Http(format!(
                "GetOwnedGames returned HTTP {}",
                resp.status()
            )));
        }

        let parsed: GetOwnedGamesResponse = resp
            .json()
            .await
            .map_err(|e| SteamApiError::Parse(e.to_string()))?;
        Ok(parsed.response.games)
    }

    /// Fetches achievement schema from `ISteamUserStats/GetSchemaForGame/v0002/`.
    /// Returns an empty list gracefully for apps with no schema or stats.
    pub async fn fetch_game_schema(
        &self,
        api_key: &str,
        appid: u64,
    ) -> Result<Vec<SteamSchemaAchievement>, SteamApiError> {
        let url = format!(
            "{}/ISteamUserStats/GetSchemaForGame/v0002/?key={}&appid={}&l=english&format=json",
            self.base_url, api_key, appid
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SteamApiError::Http(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SteamApiError::RateLimited);
        }
        if status == reqwest::StatusCode::BAD_REQUEST
            || status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::NOT_FOUND
        {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(SteamApiError::Http(format!(
                "GetSchemaForGame({}) returned HTTP {}",
                appid, status
            )));
        }

        let parsed: GetSchemaForGameResponse = match resp.json().await {
            Ok(p) => p,
            Err(_) => return Ok(Vec::new()),
        };

        Ok(parsed
            .game
            .and_then(|g| g.available_game_stats)
            .map(|s| s.achievements)
            .unwrap_or_default())
    }

    /// Fetches user unlocked achievements + timestamps from `ISteamUserStats/GetPlayerAchievements/v0001/`.
    /// Returns `Ok(None)` gracefully for private profiles or games without achievements.
    pub async fn fetch_player_achievements(
        &self,
        api_key: &str,
        steam_id: &str,
        appid: u64,
    ) -> Result<Option<Vec<SteamPlayerAchievement>>, SteamApiError> {
        let url = format!(
            "{}/ISteamUserStats/GetPlayerAchievements/v0001/?key={}&steamid={}&appid={}&l=english&format=json",
            self.base_url, api_key, steam_id, appid
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SteamApiError::Http(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SteamApiError::RateLimited);
        }
        // Valve returns 400/403 for games without stats or private game details
        if status == reqwest::StatusCode::BAD_REQUEST
            || status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::NOT_FOUND
        {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(SteamApiError::Http(format!(
                "GetPlayerAchievements({}) returned HTTP {}",
                appid, status
            )));
        }

        let parsed: GetPlayerAchievementsResponse = match resp.json().await {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        match parsed.playerstats {
            Some(stats) if stats.success => Ok(Some(stats.achievements)),
            _ => Ok(None),
        }
    }
}

/// Combines Steam game achievement schema and player achievement status into `SavedTrophy` entries.
pub fn build_trophies_from_steam(
    schema: &[SteamSchemaAchievement],
    player: &[SteamPlayerAchievement],
) -> Vec<SavedTrophy> {
    if schema.is_empty() && player.is_empty() {
        return Vec::new();
    }

    let player_map: HashMap<&str, &SteamPlayerAchievement> = player
        .iter()
        .map(|p| (p.apiname.as_str(), p))
        .collect();

    if !schema.is_empty() {
        return schema
            .iter()
            .map(|s| {
                let p_opt = player_map.get(s.name.as_str()).copied();
                let unlocked = p_opt.map(|p| p.achieved == 1).unwrap_or(false);
                let unlocked_time = if unlocked {
                    p_opt
                        .map(|p| p.unlocktime)
                        .filter(|&t| t > 0)
                        .map(|t| t as i64)
                } else {
                    None
                };

                let title = s
                    .display_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        p_opt
                            .and_then(|p| p.name.as_deref())
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| s.name.clone());

                let description = s
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        p_opt
                            .and_then(|p| p.description.as_deref())
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| {
                        if s.hidden.unwrap_or(0) == 1 {
                            "Hidden achievement".to_string()
                        } else {
                            String::new()
                        }
                    });

                let grade = if s.hidden.unwrap_or(0) == 1 {
                    "Silver".to_string()
                } else {
                    "Bronze".to_string()
                };

                SavedTrophy {
                    id: s.name.clone(),
                    name: title,
                    description,
                    grade,
                    unlocked,
                    unlocked_time,
                }
            })
            .collect();
    }

    // Fallback when schema was unavailable but playerstats returned achievements
    player
        .iter()
        .map(|p| {
            let unlocked = p.achieved == 1;
            let unlocked_time = if unlocked && p.unlocktime > 0 {
                Some(p.unlocktime as i64)
            } else {
                None
            };
            SavedTrophy {
                id: p.apiname.clone(),
                name: p
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .unwrap_or(&p.apiname)
                    .to_string(),
                description: p
                    .description
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string(),
                grade: "Bronze".to_string(),
                unlocked,
                unlocked_time,
            }
        })
        .collect()
}

/// Returns true for Bloodborne / custom PC ports / non-Steam shortcuts that must never be overwritten by Steam Web API data.
pub fn is_protected_non_steam_entry(entry: &SavedGameEntry) -> bool {
    let plat_lower = entry.platform.to_lowercase();
    entry.source == "manual"
        || entry.source == "shortcut"
        || plat_lower.contains("port")
        || plat_lower.contains("recomp")
        || plat_lower.contains("decomp")
        || plat_lower.contains("non-steam")
}

/// Merges a pulled Steam game and optional trophies into the `LibraryDatabase`,
/// preserving monotonic `MAX` playtime and existing non-Steam / Bloodborne records.
pub fn merge_steam_game_entry(
    db: &mut LibraryDatabase,
    owned: &SteamOwnedGame,
    fetched_trophies: Option<Vec<SavedTrophy>>,
    now: u64,
) {
    let appid_str = owned.appid.to_string();
    let incoming_playtime = owned.playtime_forever.unwrap_or(0);
    let incoming_last_played = owned.rtime_last_played.filter(|&t| t > 0);
    let incoming_name = owned
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Find target key by exact appid or existing non-protected Steam entry matching appid
    let target_key = if let Some(existing) = db.games.get(&appid_str) {
        if is_protected_non_steam_entry(existing) {
            return;
        }
        appid_str.clone()
    } else if let Some(found_key) = db
        .games
        .iter()
        .find(|(_, g)| g.appid.as_deref() == Some(appid_str.as_str()) && !is_protected_non_steam_entry(g))
        .map(|(k, _)| k.clone())
    {
        found_key
    } else {
        appid_str.clone()
    };

    if let Some(existing) = db.games.get_mut(&target_key) {
        if let Some(name) = incoming_name {
            existing.name = name;
        }
        existing.platform = "Steam".to_string();
        existing.source = "steam".to_string();
        existing.appid = Some(appid_str);

        // Monotonic playtime protection: never decrease playtime
        existing.playtime_minutes = incoming_playtime
            .max(existing.playtime_minutes)
            .max(existing.base_playtime_minutes);

        existing.last_played = match (incoming_last_played, existing.last_played) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };

        if let Some(new_trophies) = fetched_trophies {
            if !new_trophies.is_empty() {
                // Merge unlocked states monotonically against previously stored trophies
                let old_map: HashMap<String, &SavedTrophy> = existing
                    .trophies
                    .iter()
                    .map(|t| (t.id.clone(), t))
                    .collect();

                let merged: Vec<SavedTrophy> = new_trophies
                    .into_iter()
                    .map(|mut t| {
                        if let Some(old) = old_map.get(&t.id) {
                            if old.unlocked && !t.unlocked {
                                t.unlocked = true;
                                t.unlocked_time = old.unlocked_time;
                            } else if t.unlocked && t.unlocked_time.is_none() {
                                t.unlocked_time = old.unlocked_time;
                            }
                        }
                        t
                    })
                    .collect();

                let total = merged.len() as u32;
                let unlocked = merged.iter().filter(|t| t.unlocked).count() as u32;
                existing.total = total;
                existing.unlocked = unlocked;
                existing.pct = if total > 0 { (unlocked * 100) / total } else { 0 };
                existing.platinum = total > 0 && unlocked == total;
                existing.trophies = merged;
            }
        }

        existing.last_scanned = now;
    } else {
        let trophies = fetched_trophies.unwrap_or_default();
        let total = trophies.len() as u32;
        let unlocked = trophies.iter().filter(|t| t.unlocked).count() as u32;
        let pct = if total > 0 { (unlocked * 100) / total } else { 0 };
        let platinum = total > 0 && unlocked == total;

        db.games.insert(
            appid_str.clone(),
            SavedGameEntry {
                id: appid_str.clone(),
                name: incoming_name.unwrap_or_else(|| format!("Steam App {}", appid_str)),
                platform: "Steam".to_string(),
                appid: Some(appid_str),
                unlocked,
                total,
                pct,
                platinum,
                base_playtime_minutes: 0,
                playtime_minutes: incoming_playtime,
                last_played: incoming_last_played,
                source: "steam".to_string(),
                last_scanned: now,
                trophies,
            },
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SteamSyncReport {
    pub games_fetched: usize,
    pub games_with_trophies: usize,
    pub achievements_skipped_or_private: usize,
    pub total_games_in_db: usize,
    pub persona_name: Option<String>,
    pub synced_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SteamSyncOutcome {
    Started {
        cooldown_secs: u64,
    },
    Completed(SteamSyncReport),
    Cooldown {
        cooldown_remaining_secs: u64,
        cooldown_secs: u64,
        last_sync_at: Option<u64>,
    },
    InProgress,
    NotConfigured {
        message: String,
    },
    Failed {
        message: String,
    },
}

struct SyncInProgressGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for SyncInProgressGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

async fn fetch_game_trophies_with_retry(
    client: &SteamClient,
    api_key: &str,
    steam_id: &str,
    appid: u64,
) -> Option<Vec<SavedTrophy>> {
    // Retry once on HTTP 429 rate-limit; treat private/no-stats games as None gracefully.
    let schema = match client.fetch_game_schema(api_key, appid).await {
        Ok(s) => s,
        Err(SteamApiError::RateLimited) => {
            tokio::time::sleep(Duration::from_millis(600)).await;
            client.fetch_game_schema(api_key, appid).await.unwrap_or_default()
        }
        Err(err) => {
            tracing::debug!("Skipping schema for appid {}: {}", appid, err);
            Vec::new()
        }
    };

    let player = match client.fetch_player_achievements(api_key, steam_id, appid).await {
        Ok(p) => p,
        Err(SteamApiError::RateLimited) => {
            tokio::time::sleep(Duration::from_millis(600)).await;
            client
                .fetch_player_achievements(api_key, steam_id, appid)
                .await
                .unwrap_or(None)
        }
        Err(err) => {
            tracing::debug!("Skipping player achievements for appid {}: {}", appid, err);
            None
        }
    };

    match player {
        Some(player_achs) => {
            let trophies = build_trophies_from_steam(&schema, &player_achs);
            if trophies.is_empty() {
                None
            } else {
                Some(trophies)
            }
        }
        None => None,
    }
}

/// Runs a full Steam Web API sync and updates `AppState`, SQLite (`games`, `trophies`, `metadata`),
/// and broadcasts SSE update notifications.
pub async fn run_steam_sync(state: SharedState, force: bool) -> SteamSyncOutcome {
    let (
        api_key,
        steam_id,
        base_url,
        cooldown_secs,
        cooldown_remaining,
        sync_flag,
        sync_lock,
        existing_games_snapshot,
    ) = {
        let guard = state.read().await;
        let now = current_timestamp();
        let remaining = guard.cooldown_remaining_secs(now);
        let key = guard.settings.steam_api_key.clone().unwrap_or_default();
        (
            key,
            guard.settings.steam_id.clone(),
            guard.settings.steam_api_base_url.clone(),
            guard.settings.steam_sync_cooldown_secs,
            remaining,
            Arc::clone(&guard.sync_in_progress),
            Arc::clone(&guard.sync_lock),
            guard.database.games.clone(),
        )
    };

    if api_key.trim().is_empty() {
        return SteamSyncOutcome::NotConfigured {
            message: "STEAM_API_KEY is not configured".to_string(),
        };
    }

    if !force && cooldown_remaining > 0 {
        let last_sync_at = state.read().await.last_steam_sync_at;
        return SteamSyncOutcome::Cooldown {
            cooldown_remaining_secs: cooldown_remaining,
            cooldown_secs,
            last_sync_at,
        };
    }

    // Prevent concurrent sync execution
    if sync_flag
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return SteamSyncOutcome::InProgress;
    }
    let _in_progress_guard = SyncInProgressGuard(sync_flag);
    let _sync_mutex_guard = sync_lock.lock().await;

    // Re-check cooldown inside the lock in case another sync just completed
    if !force {
        let now = current_timestamp();
        let remaining = state.read().await.cooldown_remaining_secs(now);
        if remaining > 0 {
            let last_sync_at = state.read().await.last_steam_sync_at;
            return SteamSyncOutcome::Cooldown {
                cooldown_remaining_secs: remaining,
                cooldown_secs,
                last_sync_at,
            };
        }
    }

    let client = match SteamClient::new(base_url) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            record_sync_failure(&state, &msg).await;
            return SteamSyncOutcome::Failed { message: msg };
        }
    };

    // 1. Fetch player summary (non-fatal if unavailable)
    let player_summary = match client.fetch_player_summary(&api_key, &steam_id).await {
        Ok(summary) => summary,
        Err(err) => {
            tracing::warn!("Failed to fetch Steam player summary for {}: {}", steam_id, err);
            None
        }
    };

    // 2. Fetch owned games + total playtime
    let owned_games = match client.fetch_owned_games(&api_key, &steam_id).await {
        Ok(games) => games,
        Err(err) => {
            let msg = err.to_string();
            tracing::error!("Steam GetOwnedGames failed for {}: {}", steam_id, msg);
            record_sync_failure(&state, &msg).await;
            return SteamSyncOutcome::Failed { message: msg };
        }
    };

    // 3. Decide which games need achievement schema + player unlock fetches
    let mut existing_by_appid: HashMap<String, &SavedGameEntry> = HashMap::new();
    for entry in existing_games_snapshot.values() {
        if let Some(aid) = &entry.appid {
            existing_by_appid.insert(aid.clone(), entry);
        } else {
            existing_by_appid.insert(entry.id.clone(), entry);
        }
    }

    let mut targets_for_achievements: Vec<u64> = Vec::new();
    let mut seen_appids: HashSet<u64> = HashSet::new();
    for owned in &owned_games {
        if !seen_appids.insert(owned.appid) {
            continue;
        }
        if owned.has_community_visible_stats == Some(false) {
            continue;
        }
        let appid_str = owned.appid.to_string();
        let stored_opt = existing_by_appid.get(&appid_str).copied();
        let playtime = owned.playtime_forever.unwrap_or(0);
        let last_played = owned.rtime_last_played.unwrap_or(0);

        let should_fetch = match stored_opt {
            Some(stored) => {
                if is_protected_non_steam_entry(stored) {
                    false
                } else if force {
                    playtime > 0 || stored.unlocked > 0 || stored.total > 0 || last_played > 0
                } else {
                    let missing_manifest =
                        stored.trophies.is_empty() && (playtime > 0 || stored.unlocked > 0 || stored.total > 0);
                    let played_since_scan =
                        last_played > stored.last_scanned || playtime > stored.playtime_minutes;
                    missing_manifest || played_since_scan
                }
            }
            None => playtime > 0 || last_played > 0 || force,
        };

        if should_fetch {
            targets_for_achievements.push(owned.appid);
        }
    }

    // 4. Fetch achievement schemas and player unlocked achievements with bounded concurrency
    let semaphore = Arc::new(Semaphore::new(STEAM_CONCURRENCY_LIMIT));
    let mut handles = Vec::with_capacity(targets_for_achievements.len());
    for appid in targets_for_achievements {
        let sem = Arc::clone(&semaphore);
        let c = client.clone();
        let key = api_key.clone();
        let sid = steam_id.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            let trophies = fetch_game_trophies_with_retry(&c, &key, &sid, appid).await;
            Some((appid, trophies))
        }));
    }

    let mut fetched_trophies_map: HashMap<u64, Vec<SavedTrophy>> = HashMap::new();
    let mut skipped_or_private = 0usize;
    for handle in handles {
        if let Ok(Some((appid, trophies_opt))) = handle.await {
            match trophies_opt {
                Some(trophies) => {
                    fetched_trophies_map.insert(appid, trophies);
                }
                None => {
                    skipped_or_private += 1;
                }
            }
        }
    }

    let now = current_timestamp();
    let games_fetched = owned_games.len();
    let games_with_trophies = fetched_trophies_map.len();
    let persona_name = player_summary
        .as_ref()
        .map(|s| s.personaname.trim().to_string())
        .filter(|s| !s.is_empty());
    let avatar_url = player_summary
        .as_ref()
        .map(|s| {
            if !s.avatarfull.trim().is_empty() {
                s.avatarfull.trim().to_string()
            } else {
                s.avatarmedium.trim().to_string()
            }
        })
        .filter(|s| !s.is_empty());

    // 5. Merge into latest AppState database snapshot and persist atomically to SQLite
    let mut db_to_save = {
        let guard = state.read().await;
        guard.database.clone()
    };

    for owned in &owned_games {
        let trophies = fetched_trophies_map.remove(&owned.appid);
        merge_steam_game_entry(&mut db_to_save, owned, trophies, now);
    }

    db_to_save.preferences.steam_id = Some(steam_id.clone());
    if let Some(ref persona) = persona_name {
        db_to_save.preferences.username = persona.clone();
    }
    db_to_save.last_updated = now;
    let total_games_in_db = db_to_save.games.len();

    {
        let guard = state.read().await;
        if let Err(e) = guard.save_db_atomic(&db_to_save) {
            let msg = format!("Failed to save synced Steam database to SQLite: {}", e);
            tracing::error!("{}", msg);
            drop(guard);
            record_sync_failure(&state, &msg).await;
            return SteamSyncOutcome::Failed { message: msg };
        }
    }

    let summary_msg = format!(
        "Synced {} Steam games ({} trophy manifests, {} private/no-achievements)",
        games_fetched, games_with_trophies, skipped_or_private
    );

    {
        let mut write_guard = state.write().await;
        write_guard.database = db_to_save;
        write_guard.last_modified = std::time::SystemTime::now();
        write_guard.last_steam_sync_at = Some(now);
        write_guard.last_steam_sync_status = Some("ok".to_string());
        write_guard.last_steam_sync_message = Some(summary_msg.clone());
        if let Some(ref p) = persona_name {
            write_guard.steam_persona_name = Some(p.clone());
        }
        if let Some(ref av) = avatar_url {
            write_guard.steam_avatar_url = Some(av.clone());
        }
        if let Err(e) = write_guard.persist_sync_metadata() {
            tracing::warn!("Failed to persist Steam sync metadata: {}", e);
        }
        write_guard.broadcast_update();
    }

    tracing::info!("{}", summary_msg);

    SteamSyncOutcome::Completed(SteamSyncReport {
        games_fetched,
        games_with_trophies,
        achievements_skipped_or_private: skipped_or_private,
        total_games_in_db,
        persona_name,
        synced_at: now,
    })
}

async fn record_sync_failure(state: &SharedState, message: &str) {
    let mut write_guard = state.write().await;
    write_guard.last_steam_sync_status = Some("error".to_string());
    write_guard.last_steam_sync_message = Some(message.to_string());
    let _ = write_guard.persist_sync_metadata();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_trophies_from_steam_merges_schema_and_unlocks() {
        let schema = vec![
            SteamSchemaAchievement {
                name: "ACH_FIRST".to_string(),
                display_name: Some("First Blood".to_string()),
                description: Some("Defeat your first foe.".to_string()),
                hidden: Some(0),
            },
            SteamSchemaAchievement {
                name: "ACH_SECRET".to_string(),
                display_name: Some("Hidden Master".to_string()),
                description: None,
                hidden: Some(1),
            },
        ];
        let player = vec![
            SteamPlayerAchievement {
                apiname: "ACH_FIRST".to_string(),
                achieved: 1,
                unlocktime: 1732000123,
                name: None,
                description: None,
            },
            SteamPlayerAchievement {
                apiname: "ACH_SECRET".to_string(),
                achieved: 0,
                unlocktime: 0,
                name: None,
                description: None,
            },
        ];

        let trophies = build_trophies_from_steam(&schema, &player);
        assert_eq!(trophies.len(), 2);

        assert_eq!(trophies[0].id, "ACH_FIRST");
        assert_eq!(trophies[0].name, "First Blood");
        assert_eq!(trophies[0].description, "Defeat your first foe.");
        assert_eq!(trophies[0].grade, "Bronze");
        assert!(trophies[0].unlocked);
        assert_eq!(trophies[0].unlocked_time, Some(1732000123));

        assert_eq!(trophies[1].id, "ACH_SECRET");
        assert_eq!(trophies[1].description, "Hidden achievement");
        assert_eq!(trophies[1].grade, "Silver");
        assert!(!trophies[1].unlocked);
        assert_eq!(trophies[1].unlocked_time, None);
    }

    #[test]
    fn test_merge_steam_game_preserves_monotonic_playtime_and_bloodborne() {
        let mut db = LibraryDatabase {
            version: 1,
            last_updated: 1000,
            preferences: Default::default(),
            manual_games: Vec::new(),
            games: HashMap::new(),
        };

        // Pre-existing Bloodborne PC port entry
        db.games.insert(
            "bloodborne".to_string(),
            SavedGameEntry {
                id: "bloodborne".to_string(),
                name: "Bloodborne (PC Recomp)".to_string(),
                platform: "PC Port / Recomp".to_string(),
                appid: Some("2485153835".to_string()),
                unlocked: 40,
                total: 40,
                pct: 100,
                platinum: true,
                base_playtime_minutes: 5400,
                playtime_minutes: 5400,
                last_played: Some(1730000000),
                source: "manual".to_string(),
                last_scanned: 1730000000,
                trophies: Vec::new(),
            },
        );

        // Pre-existing Steam game with higher historical playtime (e.g. 900 mins)
        db.games.insert(
            "1245620".to_string(),
            SavedGameEntry {
                id: "1245620".to_string(),
                name: "ELDEN RING".to_string(),
                platform: "Steam".to_string(),
                appid: Some("1245620".to_string()),
                unlocked: 10,
                total: 42,
                pct: 23,
                platinum: false,
                base_playtime_minutes: 0,
                playtime_minutes: 900,
                last_played: Some(1731000000),
                source: "steam".to_string(),
                last_scanned: 1731000000,
                trophies: vec![SavedTrophy {
                    id: "ACH_OLD".to_string(),
                    name: "Old Trophy".to_string(),
                    description: "Already unlocked".to_string(),
                    grade: "Bronze".to_string(),
                    unlocked: true,
                    unlocked_time: Some(1730500000),
                }],
            },
        );

        // Incoming Steam data has LOWER playtime (300 mins) and empty trophies (private/skipped)
        let incoming = SteamOwnedGame {
            appid: 1245620,
            name: Some("ELDEN RING".to_string()),
            playtime_forever: Some(300),
            rtime_last_played: Some(1732000000),
            has_community_visible_stats: Some(true),
            img_icon_url: None,
        };

        merge_steam_game_entry(&mut db, &incoming, None, 1732500000);

        let er = db.games.get("1245620").unwrap();
        // Playtime must stay at MAX(900, 300) = 900
        assert_eq!(er.playtime_minutes, 900);
        // Existing trophies/unlocked counts must be preserved when achievement fetch returned None
        assert_eq!(er.unlocked, 10);
        assert_eq!(er.total, 42);
        assert_eq!(er.trophies.len(), 1);
        assert_eq!(er.last_played, Some(1732000000));

        // Bloodborne must be completely untouched
        let bb = db.games.get("bloodborne").unwrap();
        assert_eq!(bb.playtime_minutes, 5400);
        assert_eq!(bb.source, "manual");
    }
}
