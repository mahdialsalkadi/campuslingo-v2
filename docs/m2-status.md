# M2 implementation evidence

Date: 2026-11-13. **M2 remains IN PROGRESS.** Authority is the frozen
specification in `/home/mahdi/Downloads/plan`; this is code/test evidence, not
a replacement plan. M0/M1 historical status reports are unchanged.

**Current follow-up:** vector/raster/math/text actions, rotated CropBox
normalization, full-deck template exclusion, predicate-preserving consolidation,
150-DPI overlays/masks and 250-DPI source crops are implemented. The managed
Vulkan0 OCR-enabled full private run (`artifacts/private/m2-full-deck-1794541547719/index-rechecked.json`)
processed **3 PDFs / 200 pages / 511 crops** with **0 missing-OCR failures**,
**0 no-chunks failures**, **0 bad crops**, and **22 clean model releases** (`0`
OOM kills), retaining 1 explicit `unsupported_vector_group` warning; see
[region evidence](m2-region-evidence.md). No digital-text line crops remain.
Fenced PostgreSQL checkpoints, private crop delivery, managed sequential `Qwen3-Embedding-0.6B-Q8_0.gguf` (`S-02`–`S-04` / `P-04`) chunk embeddings under `ModelLease`, and a contained whole-stack `O-01` measurement (`artifacts/private/o01-1794542035999/o01-measurement.json`)
are recorded. **G-1 stays OPEN**: local visual annotation/OCR-fidelity review on
the new 14-sample OCR packet and user sign-off on
[m2-license-signoff-checklist.md](m2-license-signoff-checklist.md) remain incomplete.

## Delivered in this slice: PDF/crop helper containment

- `crates/ingestion/src/process.rs` is the shared runner for `pdfinfo`,
  `pdftohtml`, `pdftotext`, `pdfimages`, `pdftoppm`, `pdftocairo`, `rsvg-convert`,
  Tesseract geometry and ImageMagick crops.
- Every helper gets a new process group, a private cgroup-v2 memory limit,
  swap disabled, whole-cgroup OOM termination, CPU/file-size/core/address-space
  bounds, and a wall timeout. Cgroup attachment and rlimits happen **before
  exec**, using only allocation-free, async-signal-safe operations in `pre_exec`.
- stdout/stderr drain concurrently with separate `MAX_BYTES + 1` readers.
  Either stream overflowing aborts with `OutputOverflow`; no silent truncation.
- Normal exit, timeout, errors and future cancellation kill remaining
  descendants, including processes that escaped the process group via `setsid`.
  Direct children are awaited/reaped. Completed cleanup requires no remaining
  cgroup tasks and removal of the helper's accounting directory. Captured output
  and source paths are not placed in runner errors or production logs.
- Cgroup peak memory and kernel OOM-kill counts are available as helper usage
  evidence. They include charged page cache and are **not** labeled process RSS.
- The raster-only ImageMagick policy caps memory/map/disk, image dimensions,
  threads, duration and list length. All delegates, filters and unapproved
  coders are disabled; PNG read and WebP write are allowed. No Ghostscript
  delegate is permitted. Any future direct Ghostscript path still needs `-dSAFER`.
- Failed `pdfimages` inspection propagates an error instead of silently
  becoming zero images. The current pipeline version is
  `m2-pipeline-v4-vector-consolidation`, invalidating earlier page artifacts.

### Helper configuration (internal operational budgets)

| Budget | Value |
| --- | --- |
| Resident memory / swap | 512 MiB per helper tree / 0 |
| CPU / file size / address-space backstop / core dump | 60 CPU seconds / 64 MiB per file / 8 GiB / 0 |
| Wall timeout: inspection / XML+image list / raster / OCR geometry+crop | 15s / 30s / 60s / 45s |
| Per-stream output: inspection+raster+crop / XML+image list+geometry | 64 KiB / 8 MiB |
| Cleanup wait | 5s for child wait; 5s for empty cgroup verification |

These helper-specific limits are not model-runtime estimates, whole-host
admission evidence, or changes to frozen learning/combat constants. Real-corpus
measurement may identify legitimate inputs that need a different operational
budget; such failures must be recorded, not bypassed.

## Managed local OCR lifecycle (`DEC-017`)

- `crates/ingestion/src/process.rs` now has a resident-worker controller for
  long-lived local models. The child, its cgroup and the model lease are owned by
  a supervisor task, not by the request future, so cancellation still kills and
  reaps the whole tree before the lease can be released.
