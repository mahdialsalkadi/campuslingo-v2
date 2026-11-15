#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Disk & Memory Watchdog (`tools/resource_watchdog.py`).
Monitors free disk space on the project drive every 10 seconds.
If free space drops below 8 GB (e.g. from runaway Windows pagefile.sys or uncompressed AVI dumps),
it cleans stale scratch media and logs an alert so the machine never crashes.
"""

import argparse
import shutil
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--floor-gb", type=float, default=8.0)
    p.add_argument("--max-hours", type=float, default=12.0)
    args = p.parse_args()

    deadline = time.monotonic() + args.max_hours * 3600
    floor_bytes = int(args.floor_gb * (1024 ** 3))

    while time.monotonic() < deadline:
        usage = shutil.disk_usage(ROOT)
        if usage.free < floor_bytes:
            # Purge leftover uncompressed .avi files that already have .mp4 counterparts
            for avi in (ROOT / "Recordings").rglob("*.avi"):
                if avi.with_suffix(".mp4").exists():
                    avi.unlink(missing_ok=True)
            print(
                f"[{datetime.now(timezone.utc).isoformat()}] WARNING: Free disk {usage.free / 1e9:.2f} GB < {args.floor_gb} GB floor!",
                flush=True,
            )
        time.sleep(10.0)


if __name__ == "__main__":
    main()
