//! Visual passport generator that builds a high-resolution SVG and renders it to PNG.

use crate::data::{AchievementsData, GameItem};
use crate::icons::*;
use resvg::tiny_skia;
use std::sync::{Arc, OnceLock};
use usvg::fontdb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassportTheme {
    CyberpunkCyan,
    TrophyGold,
    BloodborneCrimson,
    EmeraldMatrix,
    SteamPurple,
}

impl PassportTheme {
    pub fn all() -> [PassportTheme; 5] {
        [
            PassportTheme::CyberpunkCyan,
            PassportTheme::TrophyGold,
            PassportTheme::BloodborneCrimson,
            PassportTheme::EmeraldMatrix,
            PassportTheme::SteamPurple,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PassportTheme::CyberpunkCyan => "Cyberpunk Cyan (Palworld Style)",
            PassportTheme::TrophyGold => "Trophy Gold",
            PassportTheme::BloodborneCrimson => "Bloodborne Crimson",
            PassportTheme::EmeraldMatrix => "Emerald Matrix",
            PassportTheme::SteamPurple => "Neon Violet",
        }
    }

    pub fn primary_color(&self) -> &'static str {
        match self {
            PassportTheme::CyberpunkCyan => "#00f0ff",
            PassportTheme::TrophyGold => "#ffd700",
            PassportTheme::BloodborneCrimson => "#ff3366",
            PassportTheme::EmeraldMatrix => "#00ff88",
            PassportTheme::SteamPurple => "#bb86fc",
        }
    }

    pub fn secondary_color(&self) -> &'static str {
        match self {
            PassportTheme::CyberpunkCyan => "#0c3b5e",
            PassportTheme::TrophyGold => "#5e4b0c",
            PassportTheme::BloodborneCrimson => "#5e0c24",
            PassportTheme::EmeraldMatrix => "#0c5e32",
            PassportTheme::SteamPurple => "#3a0c5e",
        }
    }

    pub fn panel_border(&self) -> &'static str {
        match self {
            PassportTheme::CyberpunkCyan => "#134267",
            PassportTheme::TrophyGold => "#675313",
            PassportTheme::BloodborneCrimson => "#671329",
            PassportTheme::EmeraldMatrix => "#13673b",
            PassportTheme::SteamPurple => "#481367",
        }
    }
}

pub struct PassportOptions<'a> {
    pub username: &'a str,
    pub title: &'a str,
    pub theme: PassportTheme,
    pub selected_game: Option<&'a GameItem>,
}

