# Agent Working Guide: TrophyVault

This repository contains **TrophyVault**, a pure Web/Server Rust workspace powering a self-hosted gaming achievements showcase, direct Steam Web API ingestion engine, and dynamic SVG passport generator.

## Architecture & Technology Stack

The project is structured as a Cargo workspace with two crates:

- **`trophyvault-core` (`crates/core`)**: Shared data models, SQLite storage engine (`trophyvault.db` tables `games`, `trophies`, `metadata`, `preferences`, `manual_games`), SVG generator (`resvg` 0.44, `tiny-skia` 0.11), and Bloodborne/Steam helpers.
- **`trophyvault-server` (`crates/server`)**: Headless web server and REST API built with `axum` (0.7), `tokio`, and `reqwest`. Serves the cyberpunk web showcase (`static/`), ingests Steam library & achievement data via the Steam Web API (`src/steam.rs`), manages Two Settings (`Steam API` + `CloudRedirect API` placeholder), renders dynamic SVG passport cards and badges, and broadcasts live SSE updates. Packaged for production via multi-stage Docker (`Dockerfile`).

## Project Structure

```text
.
├── Cargo.toml                # Workspace definition (default-members = ["crates/server"])
├── crates/
│   ├── core/                 # Shared data models, SQLite storage, SVG generator
│   │   └── src/
│   └── server/               # Axum web server, Steam API ingestion, static assets
│       ├── src/
│       ├── static/           # Vanilla JS, CSS, HTML showcase frontend
│       └── tests/            # API & Steam ingestion integration tests
├── .env.example              # Environment configuration template (Steam + CloudRedirect)
├── Dockerfile                # Multi-stage production container build
├── docker-compose.yml        # Deployment compose manifest
└── .github/workflows/        # CI test/build and GHCR Docker release workflows
```

## Development & Agent Workflow

When developing features, bug fixes, or enhancements:

1. **Issue Creation & Assignment Mandate**:
   - Every task must have an associated GitHub Issue. Check existing open issues (`gh issue list`).
   - When creating an issue (`gh issue create`), **ALWAYS**:
     - Assign it to the author (`MohandL3G`) using `--assignee "@me"`.
     - Assign the appropriate classification label/tag (e.g. `--label "enhancement"` or `--label "bug"`, `--label "documentation"`).
2. **Feature Branch**: Create a dedicated feature branch for the work (e.g., `git checkout -b <issue-number>-<short-description>`). Never commit feature work directly to `main`.
3. **Targeted Implementation**: Keep changes scoped strictly to the task. Maintain clean code and formatting.
4. **Pull Request Mandate**:
   - Push the branch and create a Pull Request using `gh pr create`.
   - **MANDATORY**: Always assign `@me` (`MohandL3G`) to all PRs using `--assignee "@me"`.
   - Always assign appropriate labels/tags where applicable (e.g. `--label "enhancement"` or `--label "bug"`).
   - Reference the issue in the PR description (e.g., `Closes #<issue-id>`).
5. **Merge & Deployment**:
   - Merge verified PRs into `main` using squash merge (`gh pr merge <PR_NUM> --squash --delete-branch`).
   - Verify the GitHub Actions CI & Docker build workflow (`ghcr.io/mohandl3g/trophyvault`), then deploy to CT 100 via Docker Compose.
6. **Visual Verification**:
   - Use the local Playwright MCP in CT 103 to verify UI changes and capture screenshots against the deployed instance (`https://vault.mohandl3g.ly` / `http://192.168.0.100:5050`) before closing tasks.
