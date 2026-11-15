#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"
REPO_ROOT="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
GODOT_PROJECT="$REPO_ROOT/apps/godot"
NATIVE_BIN="$REPO_ROOT/apps/godot/builds/CampusLingoVillageClash.x86_64"

MODE="desktop"
OFFLINE=0
FLAG_DEMO=0
FLAG_STUDY=0
FLAG_RTL=0
FLAG_EDIT=0
KEEP_SERVER=0
PORT="${CAMPUSLINGO_PORT:-18080}"
EXTRA_ARGS=()

usage() {
  cat <<'EOF'
CampusLingo: Village Clash launcher

Usage:
  ./run_game.sh [options] [-- extra_godot_args...]

Modes:
  (default), --desktop   Start live backend (if needed) and launch native Linux game window
  --web                  Start live backend in background and open WebGL2 game in browser
  --server, --server-only Start live backend in background and print the Web URL
  --editor               Open the Godot project in the Godot 4 editor
  --headless-test        Run headless E2E verification against live backend
  --status               Show live backend & PostgreSQL status
  --logs                 Print recent backend logs
  --stop                 Stop the background backend started by run_game.sh

Gameplay / Client Options:
  --demo                 Open straight into demo raid showcase
  --study                Open study flashcard drawer on launch
  --rtl                  Enable Arabic RTL UI preview
  --edit                 Show village placement edit grid
  --offline, --local     Run without syncing to the live backend
  --keep-server          Keep auto-started backend running after closing desktop window
  --port <PORT>          Backend HTTP port (default: 18080)
  -h, --help             Show this help message
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --desktop)
      MODE="desktop"
      ;;
    --web)
      MODE="web"
      ;;
    --server|--server-only)
      MODE="server"
      ;;
    --editor)
      MODE="editor"
      ;;
    --headless-test)
      MODE="headless-test"
      ;;
    --status)
      MODE="status"
      ;;
    --logs)
      MODE="logs"
      ;;
    --stop)
      MODE="stop"
      ;;
    --offline|--local)
      OFFLINE=1
      ;;
    --demo|--demo-raid)
      FLAG_DEMO=1
      ;;
    --study)
      FLAG_STUDY=1
      ;;
    --rtl)
      FLAG_RTL=1
      ;;
    --edit|--show-edit-grid)
      FLAG_EDIT=1
      ;;
    --keep-server)
      KEEP_SERVER=1
      ;;
    --port)
      if [ "$#" -lt 2 ]; then
        echo "Error: --port requires a port number" >&2
        exit 1
      fi
      PORT="$2"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      EXTRA_ARGS+=("$@")
      break
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
PID_FILE="$RUNTIME_DIR/campuslingo-game-${PORT}.pid"
LOG_FILE="$RUNTIME_DIR/campuslingo-game-${PORT}.log"
BASE_URL="http://127.0.0.1:${PORT}"

