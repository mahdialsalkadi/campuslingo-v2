# CampusLingo v2 — Consolidated Decisions and Implementation Plan

Version: 1.3
Date: 2026-11-12
Status: Corrective tracking update; M0 foundation verified with open feasibility gates, M1 baseline acceptance recorded, M2 in progress. CL2-CORR-001 product proposals and milestone reassignment remain pending approval.

This document consolidates the original engineering blueprint, the read-only investigation, the user's answers and corrections, and the approved implementation plan. A future coding session should be able to continue from it without reconstructing the conversation.

### Approved architecture amendment — Rust/WASM

The user subsequently required Rust for the backend API, authoritative 90-second WebSocket server, matchmaking, FSRS-6 engine, ingestion orchestrator, and background job worker. Use `axum`, `tokio`, `sqlx`, and `serde`. Use the official `fsrs` crate from `open-spaced-repetition/fsrs-rs`, explicitly configured for the FSRS-6 algorithm, and compile the shared memory/combat-validation crate to WebAssembly for the React/Capacitor client. Crate release numbers must not be confused with algorithm versions.

AI models remain separate processes accessed through local `llama-server` HTTP endpoints. TTS uses `ort` in an isolated Rust worker, with one model active at a time across both GGUF and ONNX stages. Python is restricted to offline M8 training and isolated CLI evaluation/oracle scripts; it is not an application server or production ingestion worker. Initial Python/TypeScript evaluation code may be retained as clearly labeled offline reference tooling, not as the canonical production implementation.

The Rust online server RSS target is **under 50 MiB**, measured separately from PostgreSQL, Redis, model processes, and the ONNX worker. This is an acceptance target to benchmark, not a guaranteed property of Rust. Reserve resources for those other processes separately; apply minimal feature sets, bounded queues/pools, explicit thread counts, and release-mode builds.

Canonical contracts now originate in Rust `serde`/JSON Schema types. The primary parity gate becomes native Rust versus the compiled WASM implementation on the same golden inputs, including sandbox and combat validation. Python/TypeScript comparisons are diagnostic oracles, not a requirement to maintain separate runtime schedulers. Any older Python/FastAPI/Pydantic-as-authority wording below is superseded by this amendment.

### Explicitly approved precision amendment

After observing upstream f32 native/WASM drift (maximum approximately 0.00008 days in the initial shared vectors), the user explicitly approved the f32-aware policy: `absolute_error <= 1e-6 + 1e-6 * abs(reference)` for floating memory estimates. Integer combat, validation decisions, rounded schedule days, and other discrete outcomes remain exact. Preserve absolute-only diagnostics, including failing vectors, rather than concealing prior drift. This amendment supersedes the earlier absolute-only native/WASM acceptance threshold; it is not permission to alter model parameters, omit vectors, weaken content validation, or declare unrelated M0 gates complete.

### Corrective integration — CL2-CORR-001, tracked, not blanket-approved

The user requested incorporation of CL2-CORR-001 v1.0 into this plan. The detailed register is [campuslingo-v2-corrective-addendum-v1.md](campuslingo-v2-corrective-addendum-v1.md), stored alongside this plan. This revision corrects stale wording, records remediation/evidence requirements, and reconciles them with existing implementation. It does **not** approve D-01, D-02, D-03, R-06 hysteresis, optional N-* features, or the proposed Part M milestone reassignment. The approved architecture, precision policy, full scope, and eight fairness rules retain precedence. Proposed quantization and upstream math-backend changes also require a reviewed definition before changing runtime behavior.

Third-party support, license, quality, and performance claims remain verification tasks unless linked to actual evidence. A defect in the conceptual blueprint is not automatically a defect in the new migrations; classify each item against the current code before prescribing changes. Incorporation is not milestone closure or implementation authorization while in Plan mode.

## 1. Sources, precedence, and working boundaries

- Original specification: `/home/mahdi/Downloads/campuslingo_v2_engineering_blueprint.md`, titled **CampusLingo v2.2 — Master Engineering Blueprint**.
- New project root, confirmed in the user's final answers: **`/home/mahdi/campuslingo-v2`**.
- Reference project: `/home/mahdi/campus-lingo`.
- Evaluation material: `/home/mahdi/UTM_eLearning`.
- Consolidated plan: `/home/mahdi/.opencode/plan/campuslingo-v2-master-plan.md`.
- Corrective register: `docs/corrective-addendum-v1.md` in the project repository (CL2-CORR-001 v1.0; proposals explicitly pending); working plan copy: `/home/mahdi/.opencode/plan/campuslingo-v2-corrective-addendum-v1.md`.

The user's final confirmations take precedence over earlier draft answers. In particular, `/home/mahdi/campuslingo-v2` is the canonical destination, not the earlier suggested `/home/mahdi/campus-lingo-v2`. The initial native-iOS requirement has been replaced by an iOS PWA for v1 and future Capacitor iOS configuration/scaffolding.

The new application has different goals from the reference study app. Do not treat this as an in-place upgrade, clone its product assumptions, or migrate its users and database by default. The reference project may be inspected read-only and selected helper code or tests may be adapted into the new project after review. Do not modify its files, database, configuration, or running services.

Version 1.0 was saved in Plan mode; implementation was subsequently authorized outside Plan mode. The reference project remains read-only. Current-state evidence is maintained in `/home/mahdi/campuslingo-v2/docs/m0-status.md`, `/home/mahdi/campuslingo-v2/docs/m1-status.md`, and their linked artifacts. The dated M2 handoff in §16 supplements those reports until a dedicated M2 status report exists. This plan is not a replacement test report. The present revision was prepared in Plan mode: only files under `/home/mahdi/.opencode/plan` may be changed.

The source blueprint remains reference input. Its FastAPI/py-fsrs/ts-fsrs runtime authority, public static crop paths, and conceptual startup DDL are superseded by this plan. Do not modify or copy those designs back into the new runtime; an annotation in the original blueprint or a project-document copy of the addendum is deferred until file edits outside the plan directory are permitted.

## 2. Confirmed product and delivery decisions

| Topic | Approved decision |
| --- | --- |
| Product direction | A new mobile-first competitive 3D learning strategy game, not a reskin of the existing app. |
| Overall scope | The entire blueprint, including bosses, clans, optional asset-generation evaluation, post-training, and a retention study. |
| Delivery method | Quality-gated milestones; no fixed deadline or unsupported calendar estimates. |
| Intermediate builds | Playable internal and consented evaluation builds are allowed; they do not replace the full agreed scope. |
| Web | Responsive mobile and desktop browser application. |
| Android | Capacitor-packaged app built on Linux and tested on the user's Android phone. |
| iOS v1 | Installable Safari PWA using Add to Home Screen, plus Capacitor configuration and an iOS scaffold where tooling supports generation on Linux. No native `.ipa`, Xcode build, or Apple signing required for v1. |
| Native iOS later | A future macOS/Xcode or macOS CI workstream; no Mac is currently available. |
| Languages | English first; Arabic remains in the complete roadmap. Chinese is not a required launch language. |
| Course sharing | Both public and private courses, with explicit authorization and publication permissions. |
| Runtime backend | Rust: axum, tokio, sqlx, serde; no Python/FastAPI application service. |
| Shared engine | Official Rust fsrs-rs configured for FSRS-6, plus shared combat/sandbox validation compiled to WASM. |
| Server memory | Target less than 50 MiB RSS for the Rust online process; measure idle and loaded behavior, excluding separate database/cache/model workers. |
| Python | Offline training and isolated CLI evaluation only. |
| Inference | Local models only; out-of-process llama-server HTTP plus ort in an isolated Rust TTS worker. No hosted model/OCR/TTS APIs without new approval. |
| Hosting | The existing BC-250 hosts the online backend, PostgreSQL, Redis, and local AI ingestion. |
| Load target | 30 concurrent players, including 15 simultaneous live 1v1 matches. A league bracket of 30 is also required, but is separate from concurrent-load capacity. |
| Model scheduling | One ingestion model resident at a time: load, execute, persist intermediate results, unload, and verify resource release before loading the next model. |
| Co-op | Both asynchronous shared boss progress and live lobby-based co-op. |
| Clan wars | Weekly study-group versus study-group point races based on successful raids and validated FSRS-6 stability gains. |
| Asset budget | Free assets only: verified CC0 Kenney/Quaternius packs and procedural Three.js meshes. |
| Fairness | All eight recommendations in Section 10 are approved, including the refined reporting threshold. |
| Physical testing | Local browser and Android phone; actual iOS Safari/PWA checks if an iPhone is available. Never report a physical-device check as passed without evidence. |

