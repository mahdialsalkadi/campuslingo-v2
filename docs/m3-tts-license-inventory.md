# M3 English TTS license inventory (local observation)

Date: 2026-11-13. **Status: PARTIAL (runtime verified; distribution sign-off OPEN).**
This inventory records the locally observed M3 English TTS stack, pinned artifact
digests, dynamic library bindings, and declared package licenses. It does not
grant redistribution rights and does not replace human legal sign-off.

## Native & dynamic runtime stack

Versions, digests, and declared licenses were read from the local package
database and on-disk binaries without downloading any packages or weights.

| Component | Observed version / path | SHA-256 / Declared license | Integration boundary |
| --- | --- | --- | --- |
| `onnxruntime-cpu` | `1.29.0-3` (`/usr/lib/libonnxruntime.so.1.29.0`) | `f3f934e1...24945eb` / `MIT` (`pacman -Qi`) | Dynamically loaded at runtime via `ort =2.0.0-rc.15` (`load-dynamic`) behind optional Cargo feature `onnx`; not statically linked |
| `libespeak-ng` | `1.52.0` (`espeakng-loader` `0.2.4` cached `.so`: `libespeak-ng.so.1.52.0`) | `b15ce080...efaf2c3ccf` / `espeakng-loader` `METADATA` declares no license classifier (note: upstream eSpeak NG is GPLv3; distribution review REQUIRED if shipped) | Dynamically opened at runtime via POSIX `libc::dlopen`/`dlsym` behind optional `onnx` feature under `ESPEAK_MUTEX`; not compiled or linked into binaries, and never stored in git |
| `ffmpeg` / `ffprobe` | `2:9.0.2-1.1` (`/usr/bin/ffmpeg`, `/usr/bin/ffprobe`) | `GPL-3.0-only` (`pacman -Qi`) | Spawned out-of-process inside delegated cgroup-v2 helper (`crate::process::run_helper`) for `f32le` -> `libopus` encoding and header/duration probing |
| `opus` (`libopus`) | `1.6.1-1.1` | `BSD-3-Clause` (`pacman -Qi`) | Used by `ffmpeg` out-of-process to encode `32 kbps` mono VoIP `.opus` audio |

## Pinned English TTS artifacts (local private directory only)

All weights and voice tensors reside exclusively under `artifacts/private/models/kokoro-82m/` (never committed or uploaded).

| Artifact | Pinned provenance | Size (bytes) | Observed SHA-256 | Declared card/repo license |
| --- | --- | --- | --- | --- |
| `model.onnx` | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe` | `325532346` | `45e6d0d8fd257386edf18b02c64934777ec2017ebb35d7d7e29ea138cb8de9a0` | `Apache-2.0` (model card; upstream `hexgrad/Kokoro-82M`) |
| `af_heart.bin` | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe` | `522240` | `d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b` | `Apache-2.0` card claim; per-voice training audio notice requires separate distribution check |
| `tokenizer.json` | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe` | `3497` | `77a02c8e164413299b4b4c403b14f8e0e1c1b727db4d46a09d6327b861060a34` | `Apache-2.0` card claim |
| `tokenizer_config.json` | `shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped@02ff6fe` | `113` | `be1cb066d6ef6b074b3f15e6a6dd21ac88ff3cdaedf325f0aaed686c70f75d20` | `Apache-2.0` card claim |

## Scope & Open Human Items

- **Observed / Verified:** English `Kokoro-82M` `CLAUSE_CONCAT` (`P-07`) synthesis, trimmed silence removal, exact `24 kHz` sample-derived timestamps, `DirectorTrack` synchronization, `.opus` encoding via `ffmpeg` (`libopus`), and strict exclusive `ModelLease` single-model residency.
- **Deferred (Post-M3 / Out of Scope):** Arabic TTS (`piper-voices` / `ar_JO-kareem-medium`). `Language::Arabic` audio synthesis is not implemented or claimed in this M3 build.
- **Open Redistribution Sign-Off:** Human legal review of (1) GPLv3 `libespeak-ng` runtime packaging/replacement strategy before client/server distribution, (2) `ffmpeg` GPL-3.0-only out-of-process deployment requirements, and (3) `af_heart.bin` voice provenance & upstream ONNX notices.