- The managed OCR server binds a random literal-loopback port and requires a
  per-run bearer token, including on its health check. It disables proxies and
  redirects, writes model logs/evidence with owner-only permissions (`0700`/
  `0600`), disables runtime prompt/RAM caches (`--cache-ram 0 --no-cache-prompt
  --no-cache-idle-slots --ctx-checkpoints 0`), and supports `--ocr-device
  auto|none|<device>`: by default it probes `llama-server --list-devices` for
  a Vulkan GPU (`Vulkan0`) and enables `--gpu-layers all --fit off --flash-attn on
  --parallel 2 --ctx-size 8192` with an isolated `MESA_SHADER_CACHE_DIR` under
  the model work dir (`0700`), falling back cleanly to CPU (`--device none
  --gpu-layers 0 --no-op-offload --no-mmproj-offload --parallel 1`) when
  requested or unavailable.
- `crates/ingestion/src/pipeline.rs` bounds host parallelism via
  `max_concurrency` (`CAMPUSLINGO_PIPELINE_CONCURRENCY`, default `clamped
  available_parallelism` up to `12`), running page layout extraction, vector
  preparation, independent page processing, `OcrFullPageScan` (`tokio::join!`
  over GPU OCR and Tesseract geometry), and per-page region crops concurrently
  while gating `--parallel 2` requests cleanly against periodic 16-request
  server restarts via `Semaphore` + `RwLock`.
- The worker watchdog checks both the model RSS and host headroom. A 2 GiB
  cgroup/RSS budget (`DEC-017`) replaced the earlier 1 GiB implementation cap
  after the hash-pinned Q8_0 model plus BF16 projector could not fit on CPU.
  With `Vulkan0` offload active on the BC-250 APU, model host RSS remains
  ~`538 MiB`–`564.3 MB` (`cgroup_memory_peak_bytes` max `311,222,272`) while GPU
  allocations reside in the 4 GiB DRM VRAM carveout (`~2.45 GiB` peak used).
  The 3 GiB host reserve and the frozen 1.5 GiB `O-01` admission floor are
  unchanged.
- Stop evidence records the process-tree reap, cgroup removal, cgroup memory peak,
  process peak RSS (`VmHWM`), minimum host available bytes, OOM-kill count and
  termination reason. A failed release fails the extraction instead of silently
  freeing the lease.
- The model lease is also checked against a project-global lock, so a second
  output directory cannot bypass single-model residency. Lease files are opened
  without following symlinks and forced to mode `0600`.

CPU-only baseline (`--ocr-device none`, synthetic 4 pages at 4 threads): `68.4 s`,
`1,834,303,488 bytes` peak model RSS and `2,147,524,608 bytes` charged cgroup
peak (vs ~`80 s/page` and `1.932 GiB` RSS on full-page scans). Switching to
`Vulkan0` + bounded pipeline concurrency processed all **200 private corpus
pages** (19 restarts / 22 clean releases, `0` OOM kills, max process peak RSS
`564,310,016 bytes`, max cgroup peak `311,222,272 bytes`, min host available
`8,577,298,432 bytes`): Diagram/math `142.5 s`, Digital text `66.3 s`,
Low-text/image `145.6 s`. Contained whole-stack `O-01` measurement on 4 synthetic
pages (`artifacts/private/o01-1794542035999/o01-measurement.json`) recorded
`MEASURED_SCOPED`, `cgroup_peak_absolute_bytes: 321,748,992` (unit peak `918M`,
`0B` swap), `minimum_mem_available_bytes: 9,602,314,240`, DRM VRAM peak
`2,647,830,528` of `4,294,967,296 bytes`, DRM GTT peak `252,223,488 bytes`, and
`safety_reserve_respected: true` (`Redis` not present in M2). The public model
identities and rejected local look-alikes are recorded in
`artifacts/private/model-audit-m2-20261113.json`; the license boundary remains
PARTIAL pending user sign-off. This is not private-corpus OCR-fidelity acceptance.

## Reproduction and measured results

```sh
bash scripts/verify_m2_containment.sh
cargo test --workspace --locked
bash scripts/run_ingestion_contained.sh node scripts/run_m1_db_tests.mjs
node --test tools/eval/*.test.mjs
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
git diff --check
cargo build --release -p campuslingo-ingestion --locked
bash scripts/run_ingestion_contained.sh node tools/eval/full_deck_probe.mjs --managed-ocr --model artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6.Q8_0.gguf --mmproj artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6-mmproj.gguf --threads 8
bash scripts/run_ingestion_contained.sh node scripts/measure_o01.mjs
```