## 3. Read-only findings already established

### 3.1 Reference application

Inspection of `/home/mahdi/campus-lingo` found:

- An existing Next.js 14/React application with challenge components, classroom features, course sharing, notifications, and browser/PWA-related files.
- A FastAPI backend, PostgreSQL 16 with pgvector, and Redis in the development configuration.
- PDF text extraction and embedded raster-image extraction utilities.
- Structured LLM generation and payload-validation code with existing regression tests.
- A custom FSRS-4.5 scheduler, additional memory-health calculations, and an Ebbinghaus-style progression counter.
- Existing 1536-dimensional embeddings, not the blueprint's proposed 1024-dimensional embeddings.
- A basic WebSocket quiz arena with in-process room ownership and a question clock, not the new 90-second strategy combat simulation.
- Password verification endpoints, but also an email-only identity path and endpoints accepting caller-supplied user identifiers. These are not an acceptable competitive authentication design.
- No GLB/GLTF/Blend assets found in the repository scan.

Useful references include:

- `services/core-ai/ingestion/parser.py`
- `services/core-ai/llm/generator.py`
- `services/core-ai/schema.py`
- `services/core-ai/arena/rooms.py`
- `services/core-ai/tests/`
- `apps/web/components/`
- `packages/database/schema.sql`

Do not blindly reuse old authentication, scheduling assumptions, reward logic, database bootstrap DDL, in-memory ingestion jobs, or multiplayer state management. Existing API rewrites are server-side Next.js behavior and cannot simply be carried into a bundled Capacitor client.

### 3.2 Local hardware snapshot

Read-only system inspection reported:

| Item | Observation |
| --- | --- |
| CPU | AMD BC-250, 8 cores / 16 threads exposed to Linux. |
| GPU | AMD Cyan Skillfish / BC-250. |
| Linux-visible memory | Approximately 11 GiB reported by `free -h`. |
| GPU memory reservation | 4.0 GiB reported through DRM sysfs. |
| GTT aperture/report | Approximately 11.4 GiB; this is not an additional independent pool of physical RAM. |
| Swap at inspection | Approximately 15 GiB configured, with existing usage. Swap is not a substitute for an acceptable working-set budget. |

These are observations, not completed workload benchmarks. Refresh them at M0. Do not add overlapping unified-memory measurements together or assume that model file size equals runtime memory. The original blueprint's per-model RAM figures and `< 6.5 GB` ingestion target are unverified targets.

### 3.3 Evaluation material

The supplied folder contained 55 PDFs at inspection, including Python, discrete mathematics, data management, mathematics for machine learning, and other university material.

Preliminary sampling found:

- Python Chapter 1 has extractable digital text.
- Python Chapter 3 has almost no extractable text on the sampled pages, despite embedded images.
- Python Chapter 4 has similarly low extractable text on sampled pages.
- Set Theory has image-based sampled pages with no extractable text.
- Logic includes mathematical operators, formulas, and negation that must survive extraction.
- Data Management contains mixed text/figure content.

Low-text image pages are OCR candidates, not proof of how a document was originally scanned or authored. Cover pages alone must not cause a whole digital deck to be classified as a scan.

OCR is an early dependency for the user's real material, not an optional late enhancement. No supplied lecture content has been approved for public redistribution or model training merely by supplying its path.

## 4. Product model and fixed-versus-generated boundary

The fixed shell contains the game rules, arena and island geometry, units, towers, animation, effects, audio behavior, interface, and grading implementations. AI must not invent a new game engine for each lecture.

Lecture ingestion fills four customization slots:

1. **Battle cards:** concept title, tactical summary, source figure, archetype, and validated challenge content; memory-driven collection/visual progression.
2. **Fortress towers:** course concepts and prerequisite topology; per-student shield efficiency and visual tiers.
3. **Tactical pods:** declarative hologram interactions or source-slide visual drills, accompanied by prepared briefings/subtitles.
4. **Chapter bosses and loot:** chapter-aligned challenges, solo/co-op progression, trophies, and cosmetic rewards.

Core game modes:

- 90-second live 1v1 duels.
- Recorded-ghost duels, with a labeled bot when no eligible replay exists.
- Asynchronous base raids and revenge.
- Weekly leagues with 30-player brackets and ten tiers.
- Solo chapter bosses.
- Asynchronous shared-HP co-op bosses.
- Live lobby co-op bosses using the same shared-HP/challenge model.
- Weekly clan wars.

Prepared content, deterministic grading, memory calculations, combat, and reward settlement must work without any LLM call during the daily play loop.

## 5. Technical architecture and target layout

```text
/home/mahdi/campuslingo-v2/
  apps/
    web/                  Next.js web host, discovery, accounts, course pages
    mobile/               Static client build and Capacitor Android/iOS configuration
  packages/
    game-client/          Shared React UI, Three.js scenes, touch controls, PWA-compatible play
    contracts/            Generated Rust JSON Schema and TypeScript contracts
    engine/               Browser WASM bindings and native/WASM parity tests
  crates/
    domain/               Shared serde contracts, FSRS-6, sandbox/combat validation, WASM
    storage/              sqlx persistence, versioned migrations, authorization, durable jobs
    server/               axum/tokio API, authoritative WebSockets, sqlx persistence
    ingestion/            Rust orchestration, single-model lifecycle, llama HTTP, optional ort
  tools/eval/             Isolated Python/TypeScript reference evaluation only
  tests/
    fixtures/             Small synthetic fixtures and private-corpus manifests
    integration/          Cross-service and persistence tests
    load/                 30-player / 15-duel workload
```

This is the target layout, not an assertion that every app/package already exists. Rust crates, contracts, the WASM engine package, evaluation tooling, and tests have been created; consult `docs/m0-status.md` and `docs/m1-status.md` for actual delivered state. Migrations currently live in `crates/storage/migrations/`, not a copied legacy bootstrap file. Pin compatible, maintained versions of Node, offline Python, the selected web host, React, React Three Fiber, Capacitor, PostgreSQL, and model runtimes before adopting them. Do not mandate framework upgrades from unverified release-date claims or inherit the old app's pins.

Architecture rules:

- The mobile bundle must not depend on Next.js rewrites, server actions, or other unavailable server runtime behavior. A small static/Vite host may consume the shared React client if selected in M0.
- PostgreSQL is the durable source of truth for published content, reviews, economy, jobs, and completed matches.
- In-flight matches require a durable start/intent record and a defined crash policy (R-07): bounded snapshot-based resume or deterministic void with no rewards/trophy delta. Select the policy before M5 implementation; completed-only storage is insufficient. Settlement keys and outcome/reward writes belong in the same transaction.
- Redis supports bounded queues, scheduling, matchmaking, and coordination; it is not the only durable store for outcomes or review events.
- Start with a single authoritative combat owner on the local host. Do not pretend optional Redis publishing already provides safe distributed simulation ownership.
- The server owns time, grading, damage, energy, shield state, match results, and rewards. Clients submit actions, not trusted outcomes.
- AI workers and the online API run as isolated processes with independently enforceable resource limits.
- Private assets require authorization. An opaque URL or a tenant ID in a path is not sufficient access control.
- Use a separate database/storage namespace and avoid port collisions with the reference application.
- External player access needs HTTPS/WSS and a deliberate ingress configuration. Do not expose PostgreSQL, Redis, or raw inference ports publicly.

## 6. Content contracts, persistence, and provenance

Minimum versioned contracts, with Rust serde types as the canonical source:

- `LearningUnitBundle`: source-grounded concept content, card presentation, challenges, answer/feedback data, provenance, and generation metadata.
- `SandboxSpec`: archetype, safe parameter domains, restricted formulas, interaction limits, success/failure predicates, and validation evidence.
- `DirectorTrack`: semantic camera intentions such as `focus_role`, `shot_type`, and `explode_factor`, plus subtitle/audio references.
- `AudioManifest`: codec, language, text, duration, timestamp provenance, and alignment-quality metadata.
- Multiplayer commands/events: match ID, authenticated actor, protocol version, sequence number, server timestamps, and content version.
- Review/economy events: authenticated actor and tenant/course scope, attempt/idempotency key, content/scheduler/scoring/rating-map/parameter-set versions, before/after state, and independent FSRS/reward eligibility with reason codes. These are target contracts, not a claim that every field exists in M1.

