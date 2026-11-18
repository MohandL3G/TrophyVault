//! HUD 400x100 compact badge generator for GitHub profile headers.

use trophyvault_core::data::AchievementsData;
use trophyvault_core::generator::{escape_xml, PassportTheme};

/// Generates a sleek, compact 400x100 HUD badge SVG for GitHub profile headers.
pub fn generate_badge_svg(data: &AchievementsData, theme: PassportTheme) -> String {
    let primary = theme.primary_color();
    let secondary = theme.secondary_color();
    let border = theme.panel_border();

    let raw_username = if data.username.trim().is_empty() {
        "MOHANDL3G"
    } else {
        data.username.trim()
    };
    let username = escape_xml(raw_username);

    let stats = &data.stats;
    let ovr = stats.ovr_score;
    let total_games = data.games.len();
    let perfect_games = stats.perfect_games;
    let completion_rate = format!("{:.0}%", stats.completion_rate);

    let total_hours = stats.total_playtime_minutes / 60;
    let playtime_str = if total_hours >= 1000 {
        format!("{}h", total_hours)
    } else if total_hours > 0 {
        format!("{}h", total_hours)
    } else {
        format!("{}m", stats.total_playtime_minutes)
    };

    let unlocked = stats.unlocked_achievements;
    let total = stats.total_achievements;

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 100" width="400" height="100">
  <defs>
    <linearGradient id="tv-badge-bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#040c16"/>
      <stop offset="60%" stop-color="#071829"/>
      <stop offset="100%" stop-color="#03080f"/>
    </linearGradient>
    <radialGradient id="tv-badge-glow" cx="15%" cy="30%" r="70%">
      <stop offset="0%" stop-color="{secondary}" stop-opacity="0.7"/>
      <stop offset="100%" stop-color="#000000" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <!-- Background Base -->
  <rect width="400" height="100" rx="8" fill="url(#tv-badge-bg)"/>
  <rect width="400" height="100" rx="8" fill="url(#tv-badge-glow)"/>

  <!-- Subtle Cyberpunk Grid Lines -->
  <g opacity="0.08" stroke="{primary}" stroke-width="1">
    <line x1="0" y1="25" x2="400" y2="25"/>
    <line x1="0" y1="50" x2="400" y2="50"/>
    <line x1="0" y1="75" x2="400" y2="75"/>
    <line x1="100" y1="0" x2="100" y2="100"/>
    <line x1="175" y1="0" x2="175" y2="100"/>
    <line x1="250" y1="0" x2="250" y2="100"/>
    <line x1="325" y1="0" x2="325" y2="100"/>
  </g>

  <!-- Outer HUD Frame with Chamfered Corners -->
  <polygon points="12,3 388,3 397,12 397,88 388,97 12,97 3,88 3,12"
           fill="none" stroke="{primary}" stroke-width="1.8" opacity="0.9"/>
  <polygon points="15,6 385,6 394,15 394,85 385,94 15,94 6,85 6,15"
           fill="none" stroke="{border}" stroke-width="1" opacity="0.5"/>

  <!-- Corner Notch Accent Marks -->
  <path d="M3,20 L3,12 L12,3 L20,3" fill="none" stroke="{primary}" stroke-width="3"/>
  <path d="M380,3 L388,3 L397,12 L397,20" fill="none" stroke="{primary}" stroke-width="3"/>
  <path d="M3,80 L3,88 L12,97 L20,97" fill="none" stroke="{primary}" stroke-width="3"/>
  <path d="M380,97 L388,97 L397,88 L397,80" fill="none" stroke="{primary}" stroke-width="3"/>

  <!-- LEFT SECTION: User & Brand -->
  <g transform="translate(18, 14)">
    <!-- Brand Tag -->
    <text x="0" y="8" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="8" font-weight="800" fill="{primary}" letter-spacing="1.5">TROPHYVAULT // HUD</text>
    
    <!-- Persona Name -->
    <text x="0" y="30" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="15" font-weight="900" fill="#f0f6fc" letter-spacing="0.5">{username}</text>

    <!-- OVR Hex / Diamond Badge -->
    <g transform="translate(0, 38)">
      <polygon points="0,5 5,0 45,0 50,5 50,22 45,27 5,27 0,22"
               fill="#061c2e" stroke="{primary}" stroke-width="1.5"/>
      <text x="25" y="14" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="12" font-weight="900" fill="{primary}">{ovr}</text>
      <text x="25" y="23" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="6.5" font-weight="800" fill="{primary}" letter-spacing="1">OVR</text>
    </g>

    <!-- Trophies count under name -->
    <text x="56" y="58" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="7.5" font-weight="600" fill="#8da4be">
      <tspan fill="{primary}">{unlocked}</tspan> / {total} TROPHIES
    </text>
  </g>

  <!-- Divider Line -->
  <line x1="126" y1="16" x2="126" y2="84" stroke="{border}" stroke-width="1.5" stroke-dasharray="3,3" opacity="0.6"/>

  <!-- RIGHT SECTION: 4 KPI Columns -->
  <!-- Col 1: Games -->
  <g transform="translate(160, 24)">
    <text x="0" y="12" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="8" font-weight="700" fill="#8da4be" letter-spacing="1">GAMES</text>
    <text x="0" y="36" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="19" font-weight="900" fill="#f0f6fc">{total_games}</text>
    <text x="0" y="52" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="7" font-weight="600" fill="#4f657a">TRACKED</text>
  </g>

  <!-- Col 2: 100% Completions -->
  <g transform="translate(228, 24)">
    <text x="0" y="12" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="8" font-weight="700" fill="{primary}" letter-spacing="1">PERFECT</text>
    <text x="0" y="36" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="19" font-weight="900" fill="{primary}">{perfect_games}</text>
    <text x="0" y="52" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="7" font-weight="600" fill="#4f657a">100% CLUB</text>
  </g>

  <!-- Col 3: Playtime -->
  <g transform="translate(296, 24)">
    <text x="0" y="12" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="8" font-weight="700" fill="#8da4be" letter-spacing="1">PLAYTIME</text>
    <text x="0" y="36" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="17" font-weight="900" fill="#f0f6fc">{playtime_str}</text>
    <text x="0" y="52" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="7" font-weight="600" fill="#4f657a">TOTAL</text>
  </g>

  <!-- Col 4: Completion Rate -->
  <g transform="translate(362, 24)">
    <text x="0" y="12" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="8" font-weight="700" fill="#8da4be" letter-spacing="1">RATE</text>
    <text x="0" y="36" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="17" font-weight="900" fill="#f0f6fc">{completion_rate}</text>
    <text x="0" y="52" text-anchor="middle" font-family="system-ui, -apple-system, Segoe UI, Roboto, sans-serif" font-size="7" font-weight="600" fill="#4f657a">GLOBAL</text>
  </g>
</svg>"##,
        primary = primary,
        secondary = secondary,
        border = border,
        username = username,
        ovr = ovr,
        total_games = total_games,
        perfect_games = perfect_games,
        playtime_str = playtime_str,
        completion_rate = completion_rate,
        unlocked = unlocked,
        total = total,
    )
}