| Check | Result |
| --- | --- |
| Ingestion library default suite | PASS: 52 passed (plus 2 bin unit tests = 54); 11 environment-dependent tests run separately |
| Delegated cgroup + local tools suite (`verify_m2_containment.sh`) | PASS: 52 passed (`11` ignored) + 11 passed (`52` filtered), `14.283s`, peak `571.9M` (`swap: 0B`); real Poppler, Tesseract, PNG-to-WebP crop, contained Vulkan0/OCR tests, vector masks, rotations, kernel OOM kill, timeout/cancellation reaping |
| Workspace native suite (`cargo test --workspace --locked`) | PASS: 80 passed; 21 ignored (11 contained ingestion and 10 PostgreSQL tests run separately) |
| Disposable PostgreSQL + pgvector suite (`run_m1_db_tests.mjs`) | PASS: 10 passed, 0 failed (`34.89s`, service peak `582.7M`, `swap: 0B`), including M2 worker ingestion & scoped crop delivery |
| Full private-deck OCR-enabled verifier (`artifacts/private/m2-full-deck-1794541547719/index-rechecked.json`) | PASS (structural + leased `Vulkan0` OCR): 3 PDFs, 200 pages, 511 valid crops, 200 overlays/masks, 0 missing-OCR failures, 0 no-chunks failures, 22 clean model releases (`0` OOM kills); 1 explicit `unsupported_vector_group` remains |
| Contained whole-stack `O-01` (`artifacts/private/o01-1794542035999/o01-measurement.json`) | PASS (`MEASURED_SCOPED`): cgroup peak `321,748,992` bytes (unit peak `918M`, `0B` swap), min host avail `9,602,314,240`, DRM VRAM peak `2,647,830,528` / `4,294,967,296`, `Redis` absent (excluded in M2) |
| JavaScript WebP/artifact/annotation verifier regressions | PASS: 20 passed, 0 failed |
| Workspace clippy (`-D warnings`), `cargo fmt`, `git diff --check`, `--release` ingestion build | PASS |

Current sequential validation logs: `artifacts/private/m2-validation-20261112T130723Z/`.
Its `index.json` records all actual/expected exit codes (including deliberately
blocked annotation readiness), the unchanged frozen-master hash and zero
permission mismatches across the full-deck/review tree.

The harness starts a **disposable systemd user service**, delegates the memory
controller, and keeps the supervisor in its own subgroup. It enables controllers
only inside that temporary delegation. It installs no persistent unit, modifies
no existing service, starts no llama-server/ONNX model worker and touches no
private lecture corpus. Tesseract may run for synthetic geometry checks.
The 32 MiB OOM test deliberately kills only its own bounded synthetic helper.

The latest disposable DB suite's service peak was 931.5 MiB, with 11.9 MiB
reported swap across the service. This is not a claim that every helper/model
or the whole stack satisfies a no-swap/admission gate. Helper cgroups separately
disable swap; no authoritative OCR/model inference was performed.

An initial ImageMagick test failed because list-length 1 also blocks its
temporary image clones; list-length 4 was verified with a real crop. This is
not a corpus acceptance result or a relaxation of frozen artifact budgets.

## Running extraction outside the harness

Set `CAMPUSLINGO_HELPER_CGROUP_ROOT` to a dedicated empty cgroup-v2 parent with
the memory controller enabled and writable child controls. The worker never
enables controllers in an arbitrary existing service or moves the API process.
Missing/invalid delegation fails **before launching a helper**. There is no
production bypass or fallback to `RLIMIT_AS` alone.

Install `crates/ingestion/policy/policy.xml` at the configured compile-time
location along with the worker; missing or changed policy refuses crop encoding.
The verification script needs systemd supporting `DelegateSubgroup`; other
service managers must supply equivalent explicit delegation. Keep service-level
whole-control-group termination enabled: process death/SIGKILL cannot run Rust
destructors, and runtime teardown cannot promise completion of async cleanup.

## Durable M2 persistence and authorization

- Migration `0003_ingestion.sql` adds scoped ingestion runs, page checkpoints,
  crop metadata, chunks and associations, RLS, composite foreign keys and a
  1024-dimensional pgvector/HNSW schema. Existing migrations are not rewritten.
  The disposable PostgreSQL harness provisions pgvector and explicit worker/API
  grants; this is not a production database deployment.
- `crates/storage/src/ingestion.rs` fences writes with the active durable job
  lease, uses transactional page checkpoints and canonical digests, rejects
  altered provenance/replays, and resumes the same run after lease reclaim.
- `crates/ingestion/src/worker.rs` verifies source/crop identities, persists
  fsynced immutable crop bytes before metadata commits, and binds executable
  hashes to pipeline provenance, including `pdftocairo` and `rsvg-convert`.
