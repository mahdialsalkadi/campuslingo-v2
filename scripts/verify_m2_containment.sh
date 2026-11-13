#!/usr/bin/env bash
# Disposable user service only: no sudo, persistent unit, or existing service
# changes. DelegateSubgroup keeps the supervisor out of the delegation root.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "${1:-}" != "--inside" ]]; then
  exec systemd-run --user --wait --pipe --collect \
    --property=Type=exec --property=Delegate=memory \
    --property=DelegateSubgroup=supervisor \
    --working-directory="$ROOT" \
    /bin/bash "$ROOT/scripts/verify_m2_containment.sh" --inside
fi

cd "$ROOT"
membership="$(sed -n 's/^0:://p' /proc/self/cgroup)"
if [[ "$membership" != */supervisor ]]; then
  echo "BLOCKED: service manager did not create the delegated supervisor subgroup" >&2
  exit 1
fi
export CAMPUSLINGO_HELPER_CGROUP_ROOT="/sys/fs/cgroup${membership%/supervisor}"
export TMPDIR="${TMPDIR:-/tmp/opencode}"
if ! grep -qw memory "$CAMPUSLINGO_HELPER_CGROUP_ROOT/cgroup.subtree_control"; then
  echo +memory > "$CAMPUSLINGO_HELPER_CGROUP_ROOT/cgroup.subtree_control"
fi

cargo test -p campuslingo-ingestion --lib --locked
cargo test -p campuslingo-ingestion --lib --locked -- --ignored --test-threads=1
