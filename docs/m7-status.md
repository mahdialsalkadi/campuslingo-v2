# Milestone M7 Status — Mobile/PWA Delivery, Arabic RTL & Piper CLAUSE_CONCAT TTS, Aggregated Quiet-Hours Notifications, Operational Hardening & 30-Player Load Benchmark

Status: **COMPLETE AND VERIFIED**

## Implemented Scope

### Phase A — Notifications, Quiet Hours, Least-Privilege Grants, Retention & Restore Drill (`crates/domain`, `crates/storage`, `crates/server`, `scripts/`)
- **Domain Rules & WASM Parity (`crates/domain/src/notifications.rs`, `crates/domain/src/contracts.rs`, `crates/domain/src/lib.rs`)**:
  - Implemented notification preference/opt-out validation, user-local quiet-hours evaluation (`22:00`–`08:00` deferral to `08:00` local), daily notification cap enforcement (`MAX_PUSH_NOTIFICATIONS_PER_DAY = 2`: `1` aggregated inbound raid alert `"Raided N times today"` enumerating all raid IDs so no 48h revenge window is missed, plus `1` pre-expiry shield digest), and deduplication.
  - Added `crates/domain/tests/m7_notifications_and_rtl.rs` (`3` unit tests) and `packages/engine/tests/m7_parity.test.mjs` (`2` native/WASM parity tests covering quiet hours `22:00`–`08:00` deferral and aggregated notification planning).
- **PostgreSQL 16 Schema, Grants & Storage (`crates/storage/migrations/0008_m7_notifications_and_hardening.sql`, `scripts/m7_grants.sql`, `crates/storage/src/m7.rs`)**:
  - Added `notification_devices`, `notification_preferences`, and `notification_outbox` with least-privilege runtime grants in `scripts/m7_grants.sql`.
  - Enforced user-local quiet hours (`22:00`–`08:00` deferred to `08:00` local), opt-outs, deduplication, and `MAX_PUSH_NOTIFICATIONS_PER_DAY = 2` (`"Raided N times today"` + shield digest).
  - Implemented account pseudonymization (scrubbing user PII while keeping `review_logs`, `reward_transactions`, and `matches` immutable) and versioned content retention archiving.
- **Server Hardening & Endpoints (`crates/server/src/auth.rs`, `crates/server/src/m7.rs`, `crates/server/src/lib.rs`, `crates/server/src/routes.rs`)**:
  - Added `ActorRateLimiter` (`A-06` per-actor request throttling), `security_headers` middleware (preserving explicit route `Cache-Control: private, no-store` while defaulting `/v1/*` API responses to `no-store`), separated pool sizing (`8/16/8` with `5s` acquire timeout), and `/healthz` milestone reporting (`"stage": "M1"`, `"milestone": "M7"`).
  - Exposed `/v1/notifications/*`, `/v1/courses/{course_id}/notifications/shield-digest`, `/v1/me/pseudonymize`, and `/v1/courses/{course_id}/retention/sweep`, plus automated backup/restore verification via `pg_dump` / `psql` in `crates/server/tests/m1.rs` (`m7_a06_actor_rate_limiter_and_security_headers`, `m7_notifications_quiet_hours_daily_cap_and_opt_out`, and `m7_pseudonymization_retention_and_pg_dump_restore_drill`).

### Phase B — Arabic CLAUSE_CONCAT TTS, RTL/Bidi Mixed Math UI, PWA & Mobile Scaffold (`crates/ingestion`, `packages/game-client`, `apps/mobile`)
- **Arabic CLAUSE_CONCAT TTS & P-01/P-02 Gates (`crates/ingestion/src/tts.rs`)**:
  - Integrated `Language::Arabic` via isolated Rust `ort` / Piper `ar_JO-kareem-medium` (`63,201,294` bytes, SHA-256 `9e95cab07b679da603bba17c4dec7ab3111320571964ee95c0379603c086491e`, revision `cf910fb59f1d79ab3b625a63993a108751c6c25d`; config `ar_JO-kareem-medium.onnx.json`, `5,024` bytes, SHA-256 `ea6d9b9d9076dbdb6bf5c98c6a141ef154959d2359709b37855727964e7d6c4d`) under exclusive `ModelLease`.
  - Configured `PIPER_AR_SAMPLE_RATE_HZ = 22_050` and `PIPER_AR_GAP_SAMPLES = 1_323` (`alignment_method = "CLAUSE_CONCAT"`), silence trimming, exact sample-count timings, non-mutating `P-02` Tashkeel diacritization pre-pass (`diacritize_arabic_for_tts_p02`), and `P-01` normalized WER/technical-token gate (`P01_MAX_NORMALIZED_WER = 0.12`, marking fallbacks explicitly degraded in `AudioManifest`).