- Run/crop routes recheck sessions, course membership and private-derivative
  publication boundaries. Managed local embeddings (`crates/ingestion/src/embeddings.rs`
  with hash-pinned `Qwen3-Embedding-0.6B-Q8_0.gguf` sharing `crates/ingestion/src/llama.rs`
  under single-model `ModelLease`) now run sequentially in `worker.rs` only after
  the OCR server stops and its cgroup/process release is verified.
- Scoped PostgreSQL ANN retrieval (`search_similar_chunks` and
  `search_similar_chunks_for_worker`), `S-02` 1024-d L2/provenance validation,
  `S-03` verbatim/normalized chunk preservation, `S-04` quarantine/tenant/course/language/version
  exclusions + HNSW partial-index plans (`ingestion_embedding_hnsw`), and `P-04`
  structural-fingerprint dedup guards (`safe_dedup_candidate`) are implemented
  and verified across twelve M1/M2 PG16 integration tests and contained
  `Qwen3-Embedding-0.6B-Q8_0.gguf` tests.
- This delivers the `O-02` checkpoint/persistence and `S-02`/`S-03`/`S-04`/`P-04`
  engineering slice, not a blanket closure: production SIGKILL recovery tests
  and measured private-corpus retrieval recall remain open alongside user review.

## Gate boundary and next work

**PASS (scoped):** helper tests, contained Vulkan0 OCR lifecycle tests,
structural + OCR-enabled private artifact checks, contained `O-01` host/VRAM
measurement, and durable DB/API checkpoint/authorization tests above. These
do not close visual/pedagogical content acceptance without human annotation.

**G-1 OPEN:** vector masks, clustering, template exclusion, consolidation, and
authoritative `Vulkan0` PaddleOCR-VL extraction now run end-to-end across all
200 pages with **0 missing-OCR failures** and **0 no-chunks failures**
(retaining 1 explicit `unsupported_vector_group` on the digital-text cover). A
new 14-sample OCR-enabled host-local review packet is prepared at
`artifacts/private/m2-full-deck-1794541547719/review-1794542002239/`
(`NOT_REVIEWED`; `score_region_review.mjs` returns `BLOCKED_NO_REVIEWED_REFERENCES`
exit `2`). Visual annotation, OCR fidelity verification, and assessment of the 1
explicit unsupported vector group remain open for user review; see
[region evidence](m2-region-evidence.md) and [local review](m2-local-review.md).

**A-05 implemented for the current helper set, not a complete parser sandbox:**
the nine short-lived PDF/crop/vector helpers and the managed OCR model tree now
use explicit cgroup containment, whole-tree termination, bounded pipes and
verified release. The managed OCR path has both contained synthetic tests and a
200-page private-corpus run (`22` clean releases, `0` OOM kills); private-corpus
visual/OCR-fidelity review is still pending. Production deployment and
adversarial parser coverage remain outside this claim.

**O-01 MEASURED_SCOPED:** the frozen 1.5 GiB admission floor is implemented
before a worker can claim a job and is boundary/unit tested; a low-memory pause
does not claim, reaper-mutate or create a lease. The host reserve remains 3 GiB
for model stages. Contained whole-stack measurement including the Rust API,
PostgreSQL, worker, helpers, and active `Vulkan0` OCR model
(`artifacts/private/o01-1794542035999/o01-measurement.json`) observed
`cgroup_peak_absolute_bytes: 321,748,992` (unit peak `918M`, `0B` swap),
`minimum_mem_available_bytes: 9,602,314,240`, DRM VRAM peak `2,647,830,528` of
`4,294,967,296` bytes, DRM GTT peak `252,223,488` bytes, and
`safety_reserve_respected: true`. Redis is not part of the current M2 stack
and remains excluded from this measurement.

**O-02/M2 G-7 PARTIAL:** fenced persistence, authorization, `S-02`/`S-03`/`S-04`
filtered HNSW vector storage/search, `P-04` structural merge guard, and
contained `Qwen3-Embedding-0.6B-Q8_0.gguf` sequential residency are implemented
and tested; crash-restart model recovery tests and measured private-corpus recall
benchmarks remain open. The
[license inventory](m2-license-inventory.md) remains PARTIAL pending the user's
local signature on [the sign-off checklist](m2-license-signoff-checklist.md)
(AWAITING USER SIGN-OFF). No content-quality, load, training, mobile,
deployment, or M3–M8 acceptance is claimed.

`DEC-005` remains OPEN and `DEC-008` remains ESCALATE; no protection migrations,
combat balance constants, or competitive scoring dispatch were changed.
