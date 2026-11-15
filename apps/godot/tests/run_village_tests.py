#!/usr/bin/env python3
"""Gate M1 Automated Verification Runner for CampusLingo: Village Clash (Modules + REA Parity)."""
import csv
import json
import subprocess
import sys
from pathlib import Path

GODOT_DIR = Path(__file__).resolve().parent.parent
TOOLS_PY = GODOT_DIR / "tools" / "godot.py"

M1_SECTIONS = [
    ("SEC-G01", "scripts/village/grid_math.gd"),
    ("SEC-G02", "scripts/village/grid_board_renderer.gd"),
    ("SEC-G03", "scripts/village/camera_pan_zoom.gd"),
    ("SEC-D01", "scripts/buildings/defenses/quill_archer_spire.gd"),
    ("SEC-D02", "scripts/buildings/defenses/ink_cannon.gd"),
    ("SEC-D03", "scripts/buildings/defenses/rune_mortar.gd"),
    ("SEC-D04", "scripts/buildings/defenses/hidden_proctor_tesla.gd"),
    ("SEC-SIM01", "scripts/combat/astar_wall_pathfinder.gd"),
    ("SEC-SIM02", "scripts/combat/raid_tick_star_solver.gd"),
    ("SEC-T01", "scripts/troops/squire_footman.gd"),
    ("SEC-T02", "scripts/troops/lexi_archer.gd"),
    ("SEC-T03", "scripts/troops/thesis_breaker.gd"),
    ("SEC-T04", "scripts/troops/formula_giant.gd"),
    ("SEC-T05", "scripts/troops/glyph_drake.gd"),
]

KIT_PARITY_ARTIFACTS = [
    ".agent/skills/rea-physics-reverse/SKILL.md",
    ".agent/skills/godot-autonomous-orchestrator/SKILL.md",
    "Docs/MOVEMENT_REFERENCE.md",
    "private_research/derived_reference.json",
    "scenes/combat/village_profile.tres",
    "tests/run_movement_tests.py",
]


def check_modularity() -> bool:
    ok = True
    for sec_id, rel in M1_SECTIONS:
        p = GODOT_DIR / rel
        if not p.exists():
            print(f"[FAIL] Missing M1 section {sec_id}: {rel}")
            ok = False
            continue
        lines = len(p.read_text(encoding="utf-8").splitlines())
        if lines > 150:
            print(f"[FAIL] {sec_id} ({rel}) has {lines} lines (> 150 limit)")
            ok = False
        else:
            print(f"[MODULAR OK] {sec_id:8s} {lines:3d}L {rel}")
    return ok


def check_open_source_assets_and_kit() -> bool:
    for rel in KIT_PARITY_ARTIFACTS:
        if not (GODOT_DIR / rel).exists():
            print(f"[FAIL] Missing Kit/REA artifact: {rel}")
            return False
    reg = GODOT_DIR / "data" / "asset_register.csv"
    if not reg.exists():
        print("[FAIL] Missing data/asset_register.csv")
        return False
    rows = list(csv.DictReader(reg.open(encoding="utf-8")))
    for r in rows:
        fp = GODOT_DIR / r["local_path"]
        if not fp.exists() or fp.stat().st_size == 0 or r["magic_verified"] != "True":
            print(f"[FAIL] Invalid open-source asset row: {r['local_path']}")
            return False
    print(f"[ASSETS & REA OK] {len(rows)} CC0/MIT assets & {len(KIT_PARITY_ARTIFACTS)} Kit/REA artifacts verified.")
    return True


def run_godot_m1_suites() -> bool:
    cmd = [sys.executable, str(TOOLS_PY), "script", "res://scripts/combat/verify_m1_headless.gd"]
    proc = subprocess.run(cmd, cwd=GODOT_DIR, capture_output=True, text=True)
    print(proc.stdout)
    if proc.stderr.strip():
        print(proc.stderr)
    if proc.returncode != 0:
        print(f"[FAIL] Headless M1 verification exited with {proc.returncode}")
        return False
    stats_path = GODOT_DIR / "data" / "stats.json"
    if not stats_path.exists():
        print("[FAIL] data/stats.json not written")
        return False
    stats = json.loads(stats_path.read_text(encoding="utf-8"))
    if not (stats.get("passed") == 7 and stats.get("total") == 7):
        return False
    parity = subprocess.run([sys.executable, str(GODOT_DIR / "tests" / "run_movement_tests.py")],
                            cwd=GODOT_DIR, capture_output=True, text=True)
    print(parity.stdout)
    if parity.stderr.strip():
        print(parity.stderr)
    return parity.returncode == 0


def main() -> int:
    ok = True
    ok = check_modularity() and ok
    ok = check_open_source_assets_and_kit() and ok
    ok = run_godot_m1_suites() and ok
    print("GATE_M1_TESTS=" + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
