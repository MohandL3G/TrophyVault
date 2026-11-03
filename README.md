# 🎮 Achievements Passport Generator

A luxury gaming achievements passport generator desktop application built in **Rust** (using `egui` and `wgpu`).

Inspired by the Palworld Passport HUD, it aggregates your Steam library, LuaTools games, and PC Ports/Decomps (like **Bloodborne**) into a shareable passport card featuring an overall rating badge (**OVR**), progress telemetry, playtime metrics, and library milestones.

---

## Key Features

- **Direct Steam & LuaTools Ingestion**: Reads Steam `localconfig.vdf` for playtime (handling signed/unsigned 32-bit AppIDs), binary `shortcuts.vdf` for Non-Steam games, and discovers games added via [LuaTools](https://github.com/madoiscool/LuaTools).
- **Custom Ports & Decomps Support**: Native support for **Bloodborne PC port** (`bbport`/`ShadPS4`) reading unlocked trophies from `user\RcloneUI\trophies.txt` and `metadata.json`.
- **Persistent JSON Database**: Keeps all your game records in `%APPDATA%\AchievementsPassport\library_database.json`. Even if you uninstall a game or remove its `.lua` manifest, your achievements and playtime are permanently preserved.
- **Two Passport Modes**:
  - **Overall Library**: An all-in-one passport summarizing total completions, 100% platinums, active games, and overall rating.
  - **Single Game**: Generate a dedicated card for any specific game with completion percentage, clear status, and platform origin.
- **5 Sci-Fi HUD Themes**: Cyberpunk Cyan, Trophy Gold, Bloodborne Crimson, Emerald Matrix, and Neon Violet.
- **Instant GPU Preview & Export**: Export crisp 1200 × 850 PNG cards or scalable vector SVG files with no watermarks or placeholder mottos.

---

## Automated Builds

Pre-built Windows binaries (`achievements-passport-windows.zip`) are automatically built on every commit via GitHub Actions. Download the latest build from the **Actions** tab or the **Releases** page.
