#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Linux-Adapted Central Godot 4 CLI Wrapper for CampusLingo: Village Clash (`tools/godot.py`).
Enforces D006 (`MAX_SLOTS = 1`), offscreen quiet rendering (`xvfb-run` / `--audio-driver Dummy`),
and micro-section inspection (`sections` command).
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT
TOOLS_DIR = ROOT / "tools"
EVIDENCE_PATH = ROOT / "data" / "evidence.jsonl"
OWNER_FLAG = TOOLS_DIR / ".owner_using_pc"
IMPORT_LOCK = TOOLS_DIR / ".import_lock"
MAX_SLOTS = 1


def find_godot() -> str:
    env_bin = os.environ.get("GODOT_BIN", "").strip()
    if env_bin and Path(env_bin).exists():
        return env_bin
    candidates = [
        str(Path.home() / ".local" / "bin" / "godot"),
        str(Path.home() / ".local" / "bin" / "godot4"),
        str(Path.home() / ".local" / "share" / "godot" / "Godot_v4.3-stable_linux.x86_64"),
        "godot4",
        "godot",
    ]
    for c in candidates:
        resolved = shutil.which(c) or (c if Path(c).exists() else None)
        if resolved:
            return str(resolved)
    raise RuntimeError("Godot executable not found! Install Godot 4.x or set GODOT_BIN.")


