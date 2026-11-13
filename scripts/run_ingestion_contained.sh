#!/usr/bin/env bash
# Run one explicit local command under a temporary delegated user service.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" != "--inside" ]]; then
  [[ $# -gt 0 ]] || { echo "usage: bash scripts/run_ingestion_contained.sh COMMAND [ARG...]" >&2; exit 2; }
  exec systemd-run --user --wait --pipe --collect \
    --property=Type=exec --property=Delegate=memory \
    --property=DelegateSubgroup=supervisor --working-directory="$ROOT" \
    /bin/bash "$ROOT/scripts/run_ingestion_contained.sh" --inside "$@"
fi
shift
cd "$ROOT"
membership="$(sed -n 's/^0:://p' /proc/self/cgroup)"
[[ "$membership" == */supervisor ]] || { echo "BLOCKED: missing delegated supervisor subgroup" >&2; exit 1; }
export CAMPUSLINGO_HELPER_CGROUP_ROOT="/sys/fs/cgroup${membership%/supervisor}"
export TMPDIR="${TMPDIR:-/tmp/opencode}"
if ! grep -qw memory "$CAMPUSLINGO_HELPER_CGROUP_ROOT/cgroup.subtree_control"; then
  echo +memory > "$CAMPUSLINGO_HELPER_CGROUP_ROOT/cgroup.subtree_control"
fi
exec "$@"
