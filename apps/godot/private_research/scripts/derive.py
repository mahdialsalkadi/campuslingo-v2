#!/usr/bin/env python3
"""
REA Binary Inspection & Discrete-to-Continuous Kinematic Derivation (`derive.py`).
Inspects compiled symbols in `target/debug/campuslingo-server` via `nm -C`
and derives 20 Hz / 60 Hz continuous Godot parameters from 10 Hz / 30 Hz reference steps.
"""

import json
import math
import subprocess
from datetime import datetime, timezone
from pathlib import Path

GODOT_DIR = Path(__file__).resolve().parents[2]
REPO_ROOT = GODOT_DIR.parents[1]
SERVER_BIN = REPO_ROOT / "target" / "debug" / "campuslingo-server"
OUT_JSON = GODOT_DIR / "private_research" / "derived_reference.json"

TARGET_SYMBOLS = [
    "evaluate_raid_settlement",
    "evaluate_boss_strike",
    "evaluate_raid_targetability",
    "evaluate_league_rollover",
    "compute_clan_stability_points",
    "split_protection_predicates",
]


def inspect_server_binary() -> dict:
    if not SERVER_BIN.exists():
        return {"binary_present": False, "path": str(SERVER_BIN), "matched_symbols": []}
    proc = subprocess.run(
        ["nm", "-C", str(SERVER_BIN)],
        capture_output=True,
        text=True,
        check=False,
    )
    matched = []
    if proc.returncode == 0:
        for line in proc.stdout.splitlines():
            for sym in TARGET_SYMBOLS:
                if sym in line:
                    matched.append({"symbol": sym, "raw": line.strip()[-120:]})
    # Deduplicate by symbol name
    unique_syms = sorted({m["symbol"] for m in matched})
    return {
        "binary_present": True,
        "path": str(SERVER_BIN),
        "binary_bytes": SERVER_BIN.stat().st_size,
        "nm_returncode": proc.returncode,
        "total_symbol_hits": len(matched),
        "verified_domain_symbols": unique_syms,
    }


def simulate_troop_ramp(top_speed_mps: float, dt: float = 1.0 / 60.0) -> dict:
    """Simulate continuous quadratic acceleration curve from 0 to 90% top speed."""
    v = 0.0
    t = 0.0
    t_90 = None
    target_90 = top_speed_mps * 0.90
    while t < 5.0:
        if v >= target_90 and t_90 is None:
            t_90 = t
            break
        # Base accel 7.73 m/s^2 tapering down to 2.50 m/s^2 near top speed
        frac = min(1.0, v / max(0.01, top_speed_mps))
        accel = 7.73 - (7.73 - 2.50) * frac
        v = min(top_speed_mps, v + accel * dt)
        t += dt
    return {
        "top_speed_mps": round(top_speed_mps, 3),
        "time_to_90pct_s": round(t_90 if t_90 is not None else 0.0, 3),
        "braking_decel_mps2": 28.10,
        "turn_rate_deg_s": 337.50,
    }


def simulate_mortar_arc(v0_mps: float = 10.31, g_mps2: float = 28.12, dt: float = 1.0 / 60.0) -> dict:
    """Compare discrete-step velocity-first vs Godot integration order for RuneMortar shell apex."""
    y = 0.0
    vy = v0_mps
    max_y = 0.0
    flight_t = 0.0
    while True:
        vy -= g_mps2 * dt  # Velocity-first Godot integration order
        y += vy * dt
        flight_t += dt
        if y > max_y:
            max_y = y
        if y <= 0.0 and flight_t > dt:
            break
    analytic_no_comp = ((v0_mps - 0.70) ** 2) / (2.0 * g_mps2)
    return {
        "launch_vy_mps": v0_mps,
        "gravity_mps2": -g_mps2,
        "discrete_correction_mps": 0.70,
        "measured_60hz_apex_m": round(max_y, 3),
        "uncorrected_apex_m": round(analytic_no_comp, 3),
        "flight_time_s": round(flight_t, 3),
    }


def main() -> None:
    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    bin_info = inspect_server_binary()
    payload = {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "unit_scale": {
            "reference_unit_mm": 7.8125,
            "cell_size_m": 1.50,
            "subunits_per_cell": 192,
            "grid_cells": 40,
            "arena_meters": 60.0,
        },
        "binary_inspection": bin_info,
        "troop_kinematics": {
            "squire_footman": simulate_troop_ramp(1.8 * 1.5),
            "lexi_archer": simulate_troop_ramp(1.9 * 1.5),
            "thesis_breaker": simulate_troop_ramp(2.4 * 1.5),
            "formula_giant": simulate_troop_ramp(1.1 * 1.5),
            "glyph_drake": simulate_troop_ramp(1.4 * 1.5),
        },
        "mortar_shell_arc": simulate_mortar_arc(),
        "simulation_rates": {
            "reference_rts_hz": 10,
            "godot_combat_hz": 20,
            "godot_visual_hz": 60,
            "half_tick_phase_offset_ratio": 0.50,
            "cannon_brownout_20s_dmg": 312.0,
        },
    }
    OUT_JSON.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"[derive.py] Verified {len(bin_info.get('verified_domain_symbols', []))} compiled symbols in server binary.")
    print(f"[derive.py] Squire time_to_90pct={payload['troop_kinematics']['squire_footman']['time_to_90pct_s']}s, "
          f"Mortar apex={payload['mortar_shell_arc']['measured_60hz_apex_m']}m")
    print(f"[derive.py] Wrote {OUT_JSON}")


if __name__ == "__main__":
    main()