- **Arabic RTL, Mixed Math Bidi, Installable PWA & Mobile Scaffold (`packages/game-client`, `apps/mobile`)**:
  - Added `packages/game-client/src/m7_pwa_rtl.ts` and wired it into `packages/game-client/src/shell.ts`, `packages/game-client/public/index.html`, and `packages/game-client/public/m4-shell.css` for `dir="rtl"` / `dir="auto"` overlays with `<bdi class="m7-math-ltr" dir="ltr">` mixed-math bidi isolation and `viewport-fit=cover` safe-area layout.
  - Added installable iOS Safari PWA deliverables (`packages/game-client/public/manifest.json`, `packages/game-client/public/icons/icon-192.svg`, `packages/game-client/public/icons/icon-512.svg`, `packages/game-client/public/sw.js`) caching only permitted app/content assets and blocking offline ranked outcome queuing (`m7PwaRtl` + `packages/game-client/tests/game_client.test.ts`), alongside Capacitor scaffold files under `apps/mobile/` (`package.json`, `capacitor.config.json`, `README.md`).

### Phase C — Operational Audits (`O-04`, `O-08`, `A-06`) & 30-Player / 15-Live-Duel Load Benchmark (`O-06`)
- **`O-04` CPU vs. Vulkan Comparison (`scripts/audit_m7_o04_o08.mjs`, `artifacts/benchmarks/m7-o04-cpu-vs-vulkan.json`)**:
  - Verdict: `PASSED_SINGLE_MODEL_RESIDENT_WITH_VULKAN0_OFFLOAD_AND_CPU_ONNX_TTS`.
  - Records host budget (`12,242,141,184` B RAM, `2,147,483,648` B reserved headroom, `52,428,800` B online budget, `8,589,934,592` B containment limit, `0` B swap budget), OCR CPU fallback vs. `vulkan0` offload (`3.25x` host RSS reduction from `1,834,303,488` B to `564,310,016` B), `llm_qwen35_4b` (`pass1_ms: 2282`, `pass2_ms: 22327`, `peak_rss_bytes: 422,637,568`), `tts_english_kokoro_82m` (`peak_rss_bytes: 570,425,344`), and `tts_arabic_piper_ar_jo_kareem` (`synthesis_wall_ms: 2740`, `peak_rss_bytes: 226,492,416`, `cgroup_peak_bytes: 755,708,723`).
- **`O-08` Lockfile & Artifact Hash Audit (`artifacts/benchmarks/m7-o08-version-pins.json`)**:
  - Status: `PASSED_LOCKED_AND_HASH_PINNED` (`Cargo.lock`, `package-lock.json`, migrations `0001`–`0008`, least-privilege grants `m1`–`m7`, and pinned Piper AR model + config SHA-256 digests).
- **`O-06` 30-Player / 15-Live-Duel Benchmark (`tests/load/m7_30_player_15_duels_benchmark.mjs`, `artifacts/benchmarks/m7-o06-30p-15duels-load.json`)**:
  - Status: `PASSED_O06_30P_15DUELS_STANDALONE_AND_CONCURRENT`.
  - Run 1 (standalone `30` players / `15` simultaneous live duels): `15/15` settled, `0` errors, duration `633.26 ms`, `http_latency_ms` `p50=6.395 / p95=91.193 / p99=102.665`, `ws_heartbeat_latency_ms` `p50=2.66 / p95=3.921`, `ws_strike_roundtrip_ms` `p50=33.281 / p95=72.69`, `round_deadline_response_ms` `p50=41.376 / p95=42.564`, Rust server `idle_rss=5,877,760` B, `peak_rss=13,438,976` B (`< 50 MiB`).
  - Run 2 (`30` players / `15` simultaneous live duels concurrently with sequential background Piper AR ONNX TTS under `ModelLease`): `15/15` settled, `0` errors, duration `2389.67 ms`, `http_latency_ms` `p50=8.399 / p95=31.288`, `ws_heartbeat_latency_ms` `p50=1.138 / p95=1.32`, `ws_strike_roundtrip_ms` `p50=54.399 / p95=68.32`, `round_deadline_response_ms` `p50=41.205 / p95=42.532`, Rust server `peak_rss=14,135,296` B (`< 50 MiB`), background TTS `exit_code: 0` (`2373.3 ms`).
  - Post-wave HTTP `/readyz`: `200`; delegated cgroup (`MemoryMax=8G`, `MemorySwapMax=0`): `memory_peak_bytes: 504,180,736`, `memory_swap_current_bytes: 0`.

