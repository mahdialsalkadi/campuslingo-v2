#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT_PROJECT="$REPO_ROOT/apps/godot"
SERVER_URL="${CAMPUSLINGO_SERVER_URL:-http://127.0.0.1:18080}"
GODOT_BIN="$(command -v godot || command -v godot4 || echo "$HOME/.local/bin/godot")"

if [ ! -x "$GODOT_BIN" ]; then
  echo "Godot 4 binary not found in PATH or $HOME/.local/bin/godot" >&2
  exit 1
fi

STARTED_STACK_PID=""
cleanup() {
  if [ -n "$STARTED_STACK_PID" ] && kill -0 "$STARTED_STACK_PID" 2>/dev/null; then
    kill "$STARTED_STACK_PID" 2>/dev/null || true
    wait "$STARTED_STACK_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

if ! curl -sf "$SERVER_URL/readyz" >/dev/null 2>&1; then
  echo "Live CampusLingo backend not detected at $SERVER_URL — starting run_live_stack.mjs..."
  node "$REPO_ROOT/scripts/run_live_stack.mjs" >/tmp/campuslingo_live_stack.log 2>&1 &
  STARTED_STACK_PID=$!
  for _i in $(seq 1 45); do
    if curl -sf "$SERVER_URL/readyz" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  if ! curl -sf "$SERVER_URL/readyz" >/dev/null 2>&1; then
    echo "Timed out waiting for $SERVER_URL/readyz (see /tmp/campuslingo_live_stack.log)" >&2
    exit 1
  fi
fi

export CAMPUSLINGO_SERVER_URL="$SERVER_URL"

case "${1:-}" in
  --editor)
    "$GODOT_BIN" --path "$GODOT_PROJECT" --editor
    ;;
  --headless-test)
    "$GODOT_BIN" --headless --path "$GODOT_PROJECT" --import --quit >/dev/null 2>&1 || true
    "$GODOT_BIN" --headless --path "$GODOT_PROJECT" --script res://scripts/verify_e2e_headless.gd
    ;;
  *)
    "$GODOT_BIN" --path "$GODOT_PROJECT"
    ;;
esac
