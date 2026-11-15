#!/usr/bin/env python3
"""
Automated REA Physics & Combat Parity Test Suite (`tests/run_movement_tests.py`).
Compares live measured Godot 60Hz/20Hz metrics (`tests/results/latest.json`) against
`private_research/derived_reference.json` & `Docs/MOVEMENT_REFERENCE.md` within +-3.0%.
"""

import json
import sys
from pathlib import Path

GODOT_DIR = Path(__file__).resolve().parent.parent
DERIVED_FILE = GODOT_DIR / "private_research" / "derived_reference.json"
RESULTS_FILE = GODOT_DIR / "tests" / "results" / "latest.json"


def load_reference_targets() -> dict:
    if DERIVED_FILE.exists():
        d = json.loads(DERIVED_FILE.read_text(encoding="utf-8"))
        squire = d["troop_kinematics"]["squire_footman"]
        mortar = d["mortar_shell_arc"]
        rates = d["simulation_rates"]
        return {
            "squire_top_speed_mps": float(squire["top_speed_mps"]),
            "squire_accel_90pct_s": float(squire["time_to_90pct_s"]),
            "turn_rate_deg_per_s": float(squire["turn_rate_deg_s"]),
            "mortar_shell_apex_m": float(mortar["measured_60hz_apex_m"]),
            "cannon_brownout_20s_dmg": float(rates["cannon_brownout_20s_dmg"]),
            "breaker_wall_dps": 100.0,
            "astar_short_wall_detour_cells": 9.0,
            "tesla_reveal_radius_cells": 4.0,
            "mortar_deadzone_cells": 4.0,
            "camera_zoom_default_m": 34.0,
        }
    return {
        "squire_top_speed_mps": 2.70,
        "squire_accel_90pct_s": 0.483,
        "turn_rate_deg_per_s": 337.50,
        "mortar_shell_apex_m": 1.804,
        "cannon_brownout_20s_dmg": 312.0,
        "breaker_wall_dps": 100.0,
        "astar_short_wall_detour_cells": 9.0,
        "tesla_reveal_radius_cells": 4.0,
        "mortar_deadzone_cells": 4.0,
        "camera_zoom_default_m": 34.0,
    }


def verify_metrics(measured: dict, ref_targets: dict, tolerance_pct: float = 3.0) -> int:
    failures = 0
    print(f"{'Check':<32} | {'Measured':>10} | {'Reference':>10} | {'Diff %':>8} | Status")
    print("-" * 78)
    for key, ref_val in ref_targets.items():
        val = measured.get(key)
        if val is None:
            print(f"{key:<32} | {'MISSING':>10} | {ref_val:>10.3f} | {'N/A':>8} | FAIL")
            failures += 1
            continue
        diff_pct = ((float(val) - ref_val) / ref_val) * 100.0 if ref_val != 0 else 0.0
        ok = abs(diff_pct) <= tolerance_pct
        if not ok:
            failures += 1
        status = "PASS" if ok else "FAIL"
        print(f"{key:<32} | {float(val):>10.3f} | {ref_val:>10.3f} | {diff_pct:>+7.2f}% | {status}")
    return failures


def main() -> int:
    if not RESULTS_FILE.exists():
        print(f"[FAIL] Missing measured telemetry file: {RESULTS_FILE}. Run tests/run_village_tests.py first.")
        return 1
    measured = json.loads(RESULTS_FILE.read_text(encoding="utf-8"))
    ref_targets = load_reference_targets()
    fails = verify_metrics(measured, ref_targets, tolerance_pct=3.0)
    print(f"MOVEMENT_PARITY={'PASS' if fails == 0 else 'FAIL'} ({len(ref_targets) - fails}/{len(ref_targets)})")
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