/// Generates the SVG document for the achievements passport.
pub fn generate_passport_svg(data: &AchievementsData, options: &PassportOptions) -> String {
    let primary = options.theme.primary_color();
    let panel_border = options.theme.panel_border();
    let secondary = options.theme.secondary_color();

    let username = if options.username.trim().is_empty() {
        "MOHANDL3G"
    } else {
        options.username.trim()
    };

    if let Some(game) = options.selected_game {
        // --- SINGLE GAME PASSPORT (e.g. Bloodborne, Palworld, Black Myth Wukong) ---
        let passport_category = format!("{} PASSPORT", game.name.to_uppercase());
        let ovr = game.pct;
        let locked = game.total.saturating_sub(game.unlocked);
        let status = if game.platinum || (game.total > 0 && game.unlocked == game.total) {
            "100% PERFECT"
        } else if game.unlocked > 0 {
            "IN PROGRESS"
        } else {
            "NOT STARTED"
        };

        let title_line = if options.title.trim().is_empty() {
            format!("Status: {}   •   Completion: {}%", status, ovr)
        } else {
            options.title.trim().to_string()
        };

        let appid_str = game.appid.as_deref().unwrap_or("N/A");
        let playtime_str = if game.playtime_minutes > 0 {
            format!("{}h {}m", game.playtime_minutes / 60, game.playtime_minutes % 60)
        } else {
            "0h 0m".to_string()
        };

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 850" width="1200" height="850">
  <defs>
    <linearGradient id="bg-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#040c16"/>
      <stop offset="50%" stop-color="#071829"/>
      <stop offset="100%" stop-color="#03080f"/>
    </linearGradient>
    <radialGradient id="hud-glow" cx="20%" cy="20%" r="60%">
      <stop offset="0%" stop-color="{secondary}" stop-opacity="0.6"/>
      <stop offset="100%" stop-color="#000000" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <!-- Background Base -->
  <rect width="1200" height="850" fill="url(#bg-grad)"/>
  <rect width="1200" height="850" fill="url(#hud-glow)"/>

  <!-- Subtle Cyberpunk Grid Lines -->
  <g opacity="0.07" stroke="{primary}" stroke-width="1">
    <line x1="0" y1="200" x2="1200" y2="200"/>
    <line x1="0" y1="400" x2="1200" y2="400"/>
    <line x1="0" y1="600" x2="1200" y2="600"/>
    <line x1="300" y1="0" x2="300" y2="850"/>
    <line x1="600" y1="0" x2="600" y2="850"/>
    <line x1="900" y1="0" x2="900" y2="850"/>
  </g>

  <!-- Outer HUD Frame with 45-degree Chamfered Corners -->
  <polygon points="40,15 1160,15 1185,40 1185,810 1160,835 40,835 15,810 15,40"
           fill="none" stroke="{primary}" stroke-width="3" opacity="0.9"/>
  <polygon points="45,22 1155,22 1178,45 1178,805 1155,828 45,828 22,805 22,45"
           fill="none" stroke="{panel_border}" stroke-width="1" opacity="0.6"/>

  <!-- Corner Notch Accent Marks -->
  <path d="M15,60 L15,40 L40,15 L60,15" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M1140,15 L1160,15 L1185,40 L1185,60" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M15,790 L15,810 L40,835 L60,835" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M1140,835 L1160,835 L1185,810 L1185,790" fill="none" stroke="{primary}" stroke-width="5"/>

  <!-- HEADER -->
  <g transform="translate(45, 50)">
    <rect x="0" y="5" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="800" fill="{primary}" letter-spacing="3">{passport_category}</text>

    <!-- Player Name -->
    <text x="0" y="100" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="64" font-weight="900" fill="#f0f6fc" letter-spacing="2">{username}</text>

    <!-- Subtitle Meta -->
    <text x="0" y="140" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="700" fill="#8da4be">
      {title_line}   <tspan fill="{primary}">•</tspan>   Platform: <tspan fill="#f0f6fc">{platform}</tspan>
    </text>
  </g>

  <!-- OVR BADGE (Top Right Hex-HUD) -->
  <g transform="translate(950, 42)">
    <polygon points="0,20 20,0 180,0 200,20 200,140 180,160 20,160 0,140"
             fill="#061c2e" stroke="{primary}" stroke-width="3"/>
    <polygon points="6,24 24,6 176,6 194,24 194,136 176,154 24,154 6,136"
             fill="none" stroke="{secondary}" stroke-width="1.5" opacity="0.8"/>
    <text x="100" y="98" text-anchor="middle" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="74" font-weight="900" fill="{primary}">{ovr}</text>
    <text x="100" y="136" text-anchor="middle" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="18" font-weight="800" fill="{primary}" letter-spacing="3">OVR</text>
  </g>

  <!-- SECTION 1: PROGRESS -->
  <g transform="translate(45, 260)">
    <rect x="0" y="0" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="19" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="800" fill="{primary}" letter-spacing="2">PROGRESS</text>

    <!-- Panel Box -->
    <rect x="0" y="32" width="1110" height="280" rx="10" fill="#061929" stroke="{panel_border}" stroke-width="2"/>

    <!-- Grid Dividing Lines -->
    <line x1="370" y1="42" x2="370" y2="302" stroke="{panel_border}" stroke-width="1.5" stroke-dasharray="6,4"/>
    <line x1="740" y1="42" x2="740" y2="302" stroke="{panel_border}" stroke-width="1.5" stroke-dasharray="6,4"/>
    <line x1="15" y1="172" x2="1095" y2="172" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Item 1: Unlocked Achievements -->
    <g transform="translate(30, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{MEDAL_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Achievements</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{unlocked} <tspan font-size="20" fill="#6a829e">/ {total}</tspan></text>
    </g>

    <!-- Item 2: Completion Rate -->
    <g transform="translate(400, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{COMPASS_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Completion Rate</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="{primary}">{ovr}%</text>
    </g>

    <!-- Item 3: Status -->
    <g transform="translate(770, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{TROPHY_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Clear Status</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="26" font-weight="900" fill="#f0f6fc">{status}</text>
    </g>

    <!-- Item 4: Locked Remaining -->
    <g transform="translate(30, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{CHECKLIST_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Locked Remaining</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{locked} <tspan font-size="20" fill="#6a829e">left</tspan></text>
    </g>

    <!-- Item 5: Platform Source -->
    <g transform="translate(400, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{GAMEPAD_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Platform Source</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="28" font-weight="900" fill="#f0f6fc">{platform}</text>
    </g>

    <!-- Item 6: App ID -->
    <g transform="translate(770, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{STEAM_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Game Identifier / ID</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="30" font-weight="900" fill="{primary}">{appid_str}</text>
    </g>
  </g>

  <!-- SECTION 2: SUMMARY -->
  <g transform="translate(45, 600)">
    <rect x="0" y="0" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="19" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="800" fill="{primary}" letter-spacing="2">GAME SUMMARY</text>

    <!-- Panel Box -->
    <rect x="0" y="32" width="1110" height="135" rx="10" fill="#061929" stroke="{panel_border}" stroke-width="2"/>

    <!-- Stat 1 -->
    <g transform="translate(60, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="#f0f6fc">{unlocked}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Earned Unlocks</text>
    </g>

    <line x1="260" y1="52" x2="260" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 2 -->
    <g transform="translate(320, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="#f0f6fc">{total}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Total In Schema</text>
    </g>

    <line x1="530" y1="52" x2="530" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 3 -->
    <g transform="translate(590, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="{primary}">{ovr}%</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Completion Rate</text>
    </g>

    <line x1="800" y1="52" x2="800" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 4 -->
    <g transform="translate(860, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="32" font-weight="900" fill="{primary}">{playtime_str}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Cumulative Playtime</text>
    </g>
  </g>

  <!-- CLEAN FOOTER -->
  <g transform="translate(55, 805)">
    <text x="0" y="0" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="900" fill="{primary}" letter-spacing="2">TROPHYVAULT</text>
    <text x="1090" y="0" text-anchor="end" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="13" font-weight="600" letter-spacing="1" fill="#6a829e">github.com/MohandL3G/TrophyVault</text>
  </g>
</svg>"##,
            primary = primary,
            secondary = secondary,
            panel_border = panel_border,
            passport_category = passport_category,
            username = username,
            title_line = title_line,
            platform = game.platform,
            ovr = ovr,
            unlocked = game.unlocked,
            total = game.total,
            locked = locked,
            status = status,
            playtime_str = playtime_str,
            appid_str = appid_str,
            MEDAL_ICON = MEDAL_ICON,
            COMPASS_ICON = COMPASS_ICON,
            TROPHY_ICON = TROPHY_ICON,
            CHECKLIST_ICON = CHECKLIST_ICON,
            GAMEPAD_ICON = GAMEPAD_ICON,
            STEAM_ICON = STEAM_ICON
        )
    } else {
        // --- OVERALL LIBRARY PASSPORT ---
        let s = &data.stats;
        let ovr = s.ovr_score;
        let completion_rate_str = format!("{:.1}%", s.completion_rate);

        let title_line = if options.title.trim().is_empty() {
            "Hunter of Yharnam   •   Trophy Hunter"
        } else {
            options.title.trim()
        };

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 850" width="1200" height="850">
  <defs>
    <linearGradient id="bg-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#040c16"/>
      <stop offset="50%" stop-color="#071829"/>
      <stop offset="100%" stop-color="#03080f"/>
    </linearGradient>
    <radialGradient id="hud-glow" cx="20%" cy="20%" r="60%">
      <stop offset="0%" stop-color="{secondary}" stop-opacity="0.6"/>
      <stop offset="100%" stop-color="#000000" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <!-- Background Base -->
  <rect width="1200" height="850" fill="url(#bg-grad)"/>
  <rect width="1200" height="850" fill="url(#hud-glow)"/>

  <!-- Subtle Cyberpunk Grid Lines -->
  <g opacity="0.07" stroke="{primary}" stroke-width="1">
    <line x1="0" y1="200" x2="1200" y2="200"/>
    <line x1="0" y1="400" x2="1200" y2="400"/>
    <line x1="0" y1="600" x2="1200" y2="600"/>
    <line x1="300" y1="0" x2="300" y2="850"/>
    <line x1="600" y1="0" x2="600" y2="850"/>
    <line x1="900" y1="0" x2="900" y2="850"/>
  </g>

  <!-- Outer HUD Frame with 45-degree Chamfered Corners -->
  <polygon points="40,15 1160,15 1185,40 1185,810 1160,835 40,835 15,810 15,40"
           fill="none" stroke="{primary}" stroke-width="3" opacity="0.9"/>
  <polygon points="45,22 1155,22 1178,45 1178,805 1155,828 45,828 22,805 22,45"
           fill="none" stroke="{panel_border}" stroke-width="1" opacity="0.6"/>

  <!-- Corner Notch Accent Marks -->
  <path d="M15,60 L15,40 L40,15 L60,15" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M1140,15 L1160,15 L1185,40 L1185,60" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M15,790 L15,810 L40,835 L60,835" fill="none" stroke="{primary}" stroke-width="5"/>
  <path d="M1140,835 L1160,835 L1185,810 L1185,790" fill="none" stroke="{primary}" stroke-width="5"/>

  <!-- HEADER -->
  <g transform="translate(45, 50)">
    <rect x="0" y="5" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="800" fill="{primary}" letter-spacing="3">GAMING ACHIEVEMENTS PASSPORT</text>

    <!-- Player Name -->
    <text x="0" y="100" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="64" font-weight="900" fill="#f0f6fc" letter-spacing="2">{username}</text>

    <!-- Subtitle Meta -->
    <text x="0" y="140" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="700" fill="#8da4be">
      {title_line}   <tspan fill="{primary}">•</tspan>   Total Games: <tspan fill="#f0f6fc">{total_games}</tspan>   <tspan fill="{primary}">•</tspan>   Unlocked: <tspan fill="#f0f6fc">{unlocked_ach}</tspan>
    </text>
  </g>

  <!-- OVR BADGE (Top Right Hex-HUD) -->
  <g transform="translate(950, 42)">
    <polygon points="0,20 20,0 180,0 200,20 200,140 180,160 20,160 0,140"
             fill="#061c2e" stroke="{primary}" stroke-width="3"/>
    <polygon points="6,24 24,6 176,6 194,24 194,136 176,154 24,154 6,136"
             fill="none" stroke="{secondary}" stroke-width="1.5" opacity="0.8"/>
    <text x="100" y="98" text-anchor="middle" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="74" font-weight="900" fill="{primary}">{ovr}</text>
    <text x="100" y="136" text-anchor="middle" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="18" font-weight="800" fill="{primary}" letter-spacing="3">OVR</text>
  </g>

  <!-- SECTION 1: PROGRESS -->
  <g transform="translate(45, 260)">
    <rect x="0" y="0" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="19" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="800" fill="{primary}" letter-spacing="2">PROGRESS</text>

    <!-- Panel Box -->
    <rect x="0" y="32" width="1110" height="280" rx="10" fill="#061929" stroke="{panel_border}" stroke-width="2"/>

    <!-- Grid Dividing Lines -->
    <line x1="370" y1="42" x2="370" y2="302" stroke="{panel_border}" stroke-width="1.5" stroke-dasharray="6,4"/>
    <line x1="740" y1="42" x2="740" y2="302" stroke="{panel_border}" stroke-width="1.5" stroke-dasharray="6,4"/>
    <line x1="15" y1="172" x2="1095" y2="172" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Item 1: Perfect Games -->
    <g transform="translate(30, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{TROPHY_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Perfect Games (100%)</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{perfect_games} <tspan font-size="20" fill="#6a829e">/ {total_games}</tspan></text>
    </g>

    <!-- Item 2: Completion Rate -->
    <g transform="translate(400, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{COMPASS_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Completion Rate</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="{primary}">{completion_rate}</text>
    </g>

    <!-- Item 3: Total Achievements -->
    <g transform="translate(770, 65)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{MEDAL_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Unlocked Achievements</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{unlocked_ach} <tspan font-size="20" fill="#6a829e">/ {total_ach}</tspan></text>
    </g>

    <!-- Item 4: In Progress Games -->
    <g transform="translate(30, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{GAMEPAD_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Games in Progress</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{in_progress_games} <tspan font-size="20" fill="#6a829e">active</tspan></text>
    </g>

    <!-- Item 5: Steam Achievements -->
    <g transform="translate(400, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{STEAM_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Steam Achievements</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{steam_unlocked} <tspan font-size="20" fill="#6a829e">/ {steam_total}</tspan></text>
    </g>

    <!-- Item 6: PC Ports & Decomps -->
    <g transform="translate(770, 195)">
      <g transform="translate(0, 10)" stroke="{primary}" fill="none">{EMULATOR_ICON}</g>
      <text x="65" y="24" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">PC Ports &amp; Decomps</text>
      <text x="65" y="65" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#f0f6fc">{emu_unlocked} <tspan font-size="20" fill="#6a829e">/ {emu_total}</tspan></text>
    </g>
  </g>

  <!-- SECTION 2: LIBRARY OVERVIEW / STATS -->
  <g transform="translate(45, 600)">
    <rect x="0" y="0" width="5" height="24" fill="{primary}" rx="2"/>
    <text x="18" y="19" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="20" font-weight="800" fill="{primary}" letter-spacing="2">LIBRARY OVERVIEW</text>

    <!-- Panel Box -->
    <rect x="0" y="32" width="1110" height="135" rx="10" fill="#061929" stroke="{panel_border}" stroke-width="2"/>

    <!-- Stat 1: Total Games -->
    <g transform="translate(45, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="#f0f6fc">{total_games}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Total Games</text>
    </g>

    <line x1="220" y1="52" x2="220" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 2: Perfect 100% -->
    <g transform="translate(265, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="{primary}">{perfect_games}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Perfect Games</text>
    </g>

    <line x1="440" y1="52" x2="440" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 3: Unlocked Ach -->
    <g transform="translate(485, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="#f0f6fc">{unlocked_ach}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Unlocked Ach.</text>
    </g>

    <line x1="680" y1="52" x2="680" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 4: In Progress & Sparkle -->
    <g transform="translate(730, 60)">
      <g transform="translate(-5, 0)" fill="{primary}">{STAR_SPARKLE}</g>
      <text x="45" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="38" font-weight="900" fill="{primary}">{in_progress_games}</text>
      <text x="45" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">In Progress</text>
    </g>

    <line x1="910" y1="52" x2="910" y2="145" stroke="{panel_border}" stroke-width="1.5"/>

    <!-- Stat 5: Locked -->
    <g transform="translate(945, 60)">
      <text x="0" y="35" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="34" font-weight="900" fill="#8da4be">{locked_ach}</text>
      <text x="0" y="68" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="600" fill="#8da4be">Locked Remaining</text>
    </g>
  </g>

  <!-- CLEAN FOOTER -->
  <g transform="translate(55, 805)">
    <text x="0" y="0" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="16" font-weight="900" fill="{primary}" letter-spacing="2">TROPHYVAULT</text>
    <text x="1090" y="0" text-anchor="end" font-family="system-ui, Segoe UI, Roboto, sans-serif" font-size="13" font-weight="600" letter-spacing="1" fill="#6a829e">github.com/MohandL3G/TrophyVault</text>
  </g>
</svg>"##,
            primary = primary,
            secondary = secondary,
            panel_border = panel_border,
            username = username,
            title_line = title_line,
            total_games = s.total_configs,
            unlocked_ach = format_number(s.unlocked_achievements),
            total_ach = format_number(s.total_achievements),
            ovr = ovr,
            perfect_games = s.perfect_games,
            completion_rate = completion_rate_str,
            in_progress_games = s.in_progress_games,
            steam_unlocked = format_number(s.steam_official_unlocked),
            steam_total = format_number(s.steam_official_total),
            emu_unlocked = format_number(s.emulator_unlocked),
            emu_total = format_number(s.emulator_total),
            locked_ach = format_number(s.locked_achievements),
            TROPHY_ICON = TROPHY_ICON,
            COMPASS_ICON = COMPASS_ICON,
            MEDAL_ICON = MEDAL_ICON,
            GAMEPAD_ICON = GAMEPAD_ICON,
            STEAM_ICON = STEAM_ICON,
            EMULATOR_ICON = EMULATOR_ICON,
            STAR_SPARKLE = STAR_SPARKLE
        )
    }
}

fn format_number(val: u32) -> String {
    let s = val.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::new();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

static FONT_DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();

/// Cached system font database to avoid rescanning disk fonts on every render.
pub fn get_font_db() -> Arc<fontdb::Database> {
    FONT_DB
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// Renders an SVG string into a raw tiny_skia Pixmap buffer using resvg.
pub fn render_svg_to_pixmap(svg_str: &str, width: u32, height: u32) -> Result<tiny_skia::Pixmap, String> {
    let mut opt = usvg::Options::default();
    opt.font_family = "sans-serif".to_string();
    opt.fontdb = get_font_db();

    let tree = usvg::Tree::from_str(svg_str, &opt)
        .map_err(|e| format!("Failed to parse SVG: {}", e))?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "Failed to allocate pixel map buffer".to_string())?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    Ok(pixmap)
}

/// Renders an SVG string into high-resolution PNG bytes using resvg.
pub fn render_svg_to_png(svg_str: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let pixmap = render_svg_to_pixmap(svg_str, width, height)?;
    pixmap
        .encode_png()
        .map_err(|e| format!("Failed to encode PNG: {}", e))
}
