# TrophyVault

A self-hosted **Rust** web showcase server, Steam Web API ingestion engine, and dynamic SVG Achievements Passport generator built with **Axum**, **Tokio**, and **SQLite**.

Inspired by the Palworld Passport HUD, TrophyVault aggregates your Steam library (playtime, achievements, and unlock timestamps) alongside preserved PC Ports/Decomps (such as **Bloodborne**) into a live cyberpunk showcase and embeddable SVG passport cards with an overall rating badge (**OVR**).

---

## Key Features

- **Direct Steam Web API Ingestion**: Pulls owned games, cumulative playtime, player summary, achievement schemas, and unlocked achievement timestamps directly from the Steam Web API (`IPlayerService`, `ISteamUser`, `ISteamUserStats`).
- **Auto-Sync with Cooldown Protection**: Opening or refreshing the web dashboard triggers a non-blocking background sync respecting `STEAM_SYNC_COOLDOWN_SECS` (default `300s`), returning cached SQLite data instantly and refreshing the UI via Server-Sent Events (SSE) when new data arrives.
- **Monotonic Playtime & Historical Retention**: Preserves the high-watermark `MAX(stored, incoming)` playtime and retains historical entries (including Bloodborne PC port and custom entries) in SQLite (`trophyvault.db`).
- **Two Settings Architecture**:
  - **Steam Web API**: `STEAM_API_KEY`, `STEAM_ID`, and `STEAM_SYNC_COOLDOWN_SECS`.
  - **CloudRedirect API (Placeholder)**: `CLOUDREDIRECT_API_URL` and `CLOUDREDIRECT_API_KEY` ready for future CloudRedirect daemon integration.
  - Configurable via `.env`/environment variables or the admin-protected Settings modal in the Web UI.
- **Dynamic GitHub SVG Embeds**:
  - `GET /api/passport.svg` — Overall or single-game 1200 × 850 SVG Passport (5 cyber-tactical themes).
  - `GET /api/badge.svg` — Compact 400 × 100 GitHub Profile HUD Badge.
  - Cache-bypassing headers for live GitHub Camo rendering.

---

## Quick Start (Docker Compose)

1. Copy `.env.example` to `.env` and set `ADMIN_PASSWORD`, `API_KEY`, `STEAM_API_KEY`, and `STEAM_ID`:
   ```bash
   cp .env.example .env
   ```
2. Launch the server:
   ```bash
   docker compose up -d
   ```
3. Open `http://localhost:5050` in your browser.

---

## Development

```bash
# Run all workspace unit & integration tests
cargo test --workspace

# Run server locally
ADMIN_PASSWORD=dev-admin STEAM_ID=76561198423209668 cargo run -p trophyvault-server
```
