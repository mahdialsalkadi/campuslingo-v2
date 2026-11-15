"""Master M1/M2 Blender 5.3.0 asset generator and asset_register.csv updater."""

import csv
import hashlib
import json
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
if SCRIPT_DIR not in sys.path:
    sys.path.insert(0, SCRIPT_DIR)

import blender_models_buildings as bld_mod  # noqa: E402
import blender_models_defenses as def_mod  # noqa: E402
import blender_models_troops as trp_mod  # noqa: E402

ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, ".."))
MODELS_ROOT = os.path.join(ROOT, "assets", "models")
REG_PATH = os.path.join(ROOT, "data", "asset_register.csv")


def _collect_glb_paths(report: object, acc: list[str]) -> None:
    if isinstance(report, dict):
        if "path" in report and isinstance(report["path"], str):
            acc.append(report["path"])
        for v in report.values():
            _collect_glb_paths(v, acc)


def _update_register(paths: list[str]) -> int:
    rows: dict[str, dict] = {}
    fieldnames = ["local_path", "pack", "license", "bytes", "sha256_16", "source_url", "magic_verified"]
    if os.path.exists(REG_PATH):
        with open(REG_PATH, newline="", encoding="utf-8") as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows[row["local_path"]] = row

    for p in paths:
        rel = os.path.relpath(p, ROOT).replace("\\", "/")
        raw = open(p, "rb").read()
        sha = hashlib.sha256(raw).hexdigest()[:16]
        rows[rel] = {
            "local_path": rel,
            "pack": "Blender 5.3.0 Clean-Room M1 Architecture",
            "license": "CC0-1.0",
            "bytes": str(len(raw)),
            "sha256_16": sha,
            "source_url": "local://tools/blender_build_m1_models.py",
            "magic_verified": str(raw[:4] == b"glTF"),
        }

    sorted_keys = sorted(rows.keys())
    with open(REG_PATH, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for k in sorted_keys:
            writer.writerow(rows[k])
    return len(sorted_keys)


def main() -> None:
    report = {
        "defenses": def_mod.build_all(MODELS_ROOT),
        "buildings": bld_mod.build_all(MODELS_ROOT),
        "troops": trp_mod.build_all(MODELS_ROOT),
    }
    glbs: list[str] = []
    _collect_glb_paths(report, glbs)
    total_rows = _update_register(glbs)
    missing_tangents = []
    for sub in report.values():
        def _chk(node):
            if isinstance(node, dict) and "attrs" in node:
                if "TANGENT" not in node["attrs"] or "COLOR_0" not in node["attrs"]:
                    missing_tangents.append((node["path"], node["attrs"]))
            elif isinstance(node, dict):
                for v in node.values():
                    _chk(v)
        _chk(sub)
    print(f"M1_BLENDER_BUILD_OK generated={len(glbs)} registered_total={total_rows} missing_attrs={len(missing_tangents)}")
    if missing_tangents:
        print("MISSING_ATTRS:", json.dumps(missing_tangents, indent=2))
        sys.exit(1)


if __name__ == "__main__":
    main()