class FileLock:
    def __init__(self, lock_file: Path, timeout: float = 300.0):
        self.lock_file = lock_file
        self.timeout = timeout

    def __enter__(self):
        start = time.monotonic()
        TOOLS_DIR.mkdir(parents=True, exist_ok=True)
        while True:
            try:
                fd = os.open(str(self.lock_file), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                os.write(fd, f"{os.getpid()}\n".encode("utf-8"))
                os.close(fd)
                return self
            except FileExistsError:
                try:
                    if time.time() - self.lock_file.stat().st_mtime > 600:
                        self.lock_file.unlink(missing_ok=True)
                        continue
                except OSError:
                    pass
                if time.monotonic() - start > self.timeout:
                    raise TimeoutError(f"Timed out waiting for lock {self.lock_file}")
                time.sleep(1.0)

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.lock_file.unlink(missing_ok=True)


class RunSlot:
    def __init__(self, timeout: float = 1800.0):
        self.timeout = timeout
        self.slot_path = None

    def __enter__(self):
        start = time.monotonic()
        TOOLS_DIR.mkdir(parents=True, exist_ok=True)
        while True:
            for idx in range(MAX_SLOTS):
                candidate = TOOLS_DIR / f".run_slot_{idx}"
                try:
                    fd = os.open(str(candidate), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                    os.write(fd, f"{os.getpid()}\n".encode("utf-8"))
                    os.close(fd)
                    self.slot_path = candidate
                    return self
                except FileExistsError:
                    try:
                        if time.time() - candidate.stat().st_mtime > 600:
                            candidate.unlink(missing_ok=True)
                    except OSError:
                        pass
            if time.monotonic() - start > self.timeout:
                raise TimeoutError("Timed out waiting for the single machine-wide Godot slot.")
            time.sleep(1.5)

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.slot_path:
            self.slot_path.unlink(missing_ok=True)


def append_evidence(row: dict) -> None:
    EVIDENCE_PATH.parent.mkdir(parents=True, exist_ok=True)
    with EVIDENCE_PATH.open("a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")


def run_godot(args_list: list[str], timeout: float = 300.0, use_xvfb: bool = False) -> subprocess.CompletedProcess:
    godot_bin = find_godot()
    cmd = [godot_bin, "--path", str(GAME_DIR)] + args_list
    if use_xvfb and shutil.which("xvfb-run"):
        cmd = ["xvfb-run", "-a", "-s", "-screen 0 1360x840x24"] + cmd
    env = os.environ.copy()
    env["CAMPUSLINGO_WRAPPER"] = "1"
    return subprocess.run(cmd, cwd=str(ROOT), env=env, text=True, capture_output=True, timeout=timeout)


def cmd_import(_args) -> None:
    with FileLock(IMPORT_LOCK):
        res = run_godot(["--headless", "--audio-driver", "Dummy", "--import", "--quit"], timeout=240)
        print(res.stdout)
        if res.stderr:
            print(res.stderr, file=sys.stderr)
        sys.exit(res.returncode)


def cmd_check(_args) -> None:
    with FileLock(IMPORT_LOCK):
        res_imp = run_godot(["--headless", "--audio-driver", "Dummy", "--import", "--quit"], timeout=240)
        if res_imp.returncode != 0:
            print(res_imp.stdout)
            print(res_imp.stderr, file=sys.stderr)
            sys.exit(res_imp.returncode)
    with RunSlot():
        res = run_godot(["--headless", "--audio-driver", "Dummy", "--quit-after", "30"], timeout=120)
        combined = (res.stdout or "") + "\n" + (res.stderr or "")
        # Ignore Godot 4.3 dummy-renderer benign engine error on PrimitiveMesh in --headless mode
        filtered_lines = []
        lines = combined.splitlines()
        i = 0
        while i < len(lines):
            ln = lines[i]
            nxt = lines[i + 1] if i + 1 < len(lines) else ""
            if 'Parameter "m" is null.' in ln and "dummy/storage/mesh_storage.h:120" in nxt:
                i += 2
                continue
            filtered_lines.append(ln)
            i += 1
        filtered = "\n".join(filtered_lines)
        print(res.stdout)
        if filtered.strip():
            for ln in res.stderr.splitlines():
                if 'Parameter "m" is null.' not in ln and "mesh_storage.h:120" not in ln:
                    print(ln, file=sys.stderr)
        has_err = (
            res.returncode != 0
            or "SCRIPT ERROR" in filtered
            or "Parse Error" in filtered
            or "ERROR:" in filtered
        )
        if has_err:
            sys.exit(1)
        print("[godot.py check] OK — 0 script/runtime errors.")


def cmd_sections(args) -> None:
    gd_code = """extends SceneTree
func _initialize():
    var reg := get_root().get_node_or_null("SectionRegistry")
    var created_temp := false
    if reg == null:
        var reg_script := load("res://scripts/core/section_registry.gd")
        if reg_script != null:
            reg = reg_script.new()
            created_temp = true
    if reg == null:
        printerr("Missing SectionRegistry")
        quit(1)
        return
    var target := "%s"
    if target != "":
        var item: Dictionary = reg.get_section(target)
        if item.is_empty():
            printerr("Unknown section ID: " + target)
            if created_temp:
                reg.free()
            quit(1)
            return
        if bool(item.get("exists", false)):
            var loaded := ResourceLoader.load("res://" + String(item["path"]))
            item["script_load_ok"] = (loaded != null)
        print(JSON.stringify(item))
    else:
        var summary: Dictionary = reg.get_summary_counts()
        print("SUMMARY=", JSON.stringify(summary))
        for item in reg.get_all_sections():
            print(JSON.stringify(item))
    if created_temp:
        reg.free()
    quit(0)
""" % (args.id.upper() if args.id else "")
    with tempfile.NamedTemporaryFile("w", suffix=".gd", delete=False) as tf:
        tf.write(gd_code)
        temp_path = Path(tf.name)
    try:
        with RunSlot():
            res = run_godot(["--headless", "--audio-driver", "Dummy", "-s", str(temp_path)], timeout=60)
            combined = (res.stdout or "") + "\n" + (res.stderr or "")
            if res.returncode != 0 or "SCRIPT ERROR" in combined or "Parse Error" in combined:
                print(res.stdout)
                print(res.stderr, file=sys.stderr)
                sys.exit(1)
            for line in res.stdout.splitlines():
                if line.startswith("SUMMARY=") or line.startswith("{"):
                    print(line)
    finally:
        temp_path.unlink(missing_ok=True)


def cmd_capture(args) -> None:
    out_path = (ROOT / args.out).resolve()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    dev_args = ["--", f"--capture={out_path.as_posix()}", f"--wait-frames={args.wait}"]
    if args.cam:
        dev_args.append(f"--cam={args.cam}")
    if args.hide_hud:
        dev_args.append("--hide-hud")
    if args.section:
        dev_args.append(f"--section={args.section}")
    if args.extra:
        dev_args.extend(args.extra)

    with RunSlot():
        res = run_godot(["--audio-driver", "Dummy", args.scene] + dev_args, timeout=180, use_xvfb=True)
        if not out_path.exists():
            print(res.stdout)
            print(res.stderr, file=sys.stderr)
            sys.exit(1)
        sha = hashlib.sha256(out_path.read_bytes()).hexdigest()
        append_evidence({
            "utc": datetime.now(timezone.utc).isoformat(),
            "feature": out_path.parent.name,
            "capture_type": "native_viewport_xvfb",
            "scene": args.scene,
            "camera": args.cam or "default",
            "file": out_path.relative_to(ROOT).as_posix() if out_path.is_relative_to(ROOT) else out_path.as_posix(),
            "sha256": sha,
        })
        print(f"[godot.py capture] Saved {out_path} ({out_path.stat().st_size} bytes, sha256={sha[:12]}...)")


def cmd_movie(args) -> None:
    out_target = (ROOT / args.out).resolve()
    out_avi = out_target.with_suffix(".avi")
    out_target.parent.mkdir(parents=True, exist_ok=True)
    fps = getattr(args, "fps", 30)
    engine_args = [
        "--audio-driver", "Dummy",
        args.scene,
        "--write-movie", out_avi.as_posix(),
        "--fixed-fps", str(fps),
        "--quit-after", str(args.frames),
    ]
    dev_extra = list(args.extra or [])
    if getattr(args, "cam", ""):
        dev_extra.append(f"--cam={args.cam}")
    if dev_extra:
        engine_args.extend(["--"] + dev_extra)
    with RunSlot():
        res = run_godot(engine_args, timeout=300, use_xvfb=True)
        if not out_avi.exists():
            print(res.stdout)
            print(res.stderr, file=sys.stderr)
            sys.exit(1)
    out_mp4 = out_target if out_target.suffix == ".mp4" else out_avi.with_suffix(".mp4")
    if shutil.which("ffmpeg"):
        subprocess.run([
            "ffmpeg", "-y", "-i", str(out_avi),
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20",
            "-an", str(out_mp4)
        ], check=True, capture_output=True)
        out_avi.unlink(missing_ok=True)
        final_file = out_mp4
    else:
        final_file = out_avi
    sha = hashlib.sha256(final_file.read_bytes()).hexdigest()
    append_evidence({
        "utc": datetime.now(timezone.utc).isoformat(),
        "feature": final_file.parent.name,
        "capture_type": "movie_maker",
        "scene": args.scene,
        "frames": args.frames,
        "file": final_file.relative_to(ROOT).as_posix() if final_file.is_relative_to(ROOT) else final_file.as_posix(),
        "sha256": sha,
    })
    print(f"[godot.py movie] Saved {final_file} ({final_file.stat().st_size} bytes, {args.frames} frames)")


def cmd_run(args) -> None:
    frames = int(args.seconds * 60)
    engine_args = ["--audio-driver", "Dummy", args.scene, "--quit-after", str(frames)]
    if args.headless:
        engine_args.insert(0, "--headless")
    if args.extra:
        engine_args.extend(["--"] + args.extra)
    with RunSlot():
        res = run_godot(engine_args, timeout=max(60.0, args.seconds * 4), use_xvfb=not args.headless)
        print(res.stdout)
        if res.stderr:
            print(res.stderr, file=sys.stderr)
        sys.exit(res.returncode)


def cmd_script(args) -> None:
    extra = ["--"] + args.extra if args.extra else []
    with RunSlot():
        res = run_godot(["--headless", "--audio-driver", "Dummy", "-s", args.script] + extra, timeout=180)
        print(res.stdout)
        if res.stderr:
            print(res.stderr, file=sys.stderr)
        sys.exit(res.returncode)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("import", help="Import project assets with lock")
    sub.add_parser("check", help="Import and run 30 headless frames expecting 0 errors")

    p_sec = sub.add_parser("sections", help="List or verify registered 40 SEC-* micro-sections")
    p_sec.add_argument("--id", default="", help="Optional single SEC-* ID to query")

    p_cap = sub.add_parser("capture", help="Capture native viewport PNG from a scene")
    p_cap.add_argument("scene")
    p_cap.add_argument("out")
    p_cap.add_argument("--cam", default="")
    p_cap.add_argument("--wait", type=int, default=20)
    p_cap.add_argument("--hide-hud", action="store_true")
    p_cap.add_argument("--section", default="")

    p_mov = sub.add_parser("movie", help="Record Movie Maker clip and convert to MP4")
    p_mov.add_argument("scene")
    p_mov.add_argument("out")
    p_mov.add_argument("--frames", type=int, default=120)
    p_mov.add_argument("--fps", type=int, default=30)
    p_mov.add_argument("--cam", default="")

    p_run = sub.add_parser("run", help="Run a scene for N seconds")
    p_run.add_argument("scene")
    p_run.add_argument("--seconds", type=float, default=10.0)
    p_run.add_argument("--headless", action="store_true")

    p_scr = sub.add_parser("script", help="Run a SceneTree GDScript tool")
    p_scr.add_argument("script")

    args, unknown = p.parse_known_args()
    if unknown and unknown[0] == "--":
        unknown = unknown[1:]
    args.extra = unknown
    dispatch = {
        "import": cmd_import,
        "check": cmd_check,
        "sections": cmd_sections,
        "capture": cmd_capture,
        "movie": cmd_movie,
        "run": cmd_run,
        "script": cmd_script,
    }
    dispatch[args.cmd](args)


if __name__ == "__main__":
    main()