Core data domains:

- Users, authenticated sessions, workspaces/tenants, courses, memberships, roles, and publication permissions.
- Lectures, source files, pages/regions, crops, nodes, challenges, content versions, and publication/review state.
- Node simulations, prerequisite dependencies, per-user FSRS cards, and review logs.
- Ingestion jobs, stage outputs, model/runtime versions, and failures/retries.
- Matches, replay events, raid snapshots, revenge eligibility, and reward transactions.
- League brackets and standings, clans and memberships, weekly wars, bosses/lobbies/contributions, and cosmetic inventory.
- Content reports/quarantine decisions and notification preferences/delivery records.

Shared course content must remain separate from individual memory state and player progression. Course/workspace authorization is mandatory even when a deduplication key includes `tenant_id`.

The blueprint's SQL is conceptual input, not production-ready migrations. In particular, `ADD COLUMN IF NOT EXISTS embedding vector(1024)` does not convert an existing 1536-dimensional column. New non-null fields require valid initialization/backfill on populated tables. The new project should use explicit versioned migrations rather than copying startup DDL blindly.

Record an embedding model ID, revision, dimension, and preprocessing version. Never pad or mix incompatible vectors and treat them as equivalent. Preserve provenance when reusing deduplicated content; never merge different students' memory records.

Apply the S-01…S-22 migration register in the corrective companion. Fresh-schema items, existing partial controls, and future-domain tables must be distinguished. M1 already supplies memberships, composite tenant/course FKs, `gen_random_uuid()`, before/after review states, per-attempt uniqueness, append-only triggers, and a minimal reward ledger; it does not yet supply the complete match/session eligibility, content versions, parameter history, or full economy. Use additive migrations, explicit backfills, privilege tests, and retention/erasure design; never edit an applied migration to make old evidence appear current.

The companion's SQL caveats are mandatory review guidance: caller-selected tenant settings alone do not prove authorization; match winner checks must distinguish human/ghost/bot and null/draw states; non-null additions require populated-table handling; hash uniqueness must not erase source occurrences; ANN predicates must actually exclude quarantined versions. No conceptual SQL sketch is ready-to-run migration code.

## 7. Ingestion and local-model lifecycle

### 7.1 Pipeline

Select and record the PDF processing implementation compatible with the Rust-only production rule (R-01/G-1). PyMuPDF is not permitted as a production worker. The current M2 implementation path is Rust orchestration of local Poppler (`pdfinfo`, `pdftohtml`, `pdftoppm`, `pdfimages`), ImageMagick WebP encoding, and tesseract geometry, with PaddleOCR-VL text through leased `llama-server`. Record exact licenses and deployment/distribution obligations before release adoption; subprocess separation is not a license exemption. The three representative real PDFs, vector-region fidelity, subprocess peak RSS, and hostile-input behavior remain separate gates, not established by the synthetic fixture.

1. Validate PDF bytes, size, page count, encryption, processing limits, and access rights.
2. Extract digital text, layout, image/vector regions, and language signals.
3. Route low-text, corrupted-font, math/table, or image-heavy regions to OCR/VLM processing.
4. Preserve page/region provenance, code whitespace, formulas, numbers, units, and negation.
5. Produce optimized WebP diagram crops, including rendered vector diagrams, and retain region coordinates.
6. Build language-aware lecture chunks without silently dropping source content or disturbing sequence.
7. Classify structure/theme, compact consecutive 2–4 `AXIOM` chunks, and choose modifiers.
8. Produce scoped embeddings and compare exact critical-token/formula fingerprints before semantic reuse.
9. Run Head A for pedagogical/card/sandbox/director specifications.
10. Run Head B sequentially after Head A to bind the validated spec to an audited template.
11. Generate TTS and timestamp data, validate it, and persist the completed bundle.
12. Publish only validated content; route uncertainty/failure to visible review or safe fallback states.

Work across chunks can be queued and batched, but the user's one-model-resident constraint overrides the blueprint's unrestricted inference parallelism. Head A and Head B remain sequential per chunk. CPU extraction and deterministic binding may overlap only within measured host limits.

Additional remediation requirements (S-03/S-04, A-05, O-02, P-04):

- Version normalization separately from source text. Keep critical-token order, unit/formula case, superscripts, numbers, and negation safe. Evaluate Unicode NFKC, Arabic diacritic/tatweel removal, and Arabic-Indic digit normalization as language-aware candidate matching; do not let destructive normalization authorize an unsafe merge. Store every source occurrence even when canonical chunks are shared. Keep private scopes isolated before retrieval/reuse.
- Select and measure the embedding configuration; verify pgvector/version/dimension and scope-filtered retrieval with a non-quarantined ANN index where needed. Exact/near-text dedup alone does not satisfy the embedding-selection task. Revalidate reusable older generator outputs against current contracts/source checks instead of discarding all reuse on a version bump.
- Record numeric limits at ingress and every stage: file/page/image/pixel/expansion/output size, wall time, retries, queue depth, and retained artifacts. Enforce limits **during** processing, with child termination/reaping and OS-level resource/network restrictions where required; a size check after buffering or a dropped timeout future is not a proven bound. Detect OCR truncation, geometry/crop omissions, vector-only pages, image-list failures, and nonblank pages routed as blank; preserve visible recoverable failures.
- Existing M1 upload cap is 80 MiB; the M2 CLI validator currently advertises 200 MiB/200 pages. Reconcile these as explicit entrypoint limits rather than claiming one universal cap. Current constants (200 DPI default, 24M render pixels, 60 crops/page, 16 MiB OCR input, 1 GiB OCR model RSS) are implementation settings to audit, not complete safety evidence. Resume identity must include input hash, tool/model/preprocessing versions and effective settings, plus artifact integrity checks.

### 7.2 Memory scheduling

- A model lease/supervisor prevents two ingestion models from being loaded simultaneously, including OCR, embeddings, generation, TTS, and alignment stages.
- Batch compatible work to reduce reload costs without starving other jobs indefinitely.
- Persist intermediate artifacts before switching model stages.
- Verify process/resource release; dropping a Python reference alone does not prove GPU allocations were released.
- Bound threads, context lengths, image sizes, queue depth, retry counts, and per-stage timeouts.
- Pause, defer, or terminate a background stage safely when server health or memory headroom crosses measured limits.
- Do not stop unrelated user processes or alter GPU memory reservations without permission.
- Measure cold starts, warm calls, and model switches. A quantized file's size is not a complete RAM/VRAM budget.
- Reconcile the complete resident set: online Rust <50 MiB target, database/cache, model, Rust worker, PDF/image subprocesses, and reserve. Record peak sampling/high-water marks, in-request watchdog enforcement, failure cleanup and actual resource release. A before/after OCR check or lease alone does not demonstrate a hard continuous memory ceiling.

### 7.3 Blueprint model candidates — verification required

The following are candidates from the supplied specification, not verified endorsements. Confirm availability, hashes/revisions, tokenizer/template behavior, quantization, architecture support, licenses, and actual local execution before adopting them.

