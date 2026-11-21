# Agent Working Guide: TrophyVault

This repository contains **TrophyVault**, a multi-component Rust workspace combining a futuristic desktop application and a web showcase server for achievements, trophies, and passport card generation.

## Architecture & Technology Stack

The project is structured as a Cargo workspace with three primary crates:

- **`trophyvault_core` (`crates/core`)**: Shared models, database storage (`library_database.json`), SVG generator (`resvg` 0.44, `tiny-skia` 0.11), and integrations (Steam Web API, VDF parser, Bloodborne/bbport save parser).
- **`trophyvault_desktop` (`crates/desktop`)**: Desktop native UI built with `eframe` / `egui` (0.29) and Glow OpenGL renderer. Supports interactive card generation, theme customization, Steam import, and local sync.
- **`trophyvault_server` (`crates/server`)**: High-performance headless web server and REST API built with `axum` (0.7) and `tokio`. Serves the cyberpunk web showcase (`static/`), dynamic passport cards, embeds, badges, and health/stats endpoints. Packaged for production via multi-stage Docker (`Dockerfile`).

## Project Structure

```
.
├── Cargo.toml                # Workspace definition
├── crates/
│   ├── core/                 # Shared data models, storage, parsers, generator
│   │   └── src/
│   ├── desktop/              # egui desktop GUI client
│   │   └── src/
│   └── server/               # Axum web server and static assets
│       ├── src/
│       ├── static/           # Vanilla JS, CSS, HTML showcase frontend
│       └── tests/            # API integration tests
├── Dockerfile                # Multi-stage production container build
├── docker-compose.yml        # Deployment compose manifest
└── .github/workflows/        # CI, test, build, and GHCR Docker release workflows
```

## Development & Agent Workflow

When developing features, bug fixes, or enhancements:

1. **Issue First**: Every task must have an associated GitHub Issue. Check existing open issues (`gh issue list`). If none exists, create one (`gh issue create`).
2. **Feature Branch**: Create a dedicated feature branch for the work (e.g., `git checkout -b <issue-number>-<short-description>`). Never commit feature work directly to `main`.
3. **Targeted Implementation**: Keep changes scoped strictly to the task. Maintain clean code and formatting.
4. **Pull Request Mandate**:
   - Push the branch and create a Pull Request using `gh pr create`.
   - **MANDATORY**: Always assign `@me` (`MohandL3G`) to all PRs using `--assignee "@me"`.
   - Reference the issue in the PR description (e.g., `Closes #<issue-id>`).
5. **Merge & Deployment**:
   - Merge verified PRs into `main` using squash merge (`gh pr merge <PR_NUM> --squash --delete-branch`).
   - For web/server updates, verify the GitHub Actions Docker build workflow, then deploy to CT 100 via Docker Compose.
6. **Visual Verification**:
   - Use the local Playwright MCP in CT 103 to verify UI changes and capture screenshots against the deployed instance (`http://192.168.0.100:5050`) before closing tasks.
