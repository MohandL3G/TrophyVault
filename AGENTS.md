# Agent Working Guide: Achievements Passport Desktop

This repository contains a desktop application written in **Rust** that generates futuristic, sci-fi HUD achievement passport cards (inspired by the Palworld passport layout) from the [Achievements](https://github.com/PSerban93/Achievements) desktop app data.

## Architecture & Technology Stack

- **GUI Framework**: `eframe` / `egui` (0.29) with GPU rendering via WGPU.
- **Graphic Rendering**: `resvg` (0.44), `usvg` (0.44), and `tiny-skia` (0.11). The generator constructs vector SVG documents with HUD chamfered geometry, gradients, and custom icons, then rasterizes to high-resolution PNG (1200 × 850).
- **Data Integration**:
  - Automatically locates `%APPDATA%\Achievements\` on Windows (and `~/.config/Achievements/` on Linux).
  - Reads `dashboard-summary.json` for verified achievement counts, completion rates, and platform sources.
  - Reads `preferences.json` and Steam's `loginusers.vdf` to resolve the active gamer tag (`PersonaName`).
- **File Dialogs**: `rfd` (0.15) for native OS folder and file pickers.

## Project Structure

```
src/
├── main.rs       # Application entry point and native window configuration
├── app.rs        # egui GUI implementation, state, controls, and live preview
├── data.rs       # Data models, parser for Achievements AppData & Steam users
├── generator.rs  # SVG layout generator and resvg PNG rasterizer
└── icons.rs      # Embedded vector SVG HUD icons
```

## Development & Agent Workflow

When working on features, bug fixes, or enhancements in this repository, always follow this workflow:
1. **Issue First**: Every task must have an associated GitHub Issue. Check existing open issues first (`gh issue list`). If an issue already exists for the task, use it; otherwise create a new issue before writing code (`gh issue create`).
2. **Feature Branch**: Create a dedicated feature branch for the work (e.g., `git checkout -b <issue-number>-<short-description>`). Never commit feature work directly to `main`.
3. **Targeted Implementation**: Keep changes scoped strictly to the task. Respect Git safety rules (report modified files, preserve unrelated code and comments).
4. **Pull Request**: Push the branch and open a Pull Request (`gh pr create`) linking the issue (e.g., `Fixes #<issue-id>`).
5. **Merge & Close**: Once verified, merge the PR into `main` (`gh pr merge --squash --delete-branch`), ensuring the linked issue is automatically closed.

## Build & CI Guidelines

- **Do Not Build on Resource-Constrained Containers**: Running full GUI Rust builds (with WGPU and graphics dependencies) inside minimal LXC containers can consume significant memory and CPU.
- **GitHub Actions Workflow**: Continuous integration and release builds are handled automatically in `.github/workflows/build.yml` on `windows-latest` (producing `.exe`) and `ubuntu-latest`.
- **Styling Rules**: Maintain the 1200 × 850 aspect ratio for passport outputs. Any new icons should be clean SVG paths added to `src/icons.rs`.