| Role | Candidate | Blueprint configuration / important caveat |
| --- | --- | --- |
| OCR/VLM | [mradermacher/PaddleOCR-VL-1.6-GGUF](https://huggingface.co/mradermacher/PaddleOCR-VL-1.6-GGUF), upstream [PaddlePaddle/PaddleOCR-VL-1.6](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6) | Q8_0 plus mmproj-f16; verify actual llama.cpp vision support and full runtime footprint. |
| Optional fast router | [chaoliangUNSW/Jev-Style-2B-Decision-v4-GGUF](https://huggingface.co/chaoliangUNSW/Jev-Style-2B-Decision-v4-GGUF) | Q4_K_M; audit training/source licenses and probability/logit access; benchmark against the simpler Qwen path. |
| Core generator / Arabic router | [Qwen/Qwen3.5-4B](https://huggingface.co/Qwen/Qwen3.5-4B), GGUF candidate [unsloth/Qwen3.5-4B-GGUF](https://huggingface.co/unsloth/Qwen3.5-4B-GGUF) | Q4_K_M; constrained structured output with schema and business-rule validation. |
| Embeddings | [Qwen/Qwen3-Embedding-0.6B-GGUF](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF), alternative [ggml-org/bge-m3-Q8_0-GGUF](https://huggingface.co/ggml-org/bge-m3-Q8_0-GGUF) | Q8_0; verify requested output dimension and multilingual quality rather than assuming compatibility. |
| Optional static GLB baker | [ccore/Qwen3.5-4B-GLB-3D-Coder](https://huggingface.co/ccore/Qwen3.5-4B-GLB-3D-Coder), GGUF candidate [mradermacher/Qwen3.5-4B-GLB-3D-Coder-GGUF](https://huggingface.co/mradermacher/Qwen3.5-4B-GLB-3D-Coder-GGUF) | Q4_K_M; offline isolated baking, not arbitrary code in the game. |
| English TTS | [thewh1teagle/kokoro-onnx](https://github.com/thewh1teagle/kokoro-onnx), model candidate [shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped](https://huggingface.co/shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped) | Verify the claimed timed API, `pred_dur` behavior, pronunciation, codec support, and timestamp semantics. |
| Arabic TTS/alignment | Piper `ar_JO-kareem-medium` via isolated Rust/`ort`; alignment decision pending between CTC forced alignment via `ort` and `whisper.cpp` local HTTP (R-02/G-5) | Verify voice/model licenses, Arabic quality, and runtime APIs before selection. `faster-whisper` is offline diagnostic tooling only in `tools/eval/`. Whisper word timestamps are ASR estimates, not forced alignment of a supplied script; choosing `whisper.cpp` alone does not establish that capability. |

Audit runtime/dependency licenses as well as model labels, including PDF processing libraries, fonts, voice data, and training data. Do not copy the blueprint's blanket Apache/MIT claims into release documentation without checking them.

Router thresholds such as `P(is_fluff) > 0.70` and `P(visual_3d) >= 0.45` are calibration candidates. Sampling temperature is not a replacement for probability calibration. Arabic routing and exact model alternatives require measured evaluation, not assumed language support.

Calibrate per task/language on held-out labeled data and report ECE with logit/probability access verified; there is no justified per-question temperature fit without suitable labels (P-03). For Arabic speech, evaluate optional technical-term diacritization without altering source meaning, define script/transcript normalization and a measured WER gate, and record timing provenance/quality in `AudioManifest`. Regeneration and any proportional-timing fallback must be bounded and marked degraded, not mislabeled forced alignment; publication must still satisfy accessibility/timing acceptance (P-01/P-02).

## 8. Templates, themes, and safe simulation execution

Required themes: `BIO_SYSTEM`, `CYBER_GRID`, `MACRO_POLIS`, `ARCANE_ARCHIVE`.

| Archetype | Fixed interaction | Required modifier set |
| --- | --- | --- |
| `PIPELINE` | Conduit/valve routing with bounded flow rules. | `REVERSE_ENG`, `BOTTLENECK_RELIEF`, `FAULT_INJECTION` |
| `TRADEOFF` | Coupled-core equilibrium with safe declarative formulas and target bounds. | `OVERLOAD_STRESS`, `BUDGET_CAP`, `PARETO_BALANCE` |
| `SPATIAL` | Exploded layers or slide-region pinpointing. | `BLIND_DIAGNOSIS`, `SURGICAL_REPAIR`, `CROSS_SECTION_SCAN` |
| `CONTRAST` | Dual-chamber comparison/sorting under explicit stimuli. | `STRESS_BENCHMARK`, `BLIND_CHAMBER_ID`, `CROSS_CALIBRATION` |
| `TRAP` | Timeline fault detection with a defined scientific violation. | `FORENSIC_AUDIT`, `COUNTEREXAMPLE_PROBE`, `SAFEGUARD_PATCH` |
| `AXIOM` | Low-overhead speed rounds using text, source crops, and reflex interactions. | `SPEED_BLITZ`, `COROLLARY_CHAIN`, `BOUNDARY_TEST` |

Slide alternatives include Spot Mistake, Tap to Fill, Swipe Boolean, and Match Madness where the source supports them. Rotate modifiers in draw/review scheduling without changing the lecture's logical order or excluding genuinely due reviews.

Safety requirements:

- The normal game path accepts validated JSON, never model-generated JavaScript.
- Represent formulas using a restricted expression tree with an allowlist, input domains, operation limits, and finite-output checks; no unrestricted `eval` or `exec`.
- Reject unknown schemas, references, execution modes, unsafe assets, and excessive geometry/textures.
- Validate success/failure predicates and a reachable valid solution. Finite sampling alone cannot prove arbitrary continuous mathematics solvable; unsupported cases fail closed or need a stronger verifier.
- Current supported discrete slider grids are exhaustively checked within the 4096-state ceiling. Preserve that guarantee; use sound interval analysis or another reviewed proof method before expanding to continuous domains (P-05). Reachability also needs measured touch-feasible target widths, not just a mathematical solution (P-06).
- Semantic camera intents resolve deterministically from scene roles/bounds, such as `THREE.Box3`, with safe limits.
- Simulation validity and educational correctness are separate evaluation dimensions.
- Prefer static GLB output from any optional generated-asset experiment. If an iframe experiment is retained, isolate it without `allow-same-origin`, block network access, restrict messaging, and audit the full threat model before enabling it.
- Validate embedded/external GLTF resources and block unexpected network fetches even for ostensibly static assets.
- Before any optional iframe experiment, validate `event.source`, a per-session nonce, message schema/version/sequence, and bounded payloads; an opaque sandbox origin reports `"null"` and is not an identity check. Use appropriate CSP/sandbox isolation without `allow-same-origin`. Generated code remains disabled unless separately approved (A-03).
- Validate GLTF/GLB resource URIs, allowed extensions, textures/triangles, buffer sizes, and decompression expansion; test that no unexpected network fetch occurs (A-04). Static output is not inherently safe.

## 9. FSRS-6, shields, and progression

The Rust backend is authoritative. The same official fsrs-rs-based domain implementation is compiled to WASM for client previews/validation. Reuse the old scheduler tests or initial Python/TypeScript harnesses only as offline diagnostic references, not as production schedulers or proof of FSRS-6 compatibility.

Blueprint notation, to be reconciled explicitly with the chosen libraries' parameter/sign conventions:

```text
factor(d_c) = 0.9^(-1 / d_c) - 1
R_c(t) = (1 + factor(d_c) * delta_t_c / S_c)^(-d_c)

I_u(t) = mean(R_c(t) for c in cards_of_node_u)
E_u(t) = I_u(t) * product(
  1 - w[p,u] * max(0, (0.70 - E_p(t)) / 0.70)
  for p in prerequisites_of_u
)
```

- Evaluate prerequisite efficiency in topological order; reject cycles and cross-course dependency mistakes.
- Shield brownout threshold is `E_u < 0.70`; active shields have `E_u >= 0.70`, subject to approved raid eligibility rules.
- Define new/unreviewed nodes, zero stability, empty card sets, timestamp units, and same-day attempts without division-by-zero or invented recall histories.
- Validate the official FSRS-6 parameter vector/decay representation instead of adding a magic per-card decay constant and assuming parity.
- Test floating memory estimates with the **approved** policy `absolute_error <= 1e-6 + 1e-6 * abs(reference)` (§ precision amendment). Integer combat, validation decisions, rounded schedule days, and other discrete outcomes remain exact. Retain absolute-only diagnostics and failing vectors. Add reviewed canonical quantization/boundary rules under R-06 before changing rounding; quantization is not automatically sufficient for exact parity near every boundary.
- Record native reference magnitude, absolute/relative error, f32 ULP distance, and review-chain length for diagnostics (R-05/G-4). The existing maximum `8e-5` is at approximately `327.47098` days, allowed error `0.00032847098`, not a small interval; the stored 190-vector report passes the approved policy. Long-chain/boundary coverage and richer diagnostics remain open.
- Keep the shared domain free of runtime I/O, ambient clocks/entropy, and order-dependent hashing. Use explicit time inputs and stable ordering. Audit transitive WASM features, binary size, browser memory, and upstream math calls (R-03/G-2).
- Investigate a common math backend and FMA behavior (R-04/G-3); do not promise ≤1 ULP, assume a direct `libm` dependency changes upstream `fsrs`, or silently fork/replace the official scheduler. Preserve the approved policy regardless of investigation outcome.
- Preserve scheduler, scoring, rating-map, content, and parameter-set versions on each review. Prevent duplicate attempts **and distinct repeated exposures in the same match/session** from creating extra transitions/rewards; per-attempt idempotency alone is insufficient (D-02/S-08).
- Define per-user full FSRS-6 parameter sets and immutable history before optimization. M1 currently persists f32/SQL `REAL` stability/difficulty and fixed upstream defaults; document an exact f32 round-trip policy and DB restart fixtures, or review a storage change. `DOUBLE PRECISION` alone cannot add model precision or guarantee native/WASM parity (S-19).

Original visual intent:

- Tier 0: new/unreviewed blueprint ruin.
- Tier 1: reviewed, low stability, or brownout.
- Tier 2: reinforced/rare, originally `5 <= S < 21 days` and `E_u >= 0.70`.
- Tier 3: apex/legendary, originally `S >= 21 days` and `E_u >= 0.85`.

The original conditions leave gaps, such as `S >= 21` with `0.70 <= E_u < 0.85`. Define one complete, mutually exclusive state table before implementation. Memory-driven visuals and optional base progression must not override ranked normalization.

**D-01 remains a proposal:** use weakest reviewed-card stability, `E_u` quantized to `1e-4`, then first-match tiers: unreviewed → 0; `E_u <0.70 OR S_u <5` → 1; `S_u >=21 AND E_u >=0.85` → 3; otherwise → 2. The companion retains the exact table and unresolved empty/all-archived-card semantics. Do not treat it as approved by inclusion here.

**R-06 hysteresis remains a proposal:** enter brownout below `0.695`, exit at or above `0.705`. Until explicitly approved, the approved threshold remains `0.70`; do not silently change raid/shield semantics. Define quantization/tie-breaking and invalid/empty-state handling with fixtures before adoption.

## 10. Approved fairness, privacy, and anti-abuse rules

All eight recommendations below are approved. The user additionally specified the independent-report threshold and clan scoring basis.

1. **Normalize ranked combat power.** Keep the concept-card identity and memory-driven collection/visual progression, but use tournament-standardized combat levels in ranked 1v1. Learning history must not make a new student inherently unable to compete.
2. **Make raids opt-in and bounded.** At minimum, private study courses require explicit raid opt-in. Provide new-player protection, cooldowns, capped losses, and anti-farming rules; vulnerability alone is not consent to repeated punishment.
3. **Label ghosts and bots honestly.** Recorded classmates are ghosts, not supposedly live users. A cold-start bot must also be labeled. Use eligible, permissioned replay data and define adaptation to the current match state.
4. **Protect memory privacy.** Public discovery or a raid scanner must not reveal detailed per-student recall/stability history. Publish only explicitly permitted competition data.
5. **Handle defective-card reports without griefing.** A first report can trigger a server-controlled local replacement/refund so play continues, but not global eviction. Quarantine/evict the affected content version from eligible retrieval after at least **three independent eligible student reports**, or an execution-harness failure. Deduplicate reports, check enrollment/independence, and guard against Sybil accounts, brigading, repeated refund abuse, and invalidating an opponent's match. Do not delete memory records or unrelated versions merely to remove a bad vector from retrieval.
6. **Separate practice from eligible reviews.** Repeated exposures, repeated requests, end-of-session re-grades, and already-mastered easy prompts must not artificially inflate FSRS mastery, stability gains, or competitive rewards. Eligible learning events need an auditable definition.
7. **Keep uploads private by default.** Public publication and model-training use require appropriate permission. The supplied UTM PDFs are private evaluation inputs unless permission establishes otherwise.
8. **Gate procedural generation separately.** Prefer offline static GLB baking. Treat generated code and assets as untrusted; never silently enable arbitrary execution to satisfy visual variety.

D-02 proposes separate FSRS-eligibility and reward-eligibility gates, server-derived 1–4 ratings, and one transition per card per match. Its full predicate and ordered rating cases remain pending, not an approval to replace M1 grading automatically. Current M1 grading uses correct→3/wrong→1 and one practice-XP ledger entry per distinct completed attempt; it is not the final anti-farming policy. Before any competitive use, settle session/exposure identity, timing evidence, same-day behavior, high-recall reward treatment, and unreviewed-card enrollment semantics.

F-01…F-05 remain operational definitions to close: numeric new-player protection, treatment of unreviewed cards in `I_u`, attacker/defender daily caps/losses/cooldowns/revenge expiry, replay adaptation, permitted competition projections/pseudonyms, and notification caps/quiet hours/opt-outs with an explicit timezone policy. The addendum supplies no approved numeric raid caps; do not invent them.

## 11. Competitive rules and completion details

### 11.1 Live 1v1

- Server-owned 90-second match, two-lane arena, continuously regenerating energy, deployment, and deterministic damage/shields.
- Match questions come only from authorized shared course/lecture content, with eligibility rules for material the participants have encountered.
- Blueprint defaults: rapid correct answers can yield a 2.0x overcharge/shield-pierce effect; wrong answers can create a counter-strike window; final 30 seconds use 2x energy.
- Treat the `< 4 seconds` speed bonus as a tunable rule that must account for prompt length, interaction type, accessibility, and network measurement—not a universal memory-grade shortcut.
- The blueprint mentions 6–12 second challenges, 10–15 second pods, and 15–20 second briefings. Specify separate briefing/challenge clocks; do not charge a player combat time while forcing them through a longer unskippable narration.
- D-03's **pending** numeric proposal is lobby briefing or ≤3-second skippable in-match toast, 6–12-second archetype-specific challenges with pods ≤10 seconds; async/solo may have skippable 15–20-second briefings on a separate clock. Define authoritative clock charging so skipping/muting cannot change competitive opportunities. No new numeric budget is approved by this text.
- Overtime audio tempo changes are presentation only; provide mute/reduced-motion alternatives and never make audio mandatory to win.
- Prepare the ghost/bot fallback before the four-second matchmaking deadline. Define the measured server-side deadline and network assumptions instead of promising an absolute end-to-end internet guarantee.
- Protocol: authenticated actors, sequenced commands, bounded queues, server clocks, reconnect snapshots, duplicate rejection, rate limits, deterministic result settlement, and replay versioning.
- A replay cannot blindly apply historical damage or actions when the new battle state differs; define a fair replay policy.
- Define numeric heartbeat, reconnect-grace, authoritative snapshot cadence, command/event queue caps, backpressure and sequence-gap rules before protocol freeze (O-03). Kill/restart an in-flight owner to test the selected durable void/resume policy and exactly-once settlement (R-07).

### 11.2 Asynchronous raids and revenge

- Snapshot the eligible defender sector/content state at raid start; handle changes and concurrent attackers explicitly.
- Retain the blueprint's scout briefing, sandbox task, and three verification-drill structure where pedagogically appropriate.
- Reviews restore shield state through legitimate memory updates, not a client-provided repair flag.
- Revenge may use the blueprint's 1.5x reward intent, subject to caps, expiry, and anti-collusion rules.
- Raid results, losses, loot, notifications, and revenge eligibility must settle atomically and idempotently.

### 11.3 Leagues

- Ten tiers, weekly brackets of up to 30 comparable participants, top-five promotion and bottom-five demotion for full brackets.
- Define partial-bracket behavior, ties, league scope, boundary times/time zone, inactivity, late entrants, and exactly-once rollover.
- Reward genuine learning. The blueprint's 3x new mastery and 0.2x return for already-mastered content above `R > 0.95` are candidate coefficients to test, not substitutes for a precise event/scoring definition.
- Define memory-recovery scoring from valid before/after learning evidence. Do not count arbitrary elapsed-time changes or repeated exposures as progress.

### 11.4 Bosses and clan wars

- A chapter boss has a shared HP pool. Eligible group members deal validated damage by solving chapter/node challenges.
- Asynchronous mode accepts individually completed contributions into that pool.
- Live mode joins players into a lobby with synchronized shared progress/events and reconnect support. It uses the same challenge/contribution model rather than requiring an unrelated real-time MMO combat engine.
- Validate HP updates atomically and prevent duplicate submissions, overkill reward duplication, and repeat chest claims.
- Solo bosses and cosmetic loot remain in scope.
- Clan wars are weekly study-group versus study-group point races using successful raids and validated FSRS-6 stability gains.
- Version scoring coefficients and define eligible gains, caps, membership changes, team-size normalization, ties, and rollover before implementation. Do not invent those coefficients as already user-approved values.

## 12. Assets, layout, performance, and platform delivery

### 12.1 Asset sourcing

- Sources: [Kenney CC0 Game Assets](https://kenney.itch.io/) and [Quaternius low-poly packs](https://quaternius.com/), plus deterministic procedural Three.js meshes.
- Verify each selected pack/file's license and actual formats; do not assume every downloadable item is CC0 or already GLB.
- Keep an asset manifest with source URL, author, license evidence, version/hash, conversion notes, and any attribution requirements.
- Use original presentation and licensed material, not copied Clash Royale/Clash of Clans assets or branding.
- Start with a cohesive low-poly family, then implement the four theme packs without sacrificing readability.
- Include models, rigs, animation, card frames, textures, icons, fonts, music, SFX, effects, and loot/cosmetic presentation in the audit.

### 12.2 Interface and accessibility

- Portrait starting point: approximately 55% arena/base and 45% thumb-zone interface, adjusted for safe areas, keyboard, captions, and device dimensions.
- Desktop starting point: approximately 65% scene and 35% inspector/tactical/leaderboard panel.
- Explicitly design tablet and intermediate-width layouts rather than leaving the 768–1023 px range unspecified.
- Arabic/English labels and captions use DOM/CSS overlays with correct bidi handling, text direction, and mixed mathematical notation.
- Provide equivalent 2D tactical play, keyboard/touch access, legible focus states, large targets, subtitles, mute, and reduced motion.

### 12.3 Performance and platforms

- 60 FPS is a measured target on named supported devices, not a universal Capacitor guarantee. Triangle count below 15,000 alone cannot establish it.
- Measure frame-time percentiles, draw calls, geometry, texture memory, asset loading, CPU use, and sustained thermal behavior.
- Establish measured draw-call, texture-memory, download/load-time and decompression budgets. Evaluate KTX2/BasisU, meshopt/Draco and dynamic resolution on supported devices; format names and triangle counts alone are not acceptance (O-05).
- Retain the blueprint's sub-28-FPS fallback intent, but use a measured window/hysteresis and a manual option so rendering modes do not oscillate. Reduced motion can select a low-motion/2D presentation without competitive loss.
- Test Android packaging and the user's physical phone on Linux.
- For iOS v1, provide HTTPS delivery, a valid manifest/icons, appropriate service-worker/update behavior, safe-area handling, touch/audio checks, and Add to Home Screen instructions/verification.
- Generate Capacitor configuration and the iOS project scaffold where supported without macOS-only dependency steps. Clearly distinguish scaffold generation from building/signing an `.ipa`.
- Native iOS compilation is not an M7 blocker. If no iPhone is available, clearly mark actual Safari installation/device behavior as unverified; browser emulation or a generated manifest is not proof that Add to Home Screen was tested.
- PWA installability does not imply offline ranked matches. Cache only permitted application/content data; do not queue unverifiable competitive outcomes as trusted wins.

## 13. Milestone roadmap

The approved implementation plan expands the original M0–M5 into **M0–M8** so authentication, economy, bosses/clans, platform delivery, and research all have explicit gates. Milestone numbers below refer to this consolidated plan. Nothing is silently removed from scope.

CL2-CORR-001 Part M proposes additional milestone assignments, reproduced in the companion for approval. Those are a **pending dependency overlay**, not a retroactive statement that all M0/M1 work failed or that optional N-* scope is approved. Close applicable safety/correctness gaps before relying on the affected feature; retain historical test evidence and distinguish it from new acceptance requirements. G-1…G-7 are tracked as proposed M0 addendum gates with existing evidence credited, not as newly completed gates.

### M0 — Feasibility, contracts, sandbox harness, and FSRS parity

Tasks:

1. Confirm the new destination is safe; record toolchain versions, free storage, existing service ports, and hardware/runtime capabilities without changing the reference project.
2. Create the minimal new workspace and reproducible dependency/lockfile strategy after leaving Plan mode.
3. Audit exact model repositories, files, revisions, licenses, supported backends, and timed-TTS APIs. Benchmark the simple Qwen router before committing to an additional fast router.
4. Refresh the memory baseline and benchmark one model at a time, including image sizes, context/KV-cache cost, cold/warm latency, unload behavior, and CPU/thread consumption.
5. Define a host resource budget that preserves PostgreSQL, Redis, and combat capacity. Establish admission control and model-switch scheduling.
6. Recursively inspect `/home/mahdi/UTM_eLearning` and automatically select exactly three representative English PDFs: one clean digital-text deck, one diagram/math-heavy deck, and one scanned/image-based/low-text-density deck. Record selection reasons, hashes, page statistics, and private paths.
7. Establish the shared schemas/protocol versions and an allowlisted sandbox expression model.
8. Build the sandbox validation harness with malformed, unsafe, non-finite, out-of-domain, and unreachable-target counterexamples. Accept only supported validated domains; do not claim a proof for arbitrary generated mathematics.
9. Pin the official Rust fsrs-rs implementation, explicitly select FSRS-6 parameters, resolve precision/state/time conventions, compile the shared domain crate to WASM, and run native/WASM golden-vector parity. Retain Python/TypeScript tooling only as offline diagnostic oracles.
10. Test local training feasibility independently of inference. A working GGUF inference runtime does not prove supported LoRA/GRPO training; AMD kernels, source weights, optimizer state, activations, and rollout memory require separate checks.

Corrective follow-up: review G-1…G-7, R-01…R-06, H-01…H-04 and S-19 in the companion. Preserve the already-built WASM/FSRS-6 baseline, measure remaining PDF/license/runtime/ULP gaps, and obtain explicit D-01/D-02/D-03 decisions rather than silently selecting product behavior. Part M's proposal to make every decision an M0 closure prerequisite is not yet approved.

Provisional corpus candidates from the preliminary read-only inspection, subject to automatic selection:

- Digital text: `/home/mahdi/UTM_eLearning/[SAIA1113] 02 PENGATURCARAAN PYTHON (PYTHON PROGRAMMING)/Week 1 _ 12th - 16th October 2026/Chapter 1_Understanding Python.pdf`.
- Diagram/math: `/home/mahdi/UTM_eLearning/[SAIA1143] 02 MATEMATIK DISKRIT (DISCRETE MATHEMATICS)/Chapter 1_ LOGIC/1 LOGIC_v362761.pdf`.
- Low text/image-based: `/home/mahdi/UTM_eLearning/[SAIA1113] 02 PENGATURCARAAN PYTHON (PYTHON PROGRAMMING)/Week 3 _ 26th - 30th October 2026/Chapter 3 - Operators and String Handling.pdf`.

Required artifacts in the new project (existing state is reported separately):

- A measured runtime/model compatibility and license inventory.
- A private-corpus manifest, not copied public university lecture files.
- `packages/contracts/` initial schemas and type-generation/parity checks.
- Native Rust sandbox harness, WASM entrypoint, and shared adversarial fixtures; Python harnesses are offline references only.
- `tools/eval/benchmark_en_ar.py` or equivalent **offline reference** evaluation entrypoint, with English first and Arabic explicitly pending representative material. This is not a Python production service or a claim that this exact file already exists.
- Synthetic FSRS-6 golden inputs, native Rust/WASM parity runners, and optional offline Python/TypeScript diagnostic outputs.
- Reproducible memory/latency reports with raw observations and pass/fail/blocked statuses.
- A recorded SFT/GRPO feasibility result, including any local-only blocker.

Acceptance:

- Runtime, model, license, and resource assumptions are measured or explicitly blocked; no silent unsupported dependency.
- Supported sandbox specifications fail closed on the defined invalid/adversarial suite and provide reachability evidence.
- Native Rust/WASM parity meets the user-approved f32-aware memory tolerance, while discrete outcomes remain exact. Record upstream precision and absolute-only diagnostics. Test state/time conventions and shared combat validation separately.
- English corpus selection and privacy handling are recorded.
- Any training incompatibility is documented as an M8 dependency. It need not stop independent application foundations, but full completion cannot be claimed while required training work remains blocked.

### M1 — Secure application and persistence foundation

- Implement authenticated sessions/account recovery, roles, course/workspace membership, publication controls, and authorized file/asset delivery.
- Add versioned database migrations, job persistence, idempotent review/economy writes, and separate per-user memory from shared content.
- Add retryable stage jobs, clear progress/failure states, logs, and basic operational health checks.

Corrective audit: S-01/S-02/S-05…S-08/S-10/S-21/S-22 and A-01/A-02/A-06/O-02. Credit current migrations and six real-database acceptance tests; target remaining review metadata, retention, per-actor/ingress budgets and exposure-level anti-farming with new migrations/tests. Authenticated asset delivery is established; future crop routes still need their own negative tests. Do not equate a per-IP auth throttle or local test deployment with full production ingress hardening.

Acceptance: cross-user/course access tests fail closed; impersonation, duplicate writes, and restart/retry scenarios cannot create unauthorized access or extra rewards.

### M2 — Region-aware PDF extraction, OCR, crops, and deduplication

- Implement the extraction/OCR/cropping/chunking pipeline and exact critical-token fingerprints.
- Produce WebP crops with source coordinates and select the measured embedding configuration.
- Persist recoverable intermediate outputs and visible failures rather than silently dropping chunks.

Corrective audit: R-01, S-03/S-04, P-04/P-05, A-05 and O-01/O-02. Finish DB-backed runs/pages/crops/chunks/failures and least-privilege worker/routes; prove scoped retrieval and retry/resume integrity, malicious-input subprocess limits and memory release. The current four-page synthetic annotated fixture is initial acceptance evidence only, not annotations of the three real lecture PDFs. Select and measure embeddings or leave that gate explicitly blocked; successful text dedup is not a substitute.

Acceptance: the annotated English corpus demonstrates usable extraction/crops; numeric/formula/unit/negation counterexamples do not merge incorrectly; private scopes remain isolated and the memory budget is respected.

### M3 — Sequential card/hologram generation and prepared speech

- Implement Head A followed by deterministic Head B, archetype routing, `AXIOM_PACK`, modifiers, slide hotspots, semantic camera tracks, English TTS, and subtitle/timestamp validation.
- Add confidence-based review, bounded repairs, source checks, and safe fallback publication.

Corrective audit: settle the R-02 Arabic alignment runtime before building that path; P-01…P-03/P-06 and S-11/S-12/S-20 require versioned speech quality/degradation, calibration, touch feasibility, content versions, declarative-only payloads and validated archetype/modifier/layout/cost rules. Tier-driven cost must never bypass ranked normalization; any new balance coefficients require review.

Acceptance: only validated/source-grounded bundles become playable; invalid specs fail closed; rejected generation leaves a visible recoverable state; briefing timing and alignment quality are measured.

### M4 — Fixed 3D shell and memory progression

- Build the fortress, arena, hand/deck, units, towers, effects, audio, six templates, slide alternatives, four themes, director behavior, and 2D presentation.
- Integrate FSRS-6, prerequisite shields, complete visual-tier rules, and collection progression.

Corrective audit: D-01 and R-06 require approved definitions/boundary tests; S-13 requires independent reports and quarantine per content version; O-05 requires measured rendering/device budgets.

Acceptance: hand-authored fixtures can exercise every mechanic with AI services disabled; memory parity and DAG boundaries pass; presentation remains legible and playable across supported layouts.

### M5 — Authoritative duels, ghosts, bots, and reconnects

- Implement 90-second server simulation, matchmaking/fallback, deployment/grading/damage/energy/overtime, replay adaptation, authentication, command sequencing, reconnect snapshots, and exactly-once result settlement.

Corrective audit: D-02/D-03 definitions, R-07 server crash/settlement recovery, S-14/S-15 match/replay schema and limits, A-03/A-04 optional presentation/asset security, and O-03 numeric WS budgets. Optional generated-code experiments remain disabled unless approved.

Acceptance: two real clients and adversarial protocol tests cover disconnects, retransmissions, late/duplicate actions, client tampering, racing result requests, and server kill/restart without state corruption or duplicate rewards. Prove the selected void/resume policy, exposure-level eligibility, and muted/skipped presentation parity. Prepare the 15-duel load harness; it becomes a blocking acceptance gate once available and is mandatory at M7, not replaceable by M1 HTTP load evidence.

### M6 — Raids, economy, leagues, bosses, and clans

- Implement snapshot raids, opt-in, protection/caps, shield repair/revenge, notifications, cosmetic inventory/ledger, weekly leagues, and rollover.
- Implement solo/shared-HP bosses, asynchronous contributions, live co-op lobbies, clans, and weekly wars using the approved scoring basis.
- Add independent-report quarantine and moderation/recovery workflows.

Corrective audit: S-09/S-16/S-17 and F-01…F-05 require complete mode enums, scoped brackets/tiebreak/rollover audit, full append-only economy/inventory, numeric protection/raid rules, replay adaptation, privacy and notification limits. M1's minimal practice ledger is not the complete economy.

Acceptance: concurrent raids, HP updates, duplicate claims, partial brackets, ties, scoring abuse, and scheduler retries settle correctly; easy-card spamming, repeated reviews, and collusion cannot dominate the economy or rankings.

### M7 — Cross-platform delivery, Arabic, hardening, and load validation

- Complete Android builds/device checks, responsive web/PWA behavior, iOS PWA/configuration deliverables, accessible 2D parity, and Arabic extraction/RTL/TTS/subtitle evaluation.
- Complete HTTPS/WSS deployment, notification opt-outs/quiet hours, security tests, observability, backups, and restore drills.
- Run the 30-player / 15-live-duel load test on the BC-250, first without ingestion and then with bounded sequential ingestion active.
- Measure API/command latency percentiles, event-loop delays, match deadline accuracy, RAM/GPU allocation, swap pressure, CPU, database connections/locks, queue growth, disconnect/recovery behavior, and background-worker throttling.

Corrective audit: O-04 CPU/Vulkan benchmarking (ROCm only if actually supported), O-06 blocking CI/tracing, O-07 retention/pseudonymization/restore evidence, O-08 version-pin audit, and S-18 notification/device/preferences delivery. Earlier dependencies remain early dependencies: do not defer framework pinning, GPU feasibility, Arabic runtime choice or missing authorization until M7 merely because Part M lists a later acceptance milestone. N-05 additions remain conditional on explicit approval; existing accessibility scope remains mandatory.

Acceptance: supported devices and deployed endpoints meet the performance budgets established from measurements; no OOM or unbounded queue growth; matches remain authoritative under ingestion load; iOS PWA checks are accurately labeled verified or pending physical-device access. Native Xcode signing is not required.

### M8 — Local SFT/GRPO and learning-retention study

- Build permissioned, reviewed training data and lecture-separated train/validation/held-out splits.
- Run a small reproducible SFT experiment before GRPO, subject to M0 local feasibility.
- Evaluate execution validity and pedagogy/source faithfulness separately; add model-version rollback.
- Treat the blueprint's `>= 98.5%` first-pass sandbox-harness success rate as an experimental target measured on an explicitly defined held-out dataset.
- Run consent-based 7-day and 30-day retention evaluation with independent recall assessments, documented cohorts, attrition, and uncertainty. Competition/engagement logs alone do not prove learning improvement.

Acceptance: demonstrated held-out improvement without educational regressions, reproducible model/evaluation artifacts, and an honest retention report. If training is unsupported on the local hardware, keep the milestone blocked until the user approves a feasible local alternative; do not silently switch to cloud training or declare full scope complete.

## 14. Verification strategy and definition of done

Test layers:

1. Unit/property/adversarial tests for schemas, restricted math, reachability, fingerprints, DAGs, scheduler adapters, reward math, and state machines.
2. Cross-language tests for generated types/protocol examples and deterministic FSRS vectors.
3. Real PostgreSQL/Redis integration tests for authorization, migrations, job retries, concurrent updates, settlement, and rollover.
4. Content evaluation against the private annotated corpus, separated from prompt/training examples.
5. Browser/mobile interaction tests, accessibility checks, Arabic bidi/math cases, and physical-device observations.
6. Two-client multiplayer/reconnect tests plus the 30-player/15-duel load workload.
7. Co-located ingestion stress, failed model load/unload, worker interruption, queue limits, backup restoration, and service-restart recovery.
8. Independent execution-quality and learning-quality measurements before and after training.

A milestone is complete only when its acceptance evidence exists and failures/limitations are recorded. A feature flag, placeholder, generated platform folder, schema definition, or passing mock-only test is not proof of end-to-end delivery.

For each corrective ID, record applicability, approval state, owning milestone, implementation/migration path, exact command/input/revision, expected result, actual result and evidence link. Every verification gate must end as pass/fail/blocked; until run it is open, not a pass. Preserve historical reports rather than relabeling them as new experiments. Make existing parity/sandbox/authorization gates blocking in CI; add two-client/crash/load gates when their features exist. Traces across ingestion stages must omit private source text, credentials and token material.

The complete project includes all approved modes, six archetypes/three modifiers each, four themes, prepared content/voice, source-grounded ingestion, authorized public/private sharing, FSRS-6, normalized competitive play, raids/revenge, leagues, solo/co-op bosses, clan wars, web/Android/iOS-PWA delivery, Arabic expansion, operational hardening, and the agreed training/research work. Original optional procedural GLB generation remains an explicitly gated optional feature, not a license to execute untrusted code.

## 15. Known risks and unresolved technical checks

These include technical validations and **pending product definitions**, not permission to shrink the approved full scope:

- Exact Android phone model and performance baseline have not been supplied.
- Access to an actual iPhone for Safari/Add to Home Screen verification is not confirmed. No Mac is available, and native iOS is deliberately outside v1 acceptance.
- Exact compatible model files, revisions, licensing evidence, quantized runtime support, and timed-TTS APIs remain unverified.
- Local SFT/GRPO feasibility is unknown; inference success does not establish training support.
- The three evaluation PDFs were automatically selected and are recorded in `docs/m0-status.md`/the private manifest: Types of Data, Logic, and Set Theory. Language/layout classification and full annotated extraction/crop acceptance are separate; the real corpus is not annotated by the synthetic M2 fixture. Representative Arabic PDFs/audio review material is still needed.
- Exact resource reservations, latency budgets, queue limits, model-switch costs, and safe concurrency must be measured. One model at a time is necessary but not sufficient to guarantee host health.
- Domain/TLS/ingress configuration and home-host connectivity need selection before outside players join. Do not assume router changes or third-party account creation are authorized.
- Template formula domains, complete tier boundaries, competition coefficients, partial-bracket rules, clan sizes/normalization, and report-independence checks need versioned concrete definitions and tests.
- A passing execution harness does not establish scientific truth, question quality, good alignment, or genuine retention gains.
- Free asset quality/cohesion and actual model/animation formats must be checked pack by pack; do not promise studio-quality production merely because assets are CC0.
- D-01 tier aggregation/empty-set rules, D-02 exposure/rating/reward policy, D-03 numeric clocks, R-06 quantization/hysteresis and Part M assignments remain pending as described in §17. An editorial plan update is not their approval.
- R-01 has a working Poppler/ImageMagick/PaddleOCR path, but vector/diagram fidelity, three-PDF crop/RSS evidence, licensing and real hard resource/network limits remain open. R-02 alignment runtime is undecided; `faster-whisper` cannot enter production.

## 16. Current handoff checklist

- [x] Original blueprint reviewed.
- [x] Existing project inspected read-only as a reference, not selected for in-place modification.
- [x] Canonical new root confirmed: `/home/mahdi/campuslingo-v2`.
- [x] Full scope, English-first sequencing, public/private courses, and quality-gate approach approved.
- [x] Local-only AI, co-located hosting, 30-player target, and sequential model residency approved.
- [x] iOS v1 changed to PWA plus future Capacitor configuration/scaffold.
- [x] Both co-op modes and weekly clan-war scoring basis clarified.
- [x] Free-asset sources and automatic three-PDF selection approved.
- [x] Eight fairness recommendations and refined report threshold approved.
- [x] Consolidated Markdown plan requested and saved in the permitted plan directory.
- [x] Switch out of Plan mode before implementation.
- [x] Create/initialize the new project without touching the reference project.
- [x] Apply the user-approved Rust/WASM architecture amendment; retain Python only as offline evaluation tooling.
- [x] Record native/WASM FSRS-6 foundation gates; 190 shared inputs pass the approved policy, with absolute-only drift retained.
- [x] Record M1 baseline acceptance: six real-DB tests; HTTP load probe below the online Rust RSS target (not a duel benchmark).
- [x] Start M2 Rust PDF/OCR/crop/dedup CLI and synthetic-fixture probes; full M2 acceptance is not complete.
- [x] Incorporate CL2-CORR-001 as a corrective register with product proposals and milestone changes explicitly pending.
- [ ] Close remaining M0 feasibility/license/runtime gates and proposed addendum gates as applicable; do not mark all M0 work complete from foundation tests alone.
- [ ] Obtain explicit approvals/refinements for D-01, D-02, D-03, R-06 hysteresis and proposed milestone reassignment; resolve quantization and other behavior-changing details before implementation.
- [ ] Finish M2 persistence, authorization, worker/restart evidence, safety limits, measured embedding selection and real-corpus fidelity/licensing evidence.

No implementation or benchmark was performed while preparing the original plan or this corrective planning revision. Earlier authorized implementation produced M0 foundation evidence, M1 migrations/acceptance and M2 work in progress. As of this update, `docs/m0-status.md` still records open overall M0 gates, while `docs/m1-status.md` records baseline M1 acceptance. Commit `b216667` contains the M0/M1 baseline; M2 changes remain uncommitted in the handoff and must not be discarded. No production deployment, training run, full duel load or physical-device acceptance is claimed.

M2 handoff, 2026-11-12: the latest local synthetic-fixture probe reported four successful pages (two digital, one OCR, one blank), 21 chunks, 21 crops, five duplicates and OCR-model peak RSS 730,644,480 bytes (~696.8 MiB). This is recorded probe evidence, **not** a benchmark rerun during this plan edit or proof of complete M2, security hardening, annotated real-corpus quality, embeddings or licensing. The existing CLI/page manifests must be preserved and promoted into reproducible acceptance artifacts when implementation resumes.

## 17. Corrective addendum and approval ledger

CL2-CORR-001 v1.0 is tracked in the project at [`docs/corrective-addendum-v1.md`](docs/corrective-addendum-v1.md), with the maintained working copy at `/home/mahdi/.opencode/plan/campuslingo-v2-corrective-addendum-v1.md`. The project copy and these paths were updated for publication at the user's request. Preserve the source addendum's stable D/R/S/A/P/F/O/H/N/G IDs; O-1…O-4 are editorial edit IDs, distinct from operational O-01…O-08.

| Item | State at v1.3 | Required before changing behavior |
| --- | --- | --- |
| D-01 tier table | Proposed; not approved | Define `reps_u`, empty/all-archived sets, minimum stability aggregation and quantization; approve and property-test |
| D-02 review/rating gates | Proposed; not approved | Define match/session exposure keys, trusted timing, ordered rating cases, partial/timeout/abandon, same-day path and independent reward rules; approve fixtures |
| D-03 clock budgets | Proposed; not approved | Resolve clock charging/skippable-toast fairness, per-interaction windows and accommodations; approve numeric budgets |
| R-06 hysteresis | Proposed; not approved | Explicit approval to change the current `0.70` brownout semantics; define initial state and transition persistence |
| R-06 quantization | Candidate mechanism; not yet defined/implemented | Review precision/rounding compatibility and boundary fixtures; it must not weaken exact discrete outcomes |
| Part M milestone overlay/G-1…G-7 assignment | Proposed; not approved | Approve assignments, credit existing evidence, resolve early dependency versus final acceptance timing |
| N-01…N-05 additions | Optional; not approved | Explicit feature/milestone approval; do not confuse existing accessibility obligations with new options |
| Technical remediation/verification register | Incorporated for tracking | Evidence-based applicability and closure; behavior changes, new dependencies/forks and unresolved alternatives require review |

Order for the next implementation-capable session: (1) resolve approval-dependent definitions and review the evidence delta; (2) audit M1 review metadata/repeated-exposure policy and numerical boundaries without rewriting history; (3) finish safe, authorized, resumable M2 plus measured embeddings and real-corpus/license evidence; (4) settle Arabic alignment before that M3/M7 path; (5) implement remaining milestones with their linked corrective tests. Independent work may proceed where it does not assume an unapproved decision. Stay in Plan mode until the user switches agents; this revision authorizes no code, migration, dependency or model download by itself.
