#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"

echo "=== Building TrophyVault Desktop ==="
source /root/.cargo/env
cargo build -p trophyvault

echo "=== Starting Xvfb ==="
export DISPLAY=:99
Xvfb :99 -screen 0 1280x800x24 &
XVFB_PID=$!
trap "kill $XVFB_PID 2>/dev/null || true" EXIT
sleep 1

echo "=== Launching TrophyVault ==="
export LIBGL_ALWAYS_SOFTWARE=1
"$DIR/target/debug/TrophyVault" &
APP_PID=$!
trap "kill $APP_PID 2>/dev/null || true; kill $XVFB_PID 2>/dev/null || true" EXIT
sleep 3

WID=$(xdotool search --name TrophyVault | head -n 1)
echo "Found TrophyVault Window ID: $WID"

OUT_DIR="/root/.gemini/antigravity-cli/brain/f6237633-ef48-48f0-b6e5-a0de32042c0c"

# Wake up window & Capture Tab 1: Library & Stats
xdotool mousemove 600 400
sleep 1
scrot -o "$OUT_DIR/gui_v3_library.png"

# Click Tab 2: Bloodborne Port (sidebar y~220)
xdotool mousemove 90 220 click 1
sleep 1
scrot -o "$OUT_DIR/gui_v3_bloodborne.png"

# Click Tab 3: Cloud Sync (sidebar y~265)
xdotool mousemove 90 265 click 1
sleep 1
scrot -o "$OUT_DIR/gui_v3_cloudsync.png"

# Click Tab 4: Settings (sidebar y~310)
xdotool mousemove 90 310 click 1
sleep 1
scrot -o "$OUT_DIR/gui_v3_settings.png"

echo "=== Headless GUI Capture Complete ==="
ls -lh "$OUT_DIR"/gui_v3_*.png
