//! Embedded SVG icons and vector shapes for the passport graphics.

pub const TROPHY_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6"/>
  <path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18"/>
  <path d="M4 22h16"/>
  <path d="M10 14.66V17c0 .55-.45 1-1 1H8c-.55 0-1 .45-1 1v1c0 .55.45 1 1 1h8c.55 0 1-.45 1-1v-1c0-.55-.45-1-1-1h-1c-.55 0-1-.45-1-1v-2.34"/>
  <path d="M18 4H6v6c0 3.31 2.69 6 6 6s6-2.69 6-6V4z"/>
</svg>
"#;

pub const COMPASS_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="10"/>
  <polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"/>
</svg>
"#;

pub const MEDAL_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="8" r="6"/>
  <path d="M15.477 12.89 17 22l-5-3-5 3 1.523-9.11"/>
</svg>
"#;

pub const GAMEPAD_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <line x1="6" y1="12" x2="10" y2="12"/>
  <line x1="8" y1="10" x2="8" y2="14"/>
  <line x1="15" y1="13" x2="15.01" y2="13"/>
  <line x1="18" y1="11" x2="18.01" y2="11"/>
  <rect x="2" y="6" width="20" height="12" rx="6"/>
</svg>
"#;

pub const STEAM_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M12 2a10 10 0 0 1 10 10 10 10 0 0 1-10 10c-4.4 0-8.15-2.85-9.48-6.84l4.24-1.74a3.5 3.5 0 0 0 4.24-2.42 3.5 3.5 0 0 0-2.42-4.24 3.5 3.5 0 0 0-4.24 2.42l-3.8-1.56A10 10 0 0 1 12 2z"/>
  <circle cx="14.5" cy="9.5" r="1.5" fill="currentColor"/>
</svg>
"#;

pub const EMULATOR_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <rect x="4" y="4" width="16" height="16" rx="2"/>
  <circle cx="8.5" cy="8.5" r="1.5" fill="currentColor"/>
  <polyline points="21 15 16 10 5 21"/>
</svg>
"#;

pub const CHECKLIST_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
  <rect x="8" y="2" width="8" height="4" rx="1"/>
  <path d="m9 14 2 2 4-4"/>
</svg>
"#;

pub const CLOCK_ICON: &str = r#"
<svg viewBox="0 0 24 24" width="48" height="48" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="10"/>
  <polyline points="12 6 12 12 16 14"/>
</svg>
"#;

pub const STAR_SPARKLE: &str = r#"
<svg viewBox="0 0 24 24" width="36" height="36" fill="currentColor">
  <path d="M12 0L14.5 9.5L24 12L14.5 14.5L12 24L9.5 14.5L0 12L9.5 9.5L12 0Z"/>
</svg>
"#;