## Verification Results

All checks below ran after the final code edit with zero failures:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check && git diff --check` | Passed (exit `0`) |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed (exit `0`) |
| `cargo test --workspace --all-features --locked` | Passed — `campuslingo-domain`: `49 passed; 0 failed` (`3` lib + `9` adversarial + `9` domain + `2` g4_chains + `5` g6b_combat_and_scoring + `6` m6_raids_leagues_bosses_clans + `3` m7_notifications_and_rtl + `6` shields + `6` w_base_v1); `campuslingo-ingestion`: `66 passed; 0 failed; 16 ignored` (external binaries / delegated cgroups); `campuslingo-storage`: `1 passed; 0 failed`; `crates/server/tests/m1.rs`: `26 ignored` (run explicitly below) |
| `bash scripts/run_ingestion_contained.sh cargo test -p campuslingo-ingestion --features onnx --locked --lib contained_piper_ar_clause_concat_and_exclusive_model_lease -- --ignored --nocapture` | Passed — `1 passed; 0 failed` (`clauses=2, duration_ms=6988, opus_ms=6995, opus_bytes=28644, peak_rss_mb=218`, cgroup `Memory peak: 922.9M (swap: 0B)`) |
| `bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs` | Passed — `26 passed; 0 failed; 0 ignored; finished in 118.03s`, cgroup `Memory peak: 764M (swap: 0B)` (includes `m7_a06_actor_rate_limiter_and_security_headers`, `m7_notifications_quiet_hours_daily_cap_and_opt_out`, and `m7_pseudonymization_retention_and_pg_dump_restore_drill`) |
| `npm run m0:build && npm run contracts:check && npm run typecheck` | Passed (exit `0`) |
| `npm run test:ts` | Passed — `11 passed; 0 failed` |
| `npm run test:wasm` | Passed — `11 passed; 0 failed` |
| `node --test tools/eval/*.test.mjs` | Passed — `20 passed; 0 failed` |
| `node scripts/audit_m3_wasm_and_ui.mjs` | Passed — `parityMatches: 190` (`total: 190`), P-06 mobile viewport `360px` (`doc_scroll_width_css_px: 360`, `60` controls checked, min size `44x44` px) |
| `node scripts/audit_m4_game_shell.mjs` | Passed — `rendererBackend: "webgl"`, `sceneTriangles: 1762`, `budgetOk: true`, `controlCount: 54`, `violations: []` across `mobile-360`, `tablet-768`, `desktop-1280`; `m7PwaRtl` verified (`previewComputedDir: "rtl"`, `bdiComputedDir: "ltr"`, `bdiUnicodeBidi: "isolate"`, `manifestOk: true`, `swOk: true`) |
| `node scripts/audit_m7_o04_o08.mjs` | Passed — `o04_verdict: "PASSED_SINGLE_MODEL_RESIDENT_WITH_VULKAN0_OFFLOAD_AND_CPU_ONNX_TTS"`, `o08_status: "PASSED_LOCKED_AND_HASH_PINNED"`, `migrations: 8`, `grants: 7` |
| `systemd-run --user --wait --pipe --collect --property=Type=exec --property=Delegate=memory --property=DelegateSubgroup=supervisor --property=MemoryMax=8G --property=MemorySwapMax=0 --working-directory=/home/mahdi/campuslingo-v2 /bin/bash /home/mahdi/campuslingo-v2/scripts/run_ingestion_contained.sh --inside node /home/mahdi/campuslingo-v2/tests/load/m7_30_player_15_duels_benchmark.mjs` | Passed — `PASSED_O06_30P_15DUELS_STANDALONE_AND_CONCURRENT`, `15/15` standalone + `15/15` concurrent duels settled, `0` errors, `peak_rss` `13,438,976` B (Run 1) and `14,135,296` B (Run 2), cgroup `Memory peak: 480.8M (swap: 0B)` |