resolve_node() {
  local candidate
  for candidate in \
    "${CAMPUSLINGO_NODE_BIN:-}" \
    "$HOME/.proto/bin/node" \
    "$HOME/.local/bin/node" \
    "/usr/bin/node" \
    "/usr/local/bin/node"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  candidate="$(command -v node 2>/dev/null || true)"
  if [ -n "$candidate" ] && [[ "$candidate" != *"/.proto/shims/"* ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi
  echo "Error: non-shim node binary not found (tried \$HOME/.proto/bin/node, \$HOME/.local/bin/node, /usr/bin/node)." >&2
  exit 1
}

resolve_godot() {
  local candidate
  for candidate in \
    "${CAMPUSLINGO_GODOT_BIN:-}" \
    "$(command -v godot 2>/dev/null || true)" \
    "$(command -v godot4 2>/dev/null || true)" \
    "$HOME/.local/bin/godot"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "Error: Godot 4 binary not found in PATH or \$HOME/.local/bin/godot" >&2
  exit 1
}

server_ready() {
  curl -sf "${BASE_URL}/readyz" >/dev/null 2>&1
}

is_campuslingo_server() {
  curl -sf "${BASE_URL}/healthz" >/dev/null 2>&1
}

STARTED_STACK_PID=""
CLEANUP_ON_EXIT=0

cleanup() {
  if [ "$CLEANUP_ON_EXIT" -eq 1 ]; then
    if [ -n "$STARTED_STACK_PID" ] && kill -0 "$STARTED_STACK_PID" 2>/dev/null; then
      echo "Stopping live backend (PID $STARTED_STACK_PID)..."
      kill -TERM "$STARTED_STACK_PID" 2>/dev/null || true
      for _i in $(seq 1 20); do
        if ! kill -0 "$STARTED_STACK_PID" 2>/dev/null; then
          break
        fi
        sleep 0.25
      done
      if kill -0 "$STARTED_STACK_PID" 2>/dev/null; then
        kill -KILL "$STARTED_STACK_PID" 2>/dev/null || true
      fi
    fi
    rm -f "$PID_FILE"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

ensure_backend() {
  local persist="${1:-0}"
  if server_ready; then
    echo "Live backend already running at ${BASE_URL}"
    return 0
  fi

  local node_bin
  node_bin="$(resolve_node)"
  echo "Starting CampusLingo live backend at ${BASE_URL}..."
  if [ "$persist" -eq 1 ]; then
    nohup "$node_bin" "$REPO_ROOT/scripts/run_live_stack.mjs" --port "$PORT" </dev/null >"$LOG_FILE" 2>&1 &
  else
    "$node_bin" "$REPO_ROOT/scripts/run_live_stack.mjs" --port "$PORT" </dev/null >"$LOG_FILE" 2>&1 &
  fi
  STARTED_STACK_PID=$!
  echo "$STARTED_STACK_PID" > "$PID_FILE"

  # Clean up on failed startup even in persistent mode
  CLEANUP_ON_EXIT=1

  for _i in $(seq 1 60); do
    if ! kill -0 "$STARTED_STACK_PID" 2>/dev/null; then
      echo "Backend exited unexpectedly during startup. Recent log:" >&2
      tail -n 30 "$LOG_FILE" >&2 || true
      rm -f "$PID_FILE"
      exit 1
    fi
    if server_ready; then
      break
    fi
    sleep 1
  done

  if ! server_ready; then
    echo "Timed out waiting for ${BASE_URL}/readyz. See ${LOG_FILE}" >&2
    exit 1
  fi

  if [ "$persist" -eq 1 ]; then
    CLEANUP_ON_EXIT=0
  fi
  echo "Backend ready at ${BASE_URL} (PID ${STARTED_STACK_PID}, log: ${LOG_FILE})"
}

stop_backend() {
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      echo "Stopping CampusLingo backend (PID $pid)..."
      kill -TERM "$pid" 2>/dev/null || true
      for _i in $(seq 1 20); do
        if ! kill -0 "$pid" 2>/dev/null; then
          break
        fi
        sleep 0.25
      done
      if kill -0 "$pid" 2>/dev/null; then
        kill -KILL "$pid" 2>/dev/null || true
      fi
      rm -f "$PID_FILE"
      echo "Backend stopped."
      return 0
    fi
    rm -f "$PID_FILE"
  fi

  if server_ready; then
    echo "Backend is responding at ${BASE_URL}, but no managed PID file exists at ${PID_FILE}." >&2
    return 1
  fi
  echo "No managed CampusLingo backend running on port ${PORT}."
}

print_status() {
  if server_ready; then
    local health
    health="$(curl -sf "${BASE_URL}/healthz" 2>/dev/null || echo '{}')"
    echo "Backend: UP at ${BASE_URL} (${health})"
  elif is_campuslingo_server; then
    echo "Backend: STARTING at ${BASE_URL}"
  else
    echo "Backend: DOWN at ${BASE_URL}"
  fi
  if [ -f "$PID_FILE" ]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      echo "Managed PID: ${pid} (${PID_FILE})"
    else
      echo "Managed PID: stale (${PID_FILE})"
    fi
  else
    echo "Managed PID: none"
  fi
  echo "Log file: ${LOG_FILE}"
}

build_web_url() {
  local params=()
  [ "$FLAG_DEMO" -eq 1 ] && params+=("demo=1")
  [ "$FLAG_STUDY" -eq 1 ] && params+=("study=1")
  [ "$FLAG_RTL" -eq 1 ] && params+=("rtl=1")
  [ "$FLAG_EDIT" -eq 1 ] && params+=("edit=1")
  [ "$OFFLINE" -eq 1 ] && params+=("local=1")

  local url="${BASE_URL}/"
  if [ "${#params[@]}" -gt 0 ]; then
    local joined
    joined="$(IFS='&'; echo "${params[*]}")"
    url="${url}?${joined}"
  fi
  printf '%s\n' "$url"
}

open_browser() {
  local url="$1"
  if [ "${CAMPUSLINGO_NO_BROWSER:-0}" = "1" ]; then
    echo "Browser open skipped (CAMPUSLINGO_NO_BROWSER=1): ${url}"
    return 0
  fi
  if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "No GUI display detected; open manually in your browser: ${url}"
    return 0
  fi
  if command -v xdg-open >/dev/null 2>&1; then
    nohup xdg-open "$url" >/dev/null 2>&1 </dev/null &
  elif command -v firefox >/dev/null 2>&1; then
    nohup firefox "$url" >/dev/null 2>&1 </dev/null &
  elif command -v chromium >/dev/null 2>&1; then
    nohup chromium "$url" >/dev/null 2>&1 </dev/null &
  else
    echo "No browser launcher (xdg-open/firefox/chromium) found; open: ${url}"
  fi
}

build_godot_user_args() {
  local args=()
  [ "$OFFLINE" -eq 0 ] && args+=("--live-connect")
  [ "$FLAG_DEMO" -eq 1 ] && args+=("--demo-raid")
  [ "$FLAG_STUDY" -eq 1 ] && args+=("--study")
  [ "$FLAG_RTL" -eq 1 ] && args+=("--rtl")
  [ "$FLAG_EDIT" -eq 1 ] && args+=("--show-edit-grid")
  if [ "${#EXTRA_ARGS[@]}" -gt 0 ]; then
    args+=("${EXTRA_ARGS[@]}")
  fi
  if [ "${#args[@]}" -gt 0 ]; then
    printf '%s\n' "${args[@]}"
  fi
}

case "$MODE" in
  stop)
    stop_backend
    ;;
  status)
    print_status
    ;;
  logs)
    if [ -f "$LOG_FILE" ]; then
      tail -n 80 "$LOG_FILE"
    else
      echo "No log file at ${LOG_FILE}"
    fi
    ;;
  server)
    ensure_backend 1
    echo "Play WebGL2 in browser: $(build_web_url)"
    echo "Stop server with:       $0 --stop --port ${PORT}"
    ;;
  web)
    ensure_backend 1
    WEB_URL="$(build_web_url)"
    echo "Opening CampusLingo WebGL2 client: ${WEB_URL}"
    echo "Stop server when done:             $0 --stop --port ${PORT}"
    open_browser "$WEB_URL"
    ;;
  editor)
    export CAMPUSLINGO_API_URL="$BASE_URL"
    export CAMPUSLINGO_SERVER_URL="$BASE_URL"
    GODOT_BIN="$(resolve_godot)"
    exec "$GODOT_BIN" --path "$GODOT_PROJECT" --editor
    ;;
  headless-test)
    ensure_backend 0
    export CAMPUSLINGO_API_URL="$BASE_URL"
    export CAMPUSLINGO_SERVER_URL="$BASE_URL"
    GODOT_BIN="$(resolve_godot)"
    "$GODOT_BIN" --headless --path "$GODOT_PROJECT" --import --quit >/dev/null 2>&1 || true
    "$GODOT_BIN" --headless --path "$GODOT_PROJECT" --script res://scripts/verify_e2e_headless.gd
    ;;
  desktop)
    if [ "$OFFLINE" -eq 0 ]; then
      ensure_backend "$KEEP_SERVER"
    fi
    export CAMPUSLINGO_API_URL="$BASE_URL"
    export CAMPUSLINGO_SERVER_URL="$BASE_URL"

    mapfile -t USER_ARGS < <(build_godot_user_args)
    if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
      echo "Error: No DISPLAY or WAYLAND_DISPLAY set for native window." >&2
      echo "Use 'xvfb-run $0' or '$0 --server --port ${PORT}' to play via browser." >&2
      exit 1
    fi

    if [ -x "$NATIVE_BIN" ]; then
      echo "Launching native desktop client: ${NATIVE_BIN}"
      if [ "${#USER_ARGS[@]}" -gt 0 ]; then
        "$NATIVE_BIN" -- "${USER_ARGS[@]}"
      else
        "$NATIVE_BIN"
      fi
    else
      GODOT_BIN="$(resolve_godot)"
      echo "Native binary not found at ${NATIVE_BIN}; launching via Godot project..."
      if [ "${#USER_ARGS[@]}" -gt 0 ]; then
        "$GODOT_BIN" --path "$GODOT_PROJECT" -- "${USER_ARGS[@]}"
      else
        "$GODOT_BIN" --path "$GODOT_PROJECT"
      fi
    fi
    ;;
esac
