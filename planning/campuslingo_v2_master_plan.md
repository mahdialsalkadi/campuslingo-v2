# CampusLingo v2 — Unified Master Plan & Authoritative Engineering Specification
**(Complete Unabridged Specification v1.8.1 — Consolidating Master Plan v1.5–v1.8, ADR-002, ADR-003, ADR-004, CORR-001 & CL2-PATCH-001)**

**Document ID:** `CL2-MASTER-UNIFIED-v1.8.1`
**Version:** 1.8.1 (Unabridged Single Canonical Specification — SPEC FROZEN)
**Patch:** `CL2-PATCH-001` (`ED-1..ED-4`, editorial only; no approved value changed)
**Date:** 2026-11-12
**Status:** **APPROVED & FROZEN — Single Authoritative Engineering & Product Specification**
**Target Root:** `/home/mahdi/campuslingo-v2`
**Supersedes in Full:** `campuslingo-v2-master-plan.md` v1.0–v1.8, `CL2-ADR-004` v1.1, `CL2-ADR-003` v1.1, `CL2-ADR-002`, `CL2-CORR-001` (`docs/corrective-addendum-v1.md`), and the original blueprint (`/home/mahdi/Downloads/campuslingo_v2_engineering_blueprint.md`).

---

## 0. Document Authority, Superseded Clauses Registry & Unabridged Scope Guarantee

Because this document declares **full supersession** (`Zero External Cross-Lookup Required`), **every section, parameter table, fairness rule, crash-recovery protocol, and corrective register (`S-01..S-25`, `A-01..A-07`, `P-01..P-07`, `O-01..O-08`) is embedded in full below with its original canonical IDs preserved**.

### 0.1 Explicit Registry of Superseded Clauses (Do Not Implement Legacy Wording)

| Legacy Source | Superseded Legacy Clause (DO NOT USE) | Active Authoritative Rule in Unified v1.8.1 |
|---|---|---|
| Blueprint §2.2 / v1.5 §11.1 | Unstructured asynchronous card deployment vs. synchronous round ambiguity | **Fixed-Slice Synchronous Combat Rounds with Full-Window Multi-Blitz Chaining (§10.4 & §11.1):** 90s duels run in server-synchronized round slices (`W_round_cut`); chaining a bonus `AXIOM / QUICK_BLITZ_CHAIN` (`W_base = 4.0s`) requires `remaining_slice_time >= W_personal_cut(p, QUICK_BLITZ_CHAIN)` and a distinct card; Sudden-Death tail threshold scales dynamically with `max_M`. |
| `ADR-003` §2.7 | Pairing ranked `1v1` players only within the same accommodation multiplier tier | **Unified Queue + Split Clock (§10.4):** All multipliers share one matchmaking queue; `W_round_cut = max(W_personal_cut(a), W_personal_cut(b))` is shared while grading/overcharge use `W_personal_eff(p)` exclusively. |
| `ADR-004` §6.2 / v1.6 §6.3 | Normalizing duel points by `8 / rounds_offered` (which erased speed/productivity differences) | **Window-Expansion Normalization `round_comp_bps(r)` (§10.6):** Each round $r$ scales by `clamp(10000 * W_round_cut(r) / W_cut_at_M1(r), 10000, 20000)`—compensating accommodation expansion while rewarding higher round throughput. |
| `ADR-003` §1.2 | Single `protected(u,t)` suppressing both raids and prerequisite penalty propagation | **Split Predicates (§9.5):** `raid_immune` includes `POST_RAID_COOLDOWN`, `RESUME_GRACE`, `DORMANT`; `propagation_exempt` **excludes** all three. |
| `ADR-003` §1.3 / v1.6 §5.3 | Dashboard glance starting all grace timers; `course_seen_at IS NULL` granting infinite grace on abandoned courses; `min(node_engaged, course_seen+7)` ending grace before engagement | **Bounded Notice + Engagement-Anchored Grace (§9.2):** `course_seen_at` bounded by `effective_notice_anchor + 7d`; `grace_start = node_engaged_at ?? (course_seen_at + 7d)`; `grace_end = min(grace_start + grace_hours, effective_notice_anchor + 14d)` (0h if `grace_start >= grace_end`); non-overlapping `user_node_grace_ledger`. |
| v1.7 §9.5 (`NEW_PLAYER_BUBBLE`) | `"48h from account creation OR eligible_transition_count < 20"` (which granted eternal immunity to players stopping at 5 reviews) | **Strict `AND` Conjunction (§6.2 & §9.5):** `in_new_player_bubble := (now < new_player_bubble_until) AND (eligible_transition_count < 20)` everywhere. |
| `ADR-002` / `ADR-003` §1.1 | `reps_min == 0` forcing brownout regardless of `E_u` | **Integrity-Only Brownout (§9.3):** Brownout depends strictly on `e_u_ticks` (`< 6950` enter, `>= 7050` exit, `< 7000` initial). `reps_min == 0` caps visual tier at Tier 1 only. |
| `ADR-003` §2.8 / v1.5 §10.7 | Floating-point `score_v1` or `i64` overflow on `1e20`; absolute `ln(1 + delta_s)` clan points; mid-week joiners scoring with frozen small divisor | **`score_v2` in `i128 -> i64` (§10.6) & Relative Clan Gain with Frozen Weekly War Roster (§11.4).** |
| `ADR-003` §2.5 | `TOO_FAST` voiding deployment with zero energy cost and no per-match cap | **Penalized `TOO_FAST` (§10.3):** Consumes deployed energy and is capped at **2 per match**; the 3rd+ `TOO_FAST` grades as **`Again = 1` + grants opponent counter-strike window**. |
| `ADR-003` §4.1–§4.4 | `> 75%` page area scan rule; page-level vector path count; Unicode-only math check; `RLIMIT_AS` memory limit; PDF-only egress rule | **Multi-Label Region Pipeline: Morphological Vector Clustering (`path_ink_ratio >= 0.05`) + Bounded Raster Crop (`4%..85%` area) + Font-First Math + cgroup v2 + Derivative Egress Gate (§7.1 & §7.4).** |

### 0.2 Unified Activity & Engagement Glossary (Single Canonical Terminology)

All subsystems use these three exact server-recorded timestamps:
1. **`course_opened_at(user, course)`:** Timestamp when the authenticated user opens the course dashboard or world map in the foreground (background token refresh and push receipt never count).
2. **`node_engaged_at(user, node, version)` (`challenge_presented_at`):** Timestamp when the server first delivers a challenge pod for **any card belonging to `node`** after `version` activation (merely rendering the node on the world map does not count).
3. **`last_qualifying_study_at(user)` (`return_study_at` when transitioning out of `DORMANT`):** Initialized to **`users.created_at` (`NOT NULL DEFAULT now()`)** upon account creation, and updated whenever the user completes **at least one `fsrs_eligible` card transition OR at least one completed match/raid (`LIVE_1V1`, `GHOST_1V1`, `ASYNC_BASE_RAID`, `REVENGE_RAID`, or `BOSS_RAID`)**. Merely opening the dashboard never updates `last_qualifying_study_at`.

### 0.3 Specification Freeze (v1.8.1)

This specification is **FROZEN**. Further review targets code and gate evidence (`G-1`, `G-2`, `G-4`, `G-6b`, `G-7`), not the specification text.

1. Ambiguities discovered during implementation are resolved as **implementation decisions** logged in `docs/implementation-decisions.md` (one line: date, question, decision, rationale). They do not reopen this document and do not require a new ADR.
2. Renumbering canonical IDs (`S-*`, `A-*`, `P-*`, `O-*`, `F-*`, `G-*`, `D-*`, `R-*`) and compressing or dropping sections are **prohibited**. These were the root cause of the v1.4, v1.6, and v1.7 scope regressions.
3. Escalation boundary: if a resolution would change an approved numeric contract, formula, predicate, cap, or milestone ownership, it is **not** an implementation decision. It requires an explicit spec revision with a superseded-clause entry in §0.1.
4. Completion is defined by gate evidence (`PASS` / `FAIL` / `BLOCKED`), never by the absence of further textual observations.

---

## 1. Sources, Precedence, Working Boundaries & Rust/WASM Architecture

* **Original Historical Blueprint:** `/home/mahdi/Downloads/campuslingo_v2_engineering_blueprint.md` (superseded by this document).
* **Canonical Project Root:** `/home/mahdi/campuslingo-v2`.
* **Read-Only Reference Project:** `/home/mahdi/campus-lingo` — never modify its files, database, configuration, or running services. Do not migrate its users or database by default.
* **Private Evaluation Corpus:** `/home/mahdi/UTM_eLearning` — never publish or export off-host, **including any derived artifacts** (§7.4, M8).
* **Working Plan Copy:** `/home/mahdi/.opencode/plan/campuslingo-v2-master-plan.md`.
* **Recorded Evidence Files:** `docs/m0-status.md`, `docs/m1-status.md`, `artifacts/benchmarks/native-wasm-parity.json`, migrations, grants, and integration tests.

### 1.1 Approved Rust/WASM Architecture & Single-Model Residency

Backend API, authoritative 90-second WebSocket server, matchmaking, [FSRS-6](https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm/5bf70e459e45152d40988c670fa7c4625b5e8577) engine, ingestion orchestrator, and background job worker are implemented in **Rust** ([`axum`](https://github.com/tokio-rs/axum), [`tokio`](https://github.com/tokio-rs/tokio), [`sqlx`](https://github.com/launchbadge/sqlx), `serde`). The shared memory and combat-validation crate uses the official `fsrs` crate ([`open-spaced-repetition/fsrs-rs`](https://github.com/open-spaced-repetition/fsrs-rs)), explicitly configured for **FSRS-6** (`FSRS6_DEFAULT_PARAMETERS`), compiled to `wasm32-unknown-unknown` for the React/[Capacitor](https://capacitorjs.com/) client. Crate release numbers (`fsrs = 7.0.0`) must never be confused with algorithm versions (`FSRS-6`).

AI models run as separate out-of-process workers via local `llama-server` HTTP. Neural TTS uses [`ort`](https://github.com/pykeio/ort) in an isolated Rust worker. **Strictly one ingestion model is resident at a time** across GGUF and ONNX stages (`load -> execute -> persist -> unload -> verify OS memory release`). Python is restricted to offline M8 training and isolated CLI evaluation/oracle scripts (`tools/eval/`); it is prohibited in the production server and ingestion workers.

Rust online server RSS target is **`< 50 MiB`**, measured separately from PostgreSQL, Redis, model processes, and the ONNX worker (`23.57 MiB` peak during the M1 30-learner probe). This is a benchmark target, not an automatic property of Rust.

### 1.2 Approved Precision & Integer-Tick Policy (`R-04`, `R-05`, `R-06`)

* **Floating Tolerance:** `absolute_error <= 1e-6 + 1e-6 * abs(reference)` (190 shared vectors pass; max drift $\approx 8 \times 10^{-5}$ days at $\approx 327.47$ days).
* **`R-04` / `G-3` CLOSED:** Upstream [`fsrs-rs`](https://github.com/open-spaced-repetition/fsrs-rs) is not forked; transitive math backends are not patched. **Server schedule days are canonical; client WASM previews are advisory.**
* **Integer Ticks (`ticks_1e4`):**
  ```rust
  pub fn ticks_1e4(v: f32) -> i64 {
      ((v as f64) * 10_000.0).round_ties_even() as i64
  }
  ```
  All discrete comparisons occur strictly in integer tick space:
  * `E_ENTER_BROWNOUT = 6950` (`0.6950`)
  * `E_INITIAL = 7000` (`0.7000`)
  * `E_EXIT_BROWNOUT = 7050` (`0.7050`)
  * `E_APEX = 8500` (`0.8500`)
  * `S_PROTO = 50_000` (`5.0` days)
  * `S_APEX = 210_000` (`21.0` days)
  Quantization is **never** fed back into `fsrs-rs`. Non-finite inputs (`NaN`/`Inf`) map immediately to `node_state = INVALID`.

---

## 2. Confirmed Product and Delivery Decisions

| Topic | Approved Decision |
|---|---|
| **Product direction** | Mobile-first competitive 3D learning strategy game; fixed shell + lecture-driven customization slots |
| **Overall scope** | Entire blueprint incl. bosses, clans, optional asset-generation evaluation, post-training, retention study |
| **Delivery** | Quality-gated milestones M0–M8; no fixed deadline or unsupported calendar estimates |
| **Intermediate builds** | Playable internal/consented builds allowed; never replace full scope |
| **Web** | Responsive mobile + tablet + desktop browser |
| **Android** | [Capacitor](https://capacitorjs.com/) app built on Linux, tested on the user's physical Android phone |
| **iOS v1** | Installable Safari PWA (Add to Home Screen) + Capacitor config/scaffold; no `.ipa`, Xcode build, or signing |
| **Native iOS later** | Future macOS/CI workstream; no Mac available locally |
| **Languages** | English first (M0–M6); Arabic expansion M7; Chinese not required |
| **Course sharing** | Public and private, with explicit authorization and publication permissions |
| **Runtime backend** | Rust ([`axum`](https://github.com/tokio-rs/axum), [`tokio`](https://github.com/tokio-rs/tokio), [`sqlx`](https://github.com/launchbadge/sqlx), `serde`); no Python application service |
| **Shared engine** | Official [`fsrs-rs`](https://github.com/open-spaced-repetition/fsrs-rs) (FSRS-6) + combat/sandbox validation compiled to WASM |
| **Inference** | Local models only; `llama-server` HTTP + [`ort`](https://github.com/pykeio/ort) TTS worker; no hosted APIs without new approval |
| **Model residency** | One ingestion model resident; load → execute → persist → unload → **verify release** |
| **Generation** | [`Qwen/Qwen3.5-4B`](https://huggingface.co/Qwen/Qwen3.5-4B) (`Q4_K_M`), two sequential GBNF passes per lease |
| **Voice/subtitles** | `CLAUSE_CONCAT` for EN ([`Kokoro-82M`](https://huggingface.co/hexgrad/Kokoro-82M)) and AR ([`Piper ar_JO-kareem-medium`](https://huggingface.co/rhasspy/piper-voices/tree/main/ar/ar_JO/kareem/medium)); **clause-synchronized captions** |
| **Hosting** | Existing AMD BC-250 hosts backend, PostgreSQL, Redis, local ingestion |
| **Load target** | 30 concurrent players incl. 15 simultaneous live duels; 30-player league bracket is separate |
| **Co-op** | Async shared-HP and live lobby co-op bosses |
| **Clan wars** | Weekly study-group races on successful raids + validated FSRS-6 relative stability gains (frozen weekly roster) |
| **Assets** | Verified CC0 [Kenney](https://kenney.itch.io/)/[Quaternius](https://quaternius.com/) + procedural Three.js meshes; free assets only |
| **Fairness** | All eight rules in §10.1 approved, incl. refined reporting threshold |
| **Physical testing** | Local browser + Android phone; iOS checks only with evidence |

---

## 3. Read-Only Findings and Hardware Envelope

### 3.1 Reference Application (`/home/mahdi/campus-lingo`)
Next.js 14/React app with challenge components, classroom features, sharing, notifications, PWA files; FastAPI backend; PostgreSQL 16 + `pgvector`; Redis; PDF text/raster extraction; structured generation with regression tests; **custom FSRS-4.5** scheduler and Ebbinghaus counter; **1536-d** embeddings; basic in-process WebSocket quiz arena; password verification **but also email-only identity and caller-supplied user IDs** (unacceptable for competitive auth); no GLB/GLTF assets found.

Useful references: `services/core-ai/ingestion/parser.py`, `llm/generator.py`, `schema.py`, `arena/rooms.py`, `tests/`, `apps/web/components/`, `packages/database/schema.sql`. **Do not reuse** old auth, scheduling assumptions, reward logic, bootstrap DDL, in-memory jobs, or multiplayer state management.

### 3.2 Hardware Snapshot (Observations, Not Benchmarks)

| Item | Observation |
|---|---|
| **CPU** | AMD BC-250, 8 cores / 16 threads |
| **GPU** | AMD Cyan Skillfish / BC-250 |
| **Linux-visible RAM** | $\approx 11\text{ GiB}$ |
| **GPU reservation** | $4.0\text{ GiB}$ via DRM sysfs |
| **GTT aperture** | $\approx 11.4\text{ GiB}$ — **not** an independent physical pool |
| **Swap** | $\approx 15\text{ GiB}$ configured with existing usage; not a working-set substitute |

Never sum overlapping unified-memory figures; model file size $\ne$ runtime memory. The blueprint's `< 6.5 GB` ingestion target is unverified.

### 3.3 Evaluation Corpus (`/home/mahdi/UTM_eLearning`)
55 PDFs at inspection (Python, discrete mathematics, data management, ML mathematics). Digital text extractable in Python Ch.1; Ch.3/Ch.4 low-text with embedded images; Set Theory image-based; Logic contains operators/formulas/negation that must survive extraction; Data Management mixed. Low-text pages are OCR candidates, **not** proof of scanning. **Cover pages alone must never classify a whole digital deck as a scan.** OCR is an early dependency. No supplied content is approved for redistribution or training by virtue of its path.

**Recorded baselines:** `fsrs = 7.0.0` (`FSRS6_DEFAULT_PARAMETERS`) native + browser WASM $\approx 363\text{ KiB}$, 190 parity vectors pass; six real-PostgreSQL security/persistence tests pass; M1 HTTP probe peak Rust RSS `23.57 MiB`; M2 four-page synthetic probe (2 digital, 1 OCR, 1 blank), 21 chunks, 21 crops, 5 duplicates, OCR model peak RSS $\approx 696.8\text{ MiB}$. **Synthetic evidence is not annotated real-corpus acceptance.**

---

## 4. Product Model and Fixed-Versus-Generated Boundary

The fixed shell owns game rules, arena/island geometry, units, towers, animation, effects, audio behavior, interface, and grading. AI never invents a game engine per lecture. Ingestion fills four declarative slots:

1. **Battle cards** — concept title, tactical summary, 250-DPI source crop, archetype, validated challenge payload.
2. **Fortress towers** — concept topology (DAG), per-student `e_u_ticks`, protection badges, tiers 0–3.
3. **Tactical pods** — declarative holograms (`W_base <= 10s` in live duels; `11–12s` in async/solo) or slide drills with clause-synchronized briefings.
4. **Chapter bosses and loot** — solo/co-op progression, trophies, cosmetics.

Modes: 90s live synchronous-round duels; labeled ghost duels with labeled bot fallback; async raids and revenge; weekly 30-player leagues across ten tiers; solo bosses; async shared-HP co-op; live lobby co-op; weekly clan wars. **The daily play loop requires no LLM call.**

---

## 5. Technical Architecture and Target Layout

```text
/home/mahdi/campuslingo-v2/
   apps/web/                 Next.js host, discovery, accounts, course pages
   apps/mobile/              Static client build + Capacitor Android/iOS config
   packages/game-client/     Shared React UI, Three.js scenes, touch controls
   packages/contracts/       Generated Rust JSON Schema + TypeScript contracts
   packages/engine/          Browser WASM bindings + native/WASM parity tests
   crates/domain/            serde contracts, FSRS-6, ticks_1e4, sandbox/combat validation
   crates/storage/           sqlx, versioned migrations, shield state, durable jobs
   crates/server/            axum/tokio API, authoritative WebSockets, match recovery
   crates/ingestion/         Rust orchestration, model lease, Poppler/Magick, llama HTTP, ort
   tools/eval/               Offline Python/TypeScript reference evaluation only
   tests/{fixtures,integration,load}/
```

Target layout; consult status reports for delivered state. Migrations live in `crates/storage/migrations/`.

**Architecture rules:** the mobile bundle must not depend on Next.js rewrites/server actions; PostgreSQL is the durable source of truth; Redis coordinates but is not the sole durable store; single authoritative combat owner; the server owns time, grading, damage, energy, shields, results, rewards — clients submit actions, not outcomes; AI workers and API are isolated with enforceable limits; private assets require authorization (**an opaque URL or tenant ID in a path is not access control**); separate namespace/ports from the reference app; external access requires HTTPS/WSS with deliberate ingress; never expose PostgreSQL, Redis, or inference ports publicly.

---

## 6. Content Contracts, Persistence, Provenance & Schema Hardening Register (`S-01..S-25`)

### 6.1 Canonical Domain Contracts (`crates/domain`)

* **`LearningUnitBundle`:** Concept metadata, 2–3 bullet digest, prerequisite tags, rapid micro-challenges (`SwipeBoolean`, `TapToFill`, `MatchMadness`, `SpotMistake`), and slide hotspot coordinates.
* **`SandboxSpec`:** Declarative JSON + restricted mathematical expression AST only; discrete slider grids exhaustively verified up to `4,096` states (`P-05`); **must enforce** measured touch-feasible target widths (`>= 44 CSS px` at `360 CSS px` viewport width — `P-06`, M3 gate).
* **`DirectorTrack`:** `STANDARD` and `ANALOGY_SIMPLIFIED` narration clauses (`35–50` words, `<= 320` chars) with semantic camera intents (`focus_role`, `shot_type`, `explode_factor`, `highlight_pulse`) bound to clause boundaries.
* **`AudioManifest`:** `alignment_method = "CLAUSE_CONCAT"`, exact sample-count durations (`samples / sample_rate`), trimmed leading/trailing silence, and explicit quality metadata.
* **Multiplayer Commands/Events:** Match ID, authenticated actor ID, protocol version, monotonic sequence number, server timestamps, and content version ID.
* **Review & Economy Audit (`S-08`, `D-02-A`):** `attempt_id`; unique `(user_id, card_id, scope_kind, scope_id)` exposure key; `rating_map_version = 2`, `window_table_version = 1`, `accommodation_version`, `network_model_version`; scheduler/scoring/content/parameter-set versions; before/after FSRS states; `fsrs_eligible`, `reward_eligible`, and explicit reason codes.

### 6.2 User-Level vs. Node-Level Protection, Dormancy History & Non-Overlapping Grace Ledger (Fixes Audit §4 & §8)

```sql
CREATE EXTENSION IF NOT EXISTS btree_gist;

-- 1. User-level protection, dormancy & accommodation state (1 row per user; S-23)
--    last_qualifying_study_at is initialized to users.created_at (NEVER NULL) so new users are never dormant.
CREATE TABLE user_protection_state (
    user_id                      UUID        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_qualifying_study_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_dormant                   BOOLEAN     NOT NULL DEFAULT FALSE,
    return_study_at              TIMESTAMPTZ,
    resume_grace_until           TIMESTAMPTZ,
    last_resume_grace_granted_at TIMESTAMPTZ, -- Enforces MIN_DORMANCY_CYCLE_DAYS = 14
    new_player_bubble_until      TIMESTAMPTZ NOT NULL, -- created_at + 48 hours
    eligible_transition_count    INT         NOT NULL DEFAULT 0, -- Bubble active while < 20 AND now < bubble_until
    consecutive_dormant_weeks    SMALLINT    NOT NULL DEFAULT 0, -- Reset to 0 on any non-dormant rollover; >=2 demotes 1 tier
    accommodation_multiplier     REAL        NOT NULL DEFAULT 1.0 CHECK (accommodation_multiplier IN (1.0, 1.25, 1.5)),
    accommodation_version        INT         NOT NULL DEFAULT 1,
    updated_at                   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 2. Historical dormancy interval log (allows exact point-in-time query was_dormant_at(user, version_activated_at); S-24)
CREATE TABLE user_dormancy_periods (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_at  TIMESTAMPTZ NOT NULL,
    ended_at    TIMESTAMPTZ CHECK (ended_at IS NULL OR ended_at >= started_at),
    EXCLUDE USING gist (user_id WITH =, tstzrange(started_at, ended_at, '[)') WITH &&)
);
CREATE INDEX idx_user_dormancy_lookup ON user_dormancy_periods (user_id, started_at, ended_at);

-- 3. Node-level timed protections ONLY (NEW_NODE_GRACE, VERSION_GRACE, POST_RAID_COOLDOWN; S-24)
CREATE TABLE user_node_protection (
    user_id         UUID        NOT NULL,
    node_id         UUID        NOT NULL,
    kind            VARCHAR(24) NOT NULL CHECK (kind IN ('NEW_NODE_GRACE', 'VERSION_GRACE', 'POST_RAID_COOLDOWN')),
    protected_until TIMESTAMPTZ NOT NULL,
    source_event_id UUID,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, node_id, kind)
);

-- 4. Permanent per-card record ensuring each card receives VERSION_GRACE at most ONCE ever (S-25)
CREATE TABLE user_card_version_grace (
    user_id            UUID        NOT NULL,
    card_id            UUID        NOT NULL,
    content_version_id UUID        NOT NULL,
    granted_at         TIMESTAMPTZ NOT NULL,
    grace_until        TIMESTAMPTZ NOT NULL,
    consumed_at        TIMESTAMPTZ,
    PRIMARY KEY (user_id, card_id)
);

-- 5. Append-only non-overlapping ledger enforcing <= 14 cumulative grace days per node per rolling 30d (S-25)
CREATE TABLE user_node_grace_ledger (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            UUID        NOT NULL,
    node_id            UUID        NOT NULL,
    content_version_id UUID        NOT NULL,
    started_at         TIMESTAMPTZ NOT NULL,
    ended_at           TIMESTAMPTZ NOT NULL CHECK (ended_at > started_at),
    EXCLUDE USING gist (user_id WITH =, node_id WITH =, tstzrange(started_at, ended_at, '[)') WITH &&)
);
CREATE INDEX idx_node_grace_ledger_window ON user_node_grace_ledger (user_id, node_id, ended_at);
```

### 6.3 Canonical `CL2-CORR-001` Schema Hardening Register (`S-01..S-22` Original Semantics Restored + `S-23..S-25`)

To preserve 100% traceability with `docs/m0-status.md`, `docs/m1-status.md`, test names, and Gate `G-7`, the **original `CL2-CORR-001` IDs (`S-01..S-22`) retain their exact original definitions**, and the new ADR-004/v1.8 protection tables are assigned `S-23..S-25`:

| ID | Canonical `CL2-CORR-001` Requirement (Preserved Unchanged) | Owning Milestone |
|---|---|---|
| **`S-01`** | **Safe migration & backfill discipline:** Add nullable column → backfill → `SET NOT NULL`; never edit applied migrations; zero startup DDL. | M1 (applied) |
| **`S-02`** | **Vector dimension & metadata migration:** `ADD COLUMN IF NOT EXISTS embedding vector(1024)` does not convert a `1536-d` column; persist `embedding_model_id`, `embedding_revision`, `embedding_dim`, `preprocess_version`; never pad or mix vectors. | M2 |
| **`S-03`** | **Scoped deduplication composite index & normalization split:** Index `(tenant_id, course_scope, language_code, archetype, modifier, generator_version)` where `NOT is_quarantined`; store normalized candidate text separately from verbatim source text. | M2 |
| **`S-04`** | **Quarantine-safe ANN (`pgvector` HNSW) index predicates:** Ensure ANN index and retrieval queries exclude quarantined `content_version_id` records. | M2 / M4 |
| **`S-05`** | **Prerequisite DAG integrity:** `CHECK (prerequisite_node_id <> dependent_node_id)`, same-course composite FK, cycle rejection trigger/check, and bounded DAG depth (`<= 16`) / closure (`<= 256`). | M2 / M4 |
| **`S-06`** | **Strict separation of shared course content vs. personal FSRS memory:** Shared `learning_nodes` / `challenges` vs. per-student `fsrs_cards` and `review_logs`; never merge students' memory rows. | M1 / M4 |
| **`S-07`** | **Versioned content publication & non-destructive archival:** Republishing creates a new `content_version_id`; archived cards leave `C_active` while preserving all historical `review_logs`. | M2 / M4 |
| **`S-08`** | **Idempotent review & exposure audit key:** `attempt_id` + unique `(user_id, card_id, scope_kind, scope_id)` exposure key + before/after FSRS states + version metadata + `fsrs_eligible` / `reward_eligible` flags and reason codes. | M1 base / M5 full |
| **`S-09`** | **Expanded `interaction_source` enum in `review_logs`:** Must include `'SOLO_RAID'`, `'PVP_DUEL'`, `'ASYNC_BASE_RAID'`, `'REVENGE_DEFENSE'`, `'PRACTICE'`, `'SOLO_BOSS'`, `'COOP_BOSS'`, `'LEAGUE'`, and `'CLAN_WAR'`. | M5 / M6 |
| **`S-10`** | **Timezone-explicit league & quota rollover:** Store `week_start_date`, user IANA timezone (`DEFAULT_TZ = 'Asia/Kuala_Lumpur'`), deterministic `04:00` user-local rollover, and partial-bracket/tie rules. | M6 |
| **`S-11`** | **Versioned `node_simulations`:** Enforce `UNIQUE (node_id, content_version_id)` so updating or quarantining a simulation never overwrites historical versions or breaks in-flight sessions. | M3 |
| **`S-12`** | **`SandboxSpec` range & non-pre-solved constraints:** Enforce `min_val < max_val`, `initial_val` inside bounds, `target_min <= target_max`, `initial_val` not pre-solved at tick 0, and a persisted valid `solution_witness`. | M3 |
| **`S-13`** | **Version-scoped reporting & anti-brigading quarantine:** Deduplicate `node_reports(user_id, node_id, content_version_id)`, verify enrollment and reporter independence, and quarantine `content_version_id` after $\ge 3$ eligible reports or harness failure. | M4 |
| **`S-14`** | **Durable match lifecycle & periodic snapshots:** Persist `MATCH_STARTED` before tick 0, 10-second state snapshots, and `settlement_idempotency_key`. | M5 |
| **`S-15`** | **Bounded `replay_events`, retention TTL & ghost-selection index:** Cap `replay_events` payload size/count, expire stale unreferenced replays via TTL, and index eligible ghost replays by `(course_id, content_version_id, league_tier)`. | M5 |
| **`S-16`** | **Append-only `reward_transactions` ledger & atomic settlement:** Immutable trigger on `reward_transactions`; match/raid/boss outcome and reward writes commit in one SQL transaction. | M1 base / M6 full |
| **`S-17`** | **Boss HP concurrency & chest claim idempotency:** Atomic SQL HP decrement, zero overkill reward duplication, and unique `(user_id, boss_instance_id, reward_tier)` claim constraint. | M6 |
| **`S-18`** | **Notification preferences, quiet hours & delivery ledger:** Deduplication key `(user_id, notification_kind, local_quota_date)` enforcing `MAX_PUSH_NOTIFICATIONS_PER_DAY = 2` and `22:00–08:00` local deferral to `08:00`. | M6 |
| **`S-19`** | **FSRS-6 parameter set persistence & float round-trip discipline:** Versioned 21-weight vector table `user_fsrs_parameters` (including trainable `decay`) + documented `f32`/`REAL` restart round-trip fixtures. | M4 |
| **`S-20`** | **Validated 18 `(ArchetypeType, Modifier)` pairs, deterministic `card_elixir_cost`, and collision-free `grid_coords`:** Derive `card_elixir_cost` (`1..=6`) deterministically from archetype/modifier complexity and enforce unique `(course_id, grid_coords)` per active version. | M3 / M4 |
| **`S-21`** | **Tenant-consistent composite foreign keys & RLS:** Enforce composite `(tenant_id, id)` FKs across multi-tenant tables and session-verified RLS; caller headers never prove authorization. | M1 (applied) |
| **`S-22`** | **Built-in `gen_random_uuid()` & JSONB `schema_version`:** Use PG16 `gen_random_uuid()` everywhere and enforce explicit `schema_version` fields on all JSONB payloads. | M1 / M3 |
| **`S-23`** | **User-level protection & dormancy table (`user_protection_state`):** Persist user-wide dormancy, `last_qualifying_study_at` (`DEFAULT now()`), `last_resume_grace_granted_at`, `new_player_bubble_until`, `eligible_transition_count`, and `consecutive_dormant_weeks`. | M4 |
| **`S-24`** | **Historical dormancy intervals (`user_dormancy_periods`) & node timed protections (`user_node_protection`):** Enable exact point-in-time `was_dormant_at(user, t)` lookups and multi-kind node protection storage. | M4 |
| **`S-25`** | **One-time card grace (`user_card_version_grace`) & non-overlapping rolling ledger (`user_node_grace_ledger`):** Enforce once-ever per-card version grace and `<= 14d / 30d` non-overlapping cumulative node grace. | M4 |

---

## 7. Ingestion, Multi-Label Region Pipeline & Local-Model Lifecycle

### 7.1 Hardened Extraction, Containment & Multi-Label Region Pipeline (M2 — Fixes Audit §7)

Rust orchestration of local Poppler (`pdfinfo`, `pdftohtml`, `pdftoppm`, `pdfimages`), ImageMagick WebP, Tesseract geometry, and [`mradermacher/PaddleOCR-VL-1.6-GGUF`](https://huggingface.co/mradermacher/PaddleOCR-VL-1.6-GGUF) text via leased `llama-server`. **`PyMuPDF` is prohibited as a production worker.**

1. **Process Containment (`A-05`):** Each helper runs in its own process group (`process_group(0)`). Any `pre_exec` hook must be async-signal-safe and allocation-free. On timeout or drop, `SIGKILL` the entire process group and reap all children.
2. **Resource Limits (cgroup v2 over `RLIMIT_AS`):** Enforce memory via **cgroup v2 `MemoryMax`**; use `RLIMIT_CPU` and `RLIMIT_FSIZE` for CPU/file bounds; retain `RLIMIT_AS` only as a generous backstop. Configure ImageMagick `policy.xml` caps, Ghostscript `-dSAFER`, and explicit `pdfinfo` timeouts.
3. **Fail-Closed Pipes:** Drain `stdout` and `stderr` concurrently (`tokio::join!`) with `.take(MAX_BYTES + 1)`; hitting `MAX_BYTES + 1` aborts with `OutputOverflow`, never silent truncation.
4. **Coordinates:** Normalize `/Rotate` and `CropBox` vs. `MediaBox` before mapping 72-DPI `pdftohtml` boxes to 150-DPI analysis rasters; emit an overlay PNG verification artifact. Re-render selected crops at **250 DPI `.webp`**.
5. **Decoration Exclusion & Multi-Label Region Action Pipeline (Fixes Audit §7.1 & §7.2):**
   * **Step A — Unconditional Single-Page Background Exclusion:** Uniform full-page background fills and low-ink-variance full-page border rectangles (`>= 85%` of page area with interior ink variance `< threshold`) are excluded **unconditionally on all PDFs regardless of page count**.
   * **Step B — Multi-Page Recurring Template Exclusion:** When `deck_page_count >= 5`, normalized path geometry (`coords + fill/stroke`) or image `XObjects` (`xref` or `pHash + position`) recurring on **`>= 60%` of pages AND `>= 3` pages** are excluded as slide-master decoration.
   * **Step C — Multi-Label Region Action Set (Not Mutually Exclusive Whole-Page Labels):** Each page evaluates an independent set of region-level actions (`PageActions`), allowing a single hybrid page to crop vector diagrams, crop embedded raster figures, extract digital text, and OCR math/scanned regions simultaneously:
     1. **`ACTION_OCR_FULL_PAGE_SCAN`:** Triggered if a single raster image covers `>= 85%` of `page_area` (or `raster_ratio >= 0.60` with `digital_chars < 80` and zero vector `candidate_cluster`). Runs full-page `PaddleOCR-VL` + layout sub-region isolation first, then crops isolated diagram sub-regions (`0.04 <= sub_area / page_area < 0.85`) at 250-DPI `.webp`.
     2. **`ACTION_CROP_VECTOR_CLUSTERS`:** Render non-template vector paths to a 150-DPI binary mask $M_{\text{vec}}$, apply morphological closing with radius `1.5%` of page width, and extract 8-connected components. Crop at 250-DPI `.webp` every component $C$ satisfying:
        ```text
        segments(C) >= 25
        AND 0.04 * page_area <= bbox_area(C) < 0.90 * page_area
        AND non_text_ink_ratio(C) = (bbox_area(C) - area(union(text_boxes ∩ C))) / bbox_area(C) >= 0.35
        AND path_ink_ratio(C)     = ink_pixels(M_vec ∩ C) / bbox_area_pixels(C) >= 0.05
        ```
     3. **`ACTION_CROP_RASTER_FIGURES` (Excludes `>= 85%` Full-Page Scans — Audit Fix §7.1):** For every non-template embedded raster image (`XObject`) or merged raster region satisfying **`0.04 * page_area <= bbox_area < 0.85 * page_area`** and non-uniform ink variance, crop the region at 250-DPI `.webp`. (Regions `>= 85%` of page area are handled by `ACTION_OCR_FULL_PAGE_SCAN`, never cropped raw as a single giant figure).
     4. **`ACTION_OCR_MATH_OR_GARBLED_REGIONS`:** Triggered on any text/formula block using math fonts (`CMMI`, `CMSY`, `CMEX`, `MSAM`, `MSBM`, `Symbol`, `MT Extra`, `Euclid`, `LatinModernMath`, `MathJax`), high Unicode math operator density, or `garbled_glyph_ratio > 0.08`. Preserves a 250-DPI crop and extracts LaTeX via `PaddleOCR-VL`. Math boxes are never erased by text masks.
     5. **`ACTION_EXTRACT_DIGITAL_TEXT`:** Extracts remaining clean digital text blocks with preserved code indentation, formulas, numbers, units, and negation tokens.

### 7.2 Deduplication, Embeddings & Normalization (`S-03`/`S-04`, `P-04`)
Version normalization separately from source text; preserve critical-token order, unit/formula case, superscripts, numbers, and negation. Evaluate NFKC, Arabic diacritic/tatweel removal, and Arabic-Indic digit normalization as **language-aware candidate matching only**—destructive normalization must never authorize an unsafe merge without an exact match on `structural_fingerprint`.

### 7.3 Single-Model Lease and Two-Pass Generation
One model resident at a time (`PaddleOCR-VL`, embeddings, [`Qwen/Qwen3.5-4B`](https://huggingface.co/Qwen/Qwen3.5-4B), or [`ort`](https://github.com/pykeio/ort) TTS). Persist intermediates before switching; **verify actual resource release**. Within the Qwen lease: classification GBNF pass → Stage-3 compaction (`AXIOM_PACK`, §8.1) → generation GBNF pass → deterministic Head B binding, without reloading weights.

### 7.4 Model Candidates, Licensing & Derived-Artifact Egress Gate
Candidates: [`mradermacher/PaddleOCR-VL-1.6-GGUF`](https://huggingface.co/mradermacher/PaddleOCR-VL-1.6-GGUF) (`Q8_0` + `mmproj-f16`); [`Qwen/Qwen3.5-4B`](https://huggingface.co/Qwen/Qwen3.5-4B) (`Q4_K_M`); [`Qwen/Qwen3-Embedding-0.6B-GGUF`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF) or [`ggml-org/bge-m3-Q8_0-GGUF`](https://huggingface.co/ggml-org/bge-m3-Q8_0-GGUF); [`hexgrad/Kokoro-82M`](https://huggingface.co/hexgrad/Kokoro-82M); [`rhasspy/piper-voices`](https://huggingface.co/rhasspy/piper-voices/tree/main/ar/ar_JO/kareem/medium) (`ar_JO-kareem-medium`).
* **Egress Gate (ADR-004 §4):** No private-corpus file or **derivative artifact** (chunks, 250-DPI crops, bundles, QA pairs, embeddings, adapter or merged weights) may leave the host without recorded content-owner consent covering that specific derivative class, plus de-identification, provider no-retention/no-training terms, encryption, deletion evidence, and private weights.

---

## 8. Templates, Themes, Original `W_base` v1 Table (`ADR-003 §2.6`) & Safe Simulation Execution

Themes: `BIO_SYSTEM`, `CYBER_GRID`, `MACRO_POLIS`, `ARCANE_ARCHIVE`.

### 8.1 Authoritative `W_base` v1 Table (`window_table_version = 1` — Restored to `ADR-003 §2.6` Values + `QUICK_BLITZ_CHAIN`)

To preserve the approved pedagogical difficulty gradient and the `11.0–12.0s` async/solo tier from `CL2-ADR-003 §2.6` without silent modification:
* At `M_accommodation = 1.0`, `W_personal_eff = W_base` and `W_personal_cut = W_base + min(2.0s, 0.20 * W_base)`.
* **13 Live-Duel + Async/Solo Pairs (`W_base <= 10.0s`, `W_cut <= 12.0s` at `M = 1.0`, `<= 17.0s` at `M = 1.5`)** are eligible in all modes.
* **5 Deep-Analysis Pairs (`W_base in {11.0s, 12.0s}`)** appear in **Async Base Raids, Revenge Raids, Solo Bosses, and Practice only** (in live 90s `1v1` duels, cards carrying these 5 modifiers present their rapid slide/drill variant at `W_base = 10.0s`).
* **1 Chained Sub-Mode (`AXIOM / QUICK_BLITZ_CHAIN`, `W_base = 4.0s`)** is used exclusively when a player chains a bonus rapid prompt inside the remaining time of a synchronized duel round slice (§10.4).

| Structural Archetype | Fixed Interactive Template | Modifier | `W_base` v1 (s) | `W_cut` at `M=1.0` (s) | `W_cut` at `M=1.5` (s) | Mode Eligibility |
|---|---|---|---:|---:|---:|---|
| **`PIPELINE`** | Conduit/valve routing, bounded flow | `REVERSE_ENG` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`PIPELINE`** | Conduit/valve routing, bounded flow | `BOTTLENECK_RELIEF` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`PIPELINE`** | Conduit/valve routing, bounded flow | `FAULT_INJECTION` | `9.0` | `10.8` | `15.5` | Live `1v1` + Async/Solo |
| **`TRADEOFF`** | Coupled-core equilibrium, declarative formula | `OVERLOAD_STRESS` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`TRADEOFF`** | Coupled-core equilibrium, declarative formula | `BUDGET_CAP` | `11.0` | `13.0` | `18.5` | **Async/Solo Only** (`10.0s` drill in `1v1`) |
| **`TRADEOFF`** | Coupled-core equilibrium, declarative formula | `PARETO_BALANCE` | `12.0` | `14.0` | `20.0` | **Async/Solo Only** (`10.0s` drill in `1v1`) |
| **`SPATIAL`** | Exploded layers / slide pinpointing | `BLIND_DIAGNOSIS` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`SPATIAL`** | Exploded layers / slide pinpointing | `SURGICAL_REPAIR` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`SPATIAL`** | Exploded layers / slide pinpointing | `CROSS_SECTION_SCAN` | `9.0` | `10.8` | `15.5` | Live `1v1` + Async/Solo |
| **`CONTRAST`** | Dual-chamber comparison under stimuli | `STRESS_BENCHMARK` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`CONTRAST`** | Dual-chamber comparison under stimuli | `BLIND_CHAMBER_ID` | `9.0` | `10.8` | `15.5` | Live `1v1` + Async/Solo |
| **`CONTRAST`** | Dual-chamber comparison under stimuli | `CROSS_CALIBRATION` | `11.0` | `13.0` | `18.5` | **Async/Solo Only** (`10.0s` drill in `1v1`) |
| **`TRAP`** | Timeline fault detection | `FORENSIC_AUDIT` | `12.0` | `14.0` | `20.0` | **Async/Solo Only** (`10.0s` drill in `1v1`) |
| **`TRAP`** | Timeline fault detection | `COUNTEREXAMPLE_PROBE` | `11.0` | `13.0` | `18.5` | **Async/Solo Only** (`10.0s` drill in `1v1`) |
| **`TRAP`** | Timeline fault detection | `SAFEGUARD_PATCH` | `10.0` | `12.0` | `17.0` | Live `1v1` + Async/Solo |
| **`AXIOM`** | Low-overhead speed rounds | `SPEED_BLITZ` | `6.0` | `7.2` | `10.8` | Live `1v1` + Async/Solo |
| **`AXIOM`** | Low-overhead speed rounds | `COROLLARY_CHAIN` | `8.0` | `9.6` | `14.0` | Live `1v1` + Async/Solo |
| **`AXIOM`** | Low-overhead speed rounds | `BOUNDARY_TEST` | `8.0` | `9.6` | `14.0` | Live `1v1` + Async/Solo |
| **`AXIOM` (Sub-Mode)** | In-slice bonus blitz chain (§10.4) | `QUICK_BLITZ_CHAIN` | `4.0` | `4.8` | `7.2` | Live `1v1` Multi-Blitz Chain Only |

**`AXIOM_PACK` (Restored in Full):** Stage-3 compaction merges 2–4 consecutive `AXIOM` chunks into a single `AXIOM_PACK` card. In live 90s `1v1` duels an `AXIOM_PACK` presents **exactly one sub-prompt per synchronized round slice**, using `AXIOM / SPEED_BLITZ` (`W_base = 6.0s`, `W_cut = 7.2s` at `M = 1.0`). In async and solo modes it presents **up to three back-to-back sub-prompts**. Every sub-prompt is an independent card exposure bound by the `S-08` uniqueness key `(user_id, card_id, scope_kind, scope_id)` and must reference a **distinct `card_id`**. `AXIOM_PACK` is distinct from `AXIOM / QUICK_BLITZ_CHAIN`: the former is a generated card carrying multiple sub-prompts; the latter is an in-slice bonus prompt granted under §10.4. Mapping of `AXIOM_PACK` sub-prompts onto Async Raid Stage-3 drill slots is recorded in `docs/implementation-decisions.md` (`DEC-002`, owner M6).

*(Mandatory Verification Check for `G-6b`: Before freezing `crates/domain/src/windows.rs`, assert 100% numeric identity against the self-contained fixture `tests/fixtures/w_base_v1.json` — all 18 base pairs plus the `QUICK_BLITZ_CHAIN` sub-mode, under `window_table_version = 1`. Cutoffs are asserted in **integer milliseconds** derived from the approved formula `W_cut = W_eff + min(2000ms, W_eff / 5)`, `W_eff = W_base × M`, for `M ∈ {1.0, 1.25, 1.5}`. The archived `CL2-ADR-003 §2.6` may be used as an optional historical cross-check but is never a gate dependency.)*

### 8.2 Simulation & Asset Security
Validated JSON only, never model-generated JavaScript in the normal rendering path; restricted expression tree (`A-07`) with operator allowlist, domain checks, max AST depth `<= 16`, max nodes `<= 64`, and finite-output verification; no `eval` or `Function`. Discrete slider grids are exhaustively verified up to `4,096` states (`P-05`) with a stored solution witness (`S-12`). Optional iframe experiments remain **DISABLED (`A-03`)** unless separately approved, and GLB/GLTF assets pass strict structural/resource validation (`A-04`: **per-asset `<= 8,000` triangles, instanced unit/tower `<= 2,500` triangles, total rendered scene `<= 15,000` triangles**).

---

## 9. FSRS-6, Durable Shields, Protection, and Tiers

### 9.1 Equations
```text
factor(d_c) = 0.9^(-1 / d_c) - 1
R_c(t)      = (1 + factor(d_c) * delta_t_c / S_c)^(-d_c)
I_u(t)      = mean(R_c(t) for c in C_scored(u, t))
E_u(t)      = I_u(t) * product(1 - w[p,u] * max(0, (0.70 - E_p(t)) / 0.70)
                               for p in prerequisites_of(u) where NOT propagation_exempt(p, t))
```
Evaluated in topological order; reject cycles and cross-course dependencies; bound DAG depth (`<= 16`) and closure size (`<= 256`).

### 9.2 Scoring Set, Historical Dormancy Lookup & Non-Negative Version Grace (Fixes Audit §4 & §8)
`C_active` = published, non-archived cards of the active content version (`INACTIVE` if empty). Unreviewed unlocked cards contribute `R_c = 0`. `S_u = 0` if `reps_min == 0`, else `min(S_c)`.

When a new `version` activates on a course at `version_activated_at`:
* **Version-Transition Reset Rule (Audit Fix §4.ج):** Activating a new `content_version_id` resets `course_seen_at` and `node_engaged_at` to `NULL` for the new version; **`unlocked_at` NEVER resets**. Archived cards leave `C_active` while retaining all `review_logs` (`S-07`).
* **Historical Dormancy Check (`was_dormant_at` — Audit Fix §4.أ):** Queried from `user_dormancy_periods`:
  ```sql
  SELECT ended_at FROM user_dormancy_periods
  WHERE user_id = :u AND started_at <= :version_activated_at
    AND (ended_at IS NULL OR ended_at > :version_activated_at)
  LIMIT 1;
  ```
  If a matching row exists, `effective_notice_anchor = ended_at ?? +∞` (until the user returns from that dormancy); otherwise `effective_notice_anchor = version_activated_at`.
* **Grace Start, End & Non-Negative Guard (Audit Fix §8.3):**
  ```text
  course_seen_at(user, course, version) =
      min(first_course_open_after_activation ?? +∞,
          effective_notice_anchor + VERSION_NOTICE_DAYS (7 days))

  grace_start(u) = node_engaged_at(user, u, version)
                   ?? (course_seen_at(user, course, version) + 7 days)
  grace_hours(u) = clamp(72 + 12 * |new_ungraced_cards_in_node(u)|, 72, 168)
  raw_end(u)     = min(grace_start(u) + grace_hours(u),
                       effective_notice_anchor + VERSION_ABSOLUTE_CAP_DAYS (14 days))
  ```
  If `grace_start(u) >= raw_end(u)`, **no `VERSION_GRACE` is granted (`0h`)**. Otherwise, `raw_window = [grace_start(u), raw_end(u)]` is clamped against the remaining allowance in `14 days - used_grace_sec_in_last_30d` from `user_node_grace_ledger` (which enforces non-overlapping intervals via `EXCLUDE USING gist`), and each new card is recorded once in `user_card_version_grace`.
* `C_scored(u, t) = C_active(u) \ C_graced(u, t)`. If `C_scored = ∅` with `C_active != ∅`, Tier 0 semantics apply.

### 9.3 Brownout Hysteresis (`R-06-A`, Amended)
```text
Initial : e_u_ticks < 7000
Enter   : e_u_ticks < 6950
Exit    : e_u_ticks >= 7050
```
`reps_min == 0` **does not** force brownout; it affects visual tier only. Propagation uses parents' continuous `e_u_ticks`. *(The `6950/7050` hysteresis band is intentionally distinct from the flat `R_ticks < 7000` recovery bonus threshold in `score_v2`).*

### 9.4 Total Tier Table (First Match Wins, Over `C_scored`)
1. **Tier 0** — `reps_sum == 0` (or `C_scored = ∅` with `C_active != ∅`)
2. **Tier 1** — `reps_min == 0` OR `is_brownout` OR `s_u_ticks < 50_000`
3. **Tier 3** — `s_u_ticks >= 210_000` AND `e_u_ticks >= 8500` AND NOT `is_brownout`
4. **Tier 2** — all other valid `ACTIVE` states

### 9.5 Split Protection Predicates & Strict `AND` Bubble Rule (Fixes Audit §2)
```text
NEW_PLAYER_BUBBLE(user, t) := (t < user.new_player_bubble_until)
                              AND (user.eligible_transition_count < 20)

raid_immune(u, t)          := NEW_NODE_GRACE | VERSION_GRACE | NEW_PLAYER_BUBBLE
                              | POST_RAID_COOLDOWN | RESUME_GRACE | DORMANT
                              | Tier 0 | INACTIVE | INVALID

propagation_exempt(u, t)   := NEW_NODE_GRACE | VERSION_GRACE | NEW_PLAYER_BUBBLE
                              | Tier 0 | INACTIVE | INVALID
```
`NEW_NODE_GRACE`: 72h from `unlocked_at` while `e_u_ticks < 7050`. `POST_RAID_COOLDOWN`: 6h on the targeted node.

### 9.6 Study-Anchored Dormancy & Recompute
* `is_dormant = (now - last_qualifying_study_at) >= 7 days` (where `last_qualifying_study_at` is initialized to `users.created_at` so new accounts are never dormant). Entering dormancy inserts an open interval `(user_id, started_at, NULL)` into `user_dormancy_periods`; completing a qualifying study action closes `ended_at = return_study_at`.
* On `return_study_at`, grant `RESUME_GRACE` (`72h`) **iff** `last_resume_grace_granted_at IS NULL` OR `return_study_at - last_resume_grace_granted_at >= 14 days` (`MIN_DORMANCY_CYCLE_DAYS = 14`).
* **Recompute:** Dashboard reads serve cached shield state immediately and enqueue async recompute when `now - computed_at >= 15 min` (with an "updating" indicator).

---

## 10. Grading, Eligibility, Fairness, Clocks, and `score_v2`

### 10.1 Approved Fairness Rules (All Eight — Restored in Full)
1. **Normalize ranked combat power** — tournament-standardized levels in ranked `1v1`; learning history must not make a new student unable to compete.
2. **Raids opt-in and bounded** — private courses require explicit opt-in; new-player protection, cooldowns, capped losses, anti-farming. Vulnerability alone is not consent.
3. **Label ghosts and bots honestly** — recorded classmates are labeled ghosts; cold-start bots are labeled; use eligible permissioned replays with defined state adaptation (`F-03`).
4. **Protect memory privacy** — scanners/discovery never reveal per-student recall/stability history; publish only allowlisted competition fields (`F-04`).
5. **Reports without griefing** — first report triggers server-controlled local replacement/refund; quarantine a `content_version_id` after $\ge 3$ independent enrolled reports or a harness failure (`S-13`); never delete memory records.
6. **Separate practice from eligible reviews** — repeated exposures/requests/re-grades/mastered prompts must not inflate mastery, stability gains, or rewards.
7. **Uploads private by default** — publication and training use require explicit permission; UTM PDFs and all derivatives are private.
8. **Gate procedural generation separately** — prefer offline static GLB; generated code/assets are untrusted.

### 10.2 Eligibility (`D-02-A`)
`fsrs_eligible` requires: active enrollment + owned personal memory state; mode not `SPECTATOR`/`REPLAY`/`TUTORIAL_SANDBOX`; server timestamps with `0.15s <= elapsed <= max_allowed_window`; zero prior transitions for `(user_id, card_id, scope_kind, scope_id)` in the current match/session; and either **`last_fsrs_transition_at IS NULL` (first-ever review of a new card)** or `now - last_fsrs_transition_at >= 30 min`. For a new card (`reps_before == 0`), `R_before = 0.0` (`R_ticks = 0`).

`reward_eligible = authorized_competitive_mode AND fsrs_eligible AND rewarded_exposures_today(user, card) < 3 AND rewarded_exposures_today(user, course) < 60` (rolling over at `04:00` user-local time; practice XP capped at `100/day`).

### 10.3 Rating Precedence (`rating_map_version = 2`) & Penalized `TOO_FAST`
```text
Case 0 no mutation : VOID_DELIVERY
                     | VOID_TRANSPORT_LOSS (first 2 per match; consumes energy)
                     | TOO_FAST (elapsed < 0.15s; first 2 per match; consumes energy)
Case 1 Again = 1   : TIMEOUT | ABANDONED_AFTER_REVEAL | not correct
                     | 3rd+ VOID_TRANSPORT_LOSS in match | 3rd+ TOO_FAST in match
                     (opens opponent Counter-Strike window in live 1v1)
Case 2 Hard  = 2   : correct AND (hint_used OR submits > 1 OR partial_credit
                                  OR L_net > W_personal_eff)
Case 3 Easy  = 4   : correct AND NOT hint AND submits == 1 AND NOT partial
                                  AND L_net <= 0.50 * W_personal_eff
                                  (triggers 2.0x Overcharge in combat)
Case 4 Good  = 3   : remaining correct outcomes
```
`L_net = max(0.15s, (t_recv - t_send) - min(0.30s, RTT_ewma / 2))`. Per-user void-rate and `TOO_FAST` telemetry with automated abuse-review flags are mandatory.

### 10.4 Synchronous Duel Rounds, Un-Truncated Multi-Blitz Chaining & Dynamic Tail Threshold (Fixes Audit §5)
* `M_accommodation in {1.0, 1.25, 1.5}` (default `1.0`).
* `W_personal_eff(p) = W_base * M_accommodation(p)`
* `W_personal_cut(p) = W_personal_eff(p) + min(2.0s, 0.20 * W_personal_eff(p))`
* **In Async Raids, Solo Bosses & Practice:** `W_round_cut = W_personal_cut(p)` and `max_allowed_window = W_personal_cut(p)`.
* **In Live 90s `1v1` Duels (Synchronous Round Slices):**
  1. Each synchronized round $r$ uses a shared slice duration `W_round_cut(r) = max(W_personal_cut(a, card_a), W_personal_cut(b, card_b))`. Input stays open until `W_round_cut(r)` for both players; a correct answer after `W_personal_eff(p)` up to `W_round_cut(r)` grades **Hard** (preserving accommodation privacy).
  2. **Un-Truncated Multi-Blitz Chaining Inside a Slice (Fixes Audit §5.1):** If a player finishes a fast primary card early inside round slice $r$, they may trigger at most **1 bonus `AXIOM / QUICK_BLITZ_CHAIN` prompt** (`W_base = 4.0s`, `W_personal_cut = 4.8s` at `M = 1.0`, `6.0s` at `M = 1.25`, `7.2s` at `M = 1.5`) **if and only if**:
     * `remaining_slice_time >= W_personal_cut(player, QUICK_BLITZ_CHAIN)` (guaranteeing the chained card receives its **full, un-truncated personal window** and never suffers an unfair timeout `Again`!),
     * The player has regenerated `>= 2` elixir, AND
     * The chained prompt is for a **distinct `card_id`** not yet presented to that player in the match (`S-08`).
  3. **Accommodation-Safe 90-Second Match Boundary & Tail Phase (Fixes Audit §5.2):**
     Let `W_sudden_death_cut = max(W_personal_cut(a, AXIOM/SPEED_BLITZ), W_personal_cut(b, AXIOM/SPEED_BLITZ))` (`7.2s` at `max_M = 1.0`, `9.0s` at `max_M = 1.25`, `10.8s` at `max_M = 1.5`).
     * A normal round slice $r$ starts at elapsed match time $t_{\text{match}}$ iff `90.0s - t_match >= W_round_cut(r)`.
     * If `W_sudden_death_cut <= 90.0s - t_match < W_round_cut(r)`, a final Sudden-Death `AXIOM / SPEED_BLITZ` round opens with full window `W_sudden_death_cut` (never truncating an accommodated player's window).
     * Once `90.0s - t_match < W_sudden_death_cut`, no new challenge pod opens; the remaining tail resolves in-flight units/projectiles until `90.0s`.

### 10.5 Raid Caps (`F-01`, `F-02`) & Notification Aggregation (`F-05`)
`PLAYER_BUBBLE_HOURS = 48`, `PLAYER_BUBBLE_REVIEWS = 20`; `MAX_INBOUND_RAIDS_PER_DAY = 3`; `MAX_UNORDERED_PAIR_RAIDS_DAY = 1` (revenge counts); `MAX_OUTBOUND_RAIDS_PER_DAY = 10`; `POST_RAID_SHIELD_HOURS = 6`; `MAX_DAILY_TROPHY_LOSS_FROM_RAIDS = 45`; `REVENGE_WINDOW_HOURS = 48`, single-use.
* **Notifications (`F-05`, `S-18`):** `MAX_PUSH_NOTIFICATIONS_PER_DAY = 2` (1 aggregated raid alert `"Raided N times today"` + 1 daily digest enumerating all inbound raids). Quiet hours `22:00–08:00` user-local defer delivery to `08:00` local, deduplicated.

### 10.6 Exact `i128 -> i64` Scoring (`score_v2`) & Window-Expansion Duel Normalization

```rust
pub fn round_ties_even_points(n: i128, d: i128) -> i64 {
    debug_assert!(n >= 0 && d > 0);
    let q = n / d;
    let r = n % d;
    let doubled_r = r * 2;
    let rounded = if doubled_r > d {
        q + 1
    } else if doubled_r < d {
        q
    } else {
        q + (q % 2)
    };
    i64::try_from(rounded).expect("clamped score_v2 fits in i64")
}

pub fn compute_score_v2(
    r_ticks: i64,          // 0 if reps_before == 0, else ticks_1e4(R_before) in [0, 10000]
    reps_before: u32,
    rating: u8,            // 1..=4
    is_revenge_raid: bool,
) -> i64 {
    let r_clamped = r_ticks.clamp(0, 10_000) as i128;
    let recall_bps: i128 = if r_clamped > 9500 { 2000 } else { 10_000 };
    let mastery_bps: i128 = if reps_before == 0 && rating >= 3 { 30_000 } else { 10_000 };
    let mode_bps: i128 = if is_revenge_raid { 15_000 } else { 10_000 };
    let recovery_x1e4: i128 = if reps_before > 0 && r_clamped < 7000 && rating >= 3 {
        100_000_000 + 20_000 * (7000 - r_clamped)
    } else {
        100_000_000
    };

    let den: i128 = 10_000_i128.pow(3) * 100_000_000; // 10^20
    let raw_num: i128 = 10_i128 * recall_bps * mastery_bps * mode_bps * recovery_x1e4;
    let clamped_num: i128 = raw_num.min(30 * den);    // Clamp <= 3.0x BEFORE rounding
    round_ties_even_points(clamped_num, den)
}

/// Normalizes duel competitive contribution by round window expansion (NOT by round count),
/// preserving full productivity rewards for fast players while compensating accommodated windows.
pub fn round_comp_bps(w_round_cut_ms: u32, w_cut_at_m1_ms: u32) -> i128 {
    if w_cut_at_m1_ms == 0 {
        return 10_000;
    }
    let raw = (10_000_i128 * (w_round_cut_ms as i128)) / (w_cut_at_m1_ms as i128);
    raw.clamp(10_000, 20_000)
}

pub fn compute_duel_contribution(round_points_and_windows: &[(i64, u32, u32)]) -> i64 {
    let weighted_sum: i128 = round_points_and_windows
        .iter()
        .map(|&(pts, w_round_cut_ms, w_cut_at_m1_ms)| {
            (pts.max(0) as i128) * round_comp_bps(w_round_cut_ms, w_cut_at_m1_ms)
        })
        .sum();
    round_ties_even_points(weighted_sum, 10_000_i128)
}
```

---

## 11. Competitive Rules, Raids, Leagues, Bosses, Clan Wars & Crash Recovery

### 11.1 Live `1v1`, Ghosts, Bots & Replay Adaptation (`F-03`, `O-03`)
Server-owned 90s match, two-lane arena, continuous energy, deployment, deterministic damage/shields; final 30s `2x` energy; wrong answers open a counter-strike window. Full briefings are suppressed during the 90s clock (`<= 2.5s` non-blocking skippable HUD toast). Muting/skipping grants zero clock or energy advantage.
* **Matchmaking:** Measured server-side **4-second** deadline, then labeled recorded-classmate ghost, then labeled cold-start bot.
* **`F-03` State-Aware Ghost Adaptation:** Historical ghost actions (`card_id`, `relative_latency_ms`, `was_correct`) are re-evaluated against the **live battle state** (current energy, active shields, lane HP); never replay historical raw damage numbers blindly.
* **Numeric WebSocket Budgets (`O-03`):** Server heartbeat every `5.0s`; client pong timeout `10.0s`; reconnect grace `15.0s`; full state snapshot every `10.0s` (and on reconnect); inbound command queue bounded to `32` messages/conn (`max_payload = 4096 bytes`); sequence-gap triggers immediate state snapshot resync.

### 11.2 Snapshot Raids, Concurrent Raid Settlement Rule & Raid Clocks (`D-03` — Fixes Audit §6)
* **Snapshot Isolation & General Concurrent Cooldown Rule (Fixes 60s–6h Gap — Audit §6):**
  At raid start, the server snapshots the target node's active content version and verifies `!raid_immune(u, t_start)`. If two attackers raid the same vulnerable node concurrently, the **first** attacker to settle wins the raid normally and places the node into `POST_RAID_COOLDOWN` (`6h`).
  > **General Rule for In-Flight Concurrent Raids:** Whenever a raid attempt reaches settlement and the target node is **already in an active `POST_RAID_COOLDOWN` (or otherwise `raid_immune`)** at `t_settle`:
  > 1. The attacker's eligible FSRS memory transitions still commit (`fsrs_eligible`) and award **Practice XP only** (up to `100/day`).
  > 2. **Zero defender trophy loss, zero attacker trophy gain, and zero league/clan war points** are awarded.
  > 3. The blocked settlement **does NOT consume** the defender's daily inbound raid cap (`MAX_INBOUND_RAIDS_PER_DAY = 3`), **but DOES consume** 1 of the attacker's daily outbound raids (`MAX_OUTBOUND_RAIDS_PER_DAY = 10`).
* **3-Stage Raid Flow & Clocks (`D-03`):**
  1. **Stage 1 — Scout Briefing:** `15–20s` clause-synced briefing (skippable with zero penalty).
  2. **Stage 2 — Tactical Sandbox Hack:** Governed by `W_personal_cut(attacker)` (`W_base` up to `12.0s`, cutoff up to `14.0s` at `M = 1.0` / `20.0s` at `M = 1.5`).
  3. **Stage 3 — Three Rapid Verification Drills:** `8.0–15.0s` base window per drill.
  4. **Total Raid Session Ceiling:** `120.0s` hard server timeout from Stage 2 start.
* **Shield Restoration:** Defenders restore shields strictly by completing legitimate `fsrs_eligible` reviews that raise `e_u_ticks >= 7050`.

### 11.3 Weekly 30-Player Leagues, Unified Promotion Floor, Ties & Dormancy (Fixes Audit §4.ج & §8.1)
* **Bracket Scope & Late Entrants:** Ten tiers (`1 = Bronze` to `10 = Diamond`), grouped up to 30 players per bracket upon their first `reward_eligible` point of the week (`Monday 04:00` local rollover). Entrants joining with `< 24h` left before rollover enter a **Late-Joiner Shielded Bracket** (`demotion_slots = 0`).
* **Dormancy Exclusion & Anti-Camp Demotion:** Participants dormant at rollover—or returning from dormancy with `< 3` qualifying study sessions that week—are marked `league_status = DORMANT` and excluded from `ranked_count`.
  * On any weekly rollover where a participant is **not** marked `league_status = DORMANT`, **`consecutive_dormant_weeks` resets to `0`** (Audit Fix §4.ج).
  * On a dormant rollover, `consecutive_dormant_weeks` increments by `1`: if `consecutive_dormant_weeks == 1`, tier is preserved; if **`consecutive_dormant_weeks >= 2`**, the user is **demoted 1 tier per rollover** (floor Tier 1).
* **Unified Minimum Promotion Floor (`MIN_PROMOTION_TROPHIES = 150` Across ALL Bracket Sizes — Audit Fix §8.1):**
  To prevent "bracket shopping" (delaying the first match of the week to land in a small partial bracket and promoting with 10 trophies), **every promotion in any bracket size (`1..=30`) requires `trophies_earned >= MIN_PROMOTION_TROPHIES (150)`**:
  ```text
  Let max_promo_slots = 0                               if ranked_count == 0
                        max(1, ceil(5 * ranked_count / 30)) if ranked_count >= 1
  A ranked player in the top `max_promo_slots` promotes (if tier < 10)
      IFF their weekly `trophies_earned >= 150`.

  Let demotion_slots  = 0                               if ranked_count < 10
                        max(1, ceil(5 * ranked_count / 30)) if ranked_count >= 10
  The bottom `demotion_slots` ranked players demote (if tier > 1).
  ```
* **Deterministic Tie-Breaking Order:** (1) `trophies_earned DESC`, (2) count of `reward_eligible` mastery/recovery transitions `DESC`, (3) `last_trophy_earned_at ASC` (who reached the score first), (4) `user_id ASC`.

### 11.4 Bosses & Clan Wars (Restored in Full + Frozen Roster)
* **Boss Raids:** Shared Boss HP pool per chapter; validated damage from node challenges; supports solo bosses, async shared-HP co-op (24h window), and live 2–4 player co-op lobbies with sequence-synced progress and 15s reconnect; atomic SQL HP decrement; zero overkill reward duplication; idempotent chest claims (`S-17`).
* **Clan Wars (Server-Only `f64`, Frozen Weekly Roster):**
  * At `Monday 04:00` local rollover, `war_roster_member_ids` is frozen as all non-dormant clan members with $\ge 1$ `fsrs_eligible` transition in the prior 7 days (`active_members_snapshot = len(war_roster_member_ids)`). Only members in `war_roster_member_ids` can score war points that week; mid-week joiners enter next week's roster.
  * Stability points: `relative = max(0.0, S_after - S_before) / max(S_before, 1.0)`, `raw_pts = min(50, round_ties_even(10.0 * ln(1.0 + relative)))`, `norm_pts = round_ties_even(raw_pts / sqrt(max(3, active_members_snapshot)))`. Raid win = `25` raw pts (`38` revenge), gated by `reward_eligible` and §10.5 caps.

### 11.5 Event-Sourced Crash Recovery (`R-07` — Restored in Full)
Durable `MATCH_STARTED` written to PostgreSQL before tick 0; periodic **10-second PostgreSQL state snapshots**; if the server restarts within the **15-second grace window**, it reloads the latest snapshot and **replays all committed `review_logs` and sequenced command events since that snapshot** so match state and FSRS memory history never diverge. The match clock freezes during the outage window. Any match unrecoverable after `15s` settles as `VOID_SERVER_INTERRUPTION` with zero trophy/loot delta, entry energy/costs refunded, honest match-history labeling, and preserved immutable review history (`S-14`, `S-16`).

---

## 12. Assets, Layout, Performance, and Platform Delivery (Restored in Full)

* **12.1 Assets:** Verified [Kenney CC0](https://kenney.itch.io/) and [Quaternius](https://quaternius.com/) packs plus procedural Three.js meshes; verify every pack/file's license and format. Maintain `assets/manifest.json` with source URL, author, license evidence, version/SHA-256 hash, conversion notes, and attribution. Original visual presentation only—zero Supercell IP/branding.
* **12.2 Interface & Accessibility:** Mobile portrait $\approx 55\%$ 3D scene / $45\%$ thumb zone; desktop $\approx 65\% / 35\%$; explicit tablet and `768–1023 px` split layouts; Arabic/English DOM/CSS overlays (`dir="auto"`) with proper HarfBuzz shaping, bidi isolation, and mixed LTR/RTL math (`P-07`); equivalent 2D tactical mode (`ConceptSandbox2D`), keyboard/touch navigation, focus rings, `>= 44 CSS px` touch targets (`P-06`), clause-synced subtitles, mute toggle, and `prefers-reduced-motion` support.
* **12.3 Performance & Platforms:** Scene budget **`<= 15,000` triangles total**, single custom GLB asset **`<= 8,000` triangles**, instanced tower/unit **`<= 2,500` triangles** (`A-04`). Measure frame-time percentiles (`p50`, `p95`, `p99`), draw calls, geometry/texture memory, load latency, CPU, and thermal throttling (`O-05`); evaluate KTX2/BasisU and meshopt/Draco compression. Automatic fallback to 2D Tactical Mode when sustained FPS drops below `28 FPS` over a 3-second hysteresis window (plus manual toggle). Android APK tested on physical phone; iOS v1 shipped as an installable Safari PWA (HTTPS, manifest, icons, service worker, safe-area insets, WebGL2/audio unlock on touch) + generated Capacitor iOS scaffold. **Never queue unverifiable competitive outcomes offline.**

---

## 13. Canonical `CL2-CORR-001` Security, Pedagogical & Operational Registers (`A-01..A-07`, `P-01..P-07`, `O-01..O-08`)

To preserve 100% ID compatibility with `CL2-CORR-001` and existing status reports, the original `A-01..A-06`, `P-01..P-06`, and `O-01..O-08` definitions are restored verbatim, and newly separated items take `A-07` and `P-07`:

### 13.1 Security & Adversarial Register (`A-01..A-07`)
* **`A-01` (Permission-Checked Private Asset Delivery):** Slide crops, audio files, and GLB assets for private courses require session-authenticated membership verification; an opaque URL or tenant ID in a path is never access control. *(M1/M2)*
* **`A-02` (Session-Bound Actors & WebSocket Spoofing Prevention):** All HTTP and WebSocket endpoints bind actor identity strictly from authenticated server sessions; reject email-only auth and caller-supplied user IDs, verified by negative impersonation tests. *(M1/M5)*
* **`A-03` (Generated JS & Iframe Execution Disabled):** Arbitrary LLM JS execution is **DISABLED** in the rendering path; if an optional iframe experiment is ever separately approved, it requires per-session nonce, `event.source` verification, schema/sequence checks, payload caps, and `sandbox="allow-scripts"` without `allow-same-origin`. *(Locked)*
* **`A-04` (GLB/GLTF Structural & Triangle Budget Validator):** Reject external URIs, unapproved extensions, embedded scripts, buffer decompression bombs (`> 16 MiB`), **single-asset triangle counts `> 8,000`**, **instanced unit/tower meshes `> 2,500` triangles**, **total scene budgets `> 15,000` triangles**, or textures `> 2048x2048`. *(M4/M5)*
* **`A-05` (Subprocess Group Containment & Fail-Closed Pipes):** `process_group(0)`, async-signal-safe `pre_exec`, cgroup v2 `MemoryMax`, group `SIGKILL` + waitpid reaping, Ghostscript `-dSAFER`, ImageMagick `policy.xml`, and concurrent `stdout`/`stderr` draining with `MAX_BYTES + 1` overflow abort. *(M2)*
* **`A-06` (HTTPS/WSS Ingress, Rate Limits & Port Isolation):** Deliberate TLS/WSS ingress, CSRF/CORS origin checks, per-actor rate limits, and zero public exposure of PostgreSQL, Redis, or inference ports. *(M7)*
* **`A-07` (Restricted Mathematical Expression AST & Authoritative Combat):** Declarative formula evaluator with operator allowlist, max depth `<= 16`, max nodes `<= 64`, domain/zero-division guards, and `NaN`/`Inf` rejection (zero `eval`), paired with server-authoritative combat timers and grading. *(M3/M5)*

### 13.2 Pedagogical & Content Quality Register (`P-01..P-07`)
* **`P-01` (Arabic TTS Pronunciation & WER Gate):** Evaluate synthesized Arabic narration (`Piper ar_JO-kareem-medium`) on a held-out academic Arabic test set and verify Word Error Rate (WER) and intelligibility before M7 release. *(M7)*
* **`P-02` (Arabic Diacritization / Tashkeel Pre-Pass):** Apply deterministic/lexicon-backed Arabic diacritization (Tashkeel) and scientific term vocalization before feeding text to Arabic TTS so inflection and technical terms are pronounced accurately. *(M7)*
* **`P-03` (Calibrated Router Thresholds):** Fit per-task temperature $T_q$ on held-out EN/AR chunks and verify Expected Calibration Error $\text{ECE} < 0.05$ before enforcing probability cutoffs. *(M3)*
* **`P-04` (Numeric/Formula/Negation Fingerprint):** Prevent `pgvector` false merges whenever numbers, units, operators, token order, or EN/AR negation tokens differ. *(M2)*
* **`P-05` (Exhaustive Discrete Grid Reachability):** Sweep up to `4,096` discrete slider states to prove `isWin` is false at `initial_val`, reachable in `[target_min, target_max]`, and free of `NaN`. *(M3)*
* **`P-06` (Measured Touch-Feasible Target Widths):** Assert that target slider intervals and slide hotspots render at `>= 44 CSS px` on a `360 CSS px` mobile viewport. *(M3 gate)*
* **`P-07` (`CLAUSE_CONCAT` Sample-Accurate Sync & RTL/Math Bidi Overlays):** Synthesize narration clause-by-clause via `ort`, trim silence, and compute exact sample-count timestamps for EN and AR, paired with HTML/DOM `dir="auto"` overlays for 3D labels and mixed LTR/RTL math. *(M3/M4/M7)*

### 13.3 Operational & Host Budget Register (`O-01..O-08`)
* **`O-01` (BC-250 Host Resource Budget & Admission Control):** Measure full resident set (Rust server + PG + Redis + active AI worker + subprocesses) on the `11 GiB RAM / 4 GiB VRAM` host; pause new ingestion leases when free available memory drops below safety reserve (`1.5 GiB`). *(M2)*
* **`O-02` (Crash-Consistent Resumable Ingestion Jobs):** Persist stage outputs with input SHA-256 and tool/model versions so interrupted PDF jobs resume cleanly without duplicate nodes. *(M2)*
* **`O-03` (Numeric WebSocket Protocol Budgets):** Enforce `5s` heartbeat, `10s` timeout, `15s` reconnect grace, `10s` snapshots, and `32`-msg / `4 KiB` queue bounds. *(M5)*
* **`O-04` (CPU vs. Vulkan/ROCm Backend Benchmark):** Benchmark `llama-server` and `ort` on BC-250 CPU vs. Vulkan/APU and lock the stable configuration. *(M2/M7)*
* **`O-05` (Client Frame-Time & Thermal Budget):** Measure `p50`/`p95`/`p99` frame times, draw calls, and 28-FPS hysteresis fallback on physical Android hardware. *(M4/M7)*
* **`O-06` (Co-Located 30-Player + Ingestion Load Test):** Run 15 simultaneous live `1v1` duels (30 players) while an active PDF ingestion job runs on the BC-250, asserting WebSocket tick `p99 < 50ms` and zero OOMs. *(M7)*
* **`O-07` (Backup, Restore & Migration Drill):** Verify `pg_dump` / restore and point-in-time recovery of `fsrs_cards`, `review_logs`, and `reward_transactions`. *(M7)*
* **`O-08` (Pinned Dependency & Model Revision Manifest):** Lock exact Cargo.lock, npm lockfile, GGUF/ONNX SHA-256 hashes, and Poppler/ImageMagick versions. *(M7)*

---

## 14. Milestone Roadmap (M0–M8) — Nothing Silently Removed

| Milestone | Status | Authoritative Scope and Closure Gates |
|---|---|---|
| **M0** | **DECISIONS LOCKED — baseline evidence recorded** | 190-vector parity pass; `D-01-A`, `D-02-A`, `D-03`, `R-06-A`, `W_base` v1, `score_v2` locked; `R-04`/`G-3` CLOSED; `G-5` CLOSED-BY-DESIGN; `G-6a` CLOSED. **Open and reassigned:** `G-1`→M2, `G-2`→M3, `G-4`→M4, `G-6b`→M4/M5, license inventory→M2/M3, host budget/admission→M2, training feasibility→M8. |
| **M1** | **BASELINE VERIFIED** | Session actors (`A-02`), composite tenant/course FKs (`S-21`), RLS, private asset delivery (`A-01`), immutable triggers, 6 real-DB tests, `23.57 MiB` peak RSS. Remaining: review version metadata (M5), retention/erasure (M7), per-actor/ingress budgets (M7), exposure-level anti-farming (M5). |
| **M2** | **IN PROGRESS** | §7.1 containment/cgroup/fail-closed pipes (`A-05`); `/Rotate`+`CropBox` normalization + overlay artifact; multi-label region pipeline (unconditional background exclusion, `>=5`-page template exclusion, morphological vector clustering with `path_ink_ratio >= 0.05`, bounded raster figure crop `4%..85%`, `ACTION_OCR_FULL_PAGE_SCAN` for `>=85%`, font-first math protection), 250-DPI crops; DB-backed worker (`O-02`); scoped `pgvector` HNSW (`S-02`/`S-03`/`S-04`, `P-04`); close **`G-1`** on 3 real UTM PDFs + **license inventory** + **host budget/admission control (`O-01`)**. |
| **M3** | NEXT | Two-pass GBNF on single-lease Qwen (`S-11`, `S-12`, `S-20`, `A-07`, `P-03`, `P-05`); EN Kokoro `CLAUSE_CONCAT` + clause captions (`P-07`); close **`G-2`** (WASM feature/size/browser-RSS audit) + **TTS license inventory** + **`P-06` measured touch feasibility**. |
| **M4** | PLANNED | Fixed 3D shell (`<=15k` scene / `<=8k` asset triangles, `A-04`), 6×3 archetypes, 4 themes, 2D parity; `ticks_1e4`, `user_protection_state`, `user_dormancy_periods`, `user_node_shield_state`, `user_node_protection`, `user_card_version_grace`, `user_node_grace_ledger` (`S-19`, `S-23..S-25`); §9 split protection (`AND` bubble), study-anchored dormancy, non-negative grace; async recompute; version-scoped quarantine (`S-13`); `O-05`; close **`G-4`** + **`G-6b` (shield/tier fixtures)**. |
| **M5** | PLANNED | 90s synchronous-round duel server + un-truncated `QUICK_BLITZ_CHAIN` (`4.0s`) + accommodation-safe Sudden-Death tail; `rating_map_version = 2` + penalized `TOO_FAST` (2/match cap + energy cost) + exposure uniqueness (`S-08`); §10.4 split clocks/accommodation; §10.6 `i128` `score_v2` + `round_comp_bps` window-expansion normalization; ghosts, bots, matchmaking, 4s fallback, `F-03`, `S-15`; event-sourced resume (`R-07`, `S-14`/`S-16`); `O-03` numeric WS budgets; void telemetry; close **`G-6b` (rating/clock/scoring fixtures)**. |
| **M6** | PLANNED | Snapshot raids (§11.2, `D-03` clocks, general concurrent cooldown settlement rule) with `F-01`/`F-02` caps; expanded `interaction_source` (`S-09`) & `score_v2` economy ledger (`S-16`, `S-17`); 30-player leagues (§11.3 unified `>= 150` trophy promotion floor across all bracket sizes, deterministic ties, 2-week dormancy demotion, `S-10`); solo/co-op bosses; clan wars (§11.4 frozen weekly war roster); aggregated notifications (`S-18`, `F-05`). |
| **M7** | PLANNED | Android physical device verification; iOS PWA + scaffold; Arabic RTL + Tashkeel pre-pass (`P-02`) + Piper `CLAUSE_CONCAT` + Arabic WER gate (`P-01`, `P-07`); HTTPS/WSS ingress (`A-06`), security tests, observability, `O-08` version pins; CPU/Vulkan benchmarks (`O-04`); retention/restore drills (`O-07`); blocking 30-player/15-duel load test under active ingestion (`O-06`). |
| **M8** | PLANNED | **Feasibility probe as entry gate**; local/external QLoRA + GRPO under §7.4 derived-artifact egress governance; post-merge GGUF held-out re-verification; 7/30-day consented retention study with independent recall assessment. |

---

## 15. Verification Strategy, Definition of Done & Known Risks

### 15.1 Verification Layers & Definition of Done
* **Test Layers:** (1) Unit, property, and adversarial tests (schemas, restricted math AST, 4,096-state reachability, fingerprints, DAG cycle rejection, `ticks_1e4`, `score_v2` `i128` boundary vectors, `round_comp_bps`, protection/dormancy/grace state machines); (2) Cross-target native Rust vs. `wasm32` parity tests; (3) Real PostgreSQL 16 + Redis integration tests (RLS authorization, migrations, retries, concurrent raid cooldown settlement, non-overlapping `btree_gist` grace ledger, atomic settlement, league rollover); (4) Private annotated 3-PDF UTM corpus evaluation (`G-1`); (5) Browser/mobile viewport, touch target (`P-06`), Arabic Tashkeel/WER (`P-01`/`P-02`), bidi/math (`P-07`), and physical Android device profiling (`O-05`); (6) Two-client WebSocket duel/reconnect/crash-replay (`R-07`) plus the 30-player/15-duel co-located ingestion load test (`O-06`).
* **Definition of Done:** A milestone is complete only when its acceptance evidence exists and failures/limitations are recorded. **A feature flag, placeholder, generated platform folder, schema definition, or mock-only test is not proof of end-to-end delivery.** Every gate ends **`PASS` / `FAIL` / `BLOCKED`**; until run it is **`OPEN`**. Historical status reports (`docs/m0-status.md`, `docs/m1-status.md`) are preserved as immutable records. Traces and logs must omit private source text, credentials, and session tokens.

### 15.2 Known Risks and Unresolved Technical Checks
* Exact Android phone model and performance baseline not yet recorded.
* No iPhone access confirmed; no Mac available; native `.ipa` outside v1 acceptance.
* Exact model files, revisions, licenses, and quantized runtime stability require verification in M2/M3; voice licensing remains open.
* Local SFT/GRPO feasibility on BC-250 is unknown (M8 entry gate).
* Real-corpus extraction/crop acceptance is open (`G-1`); synthetic fixture is not annotated lecture data. Representative Arabic PDFs/audio review material still needed before M7.
* Host resource reservations, model-switch latency, and safe concurrency under load require empirical measurement (`O-01`, `O-06`).
* Domain/TLS/ingress and home-host connectivity undecided; no router changes or third-party account creation authorized without explicit user approval.

---

## 16. Current Handoff Checklist & Repository State

* [x] Blueprint reviewed; reference project (`/home/mahdi/campus-lingo`) inspected read-only; canonical root (`/home/mahdi/campuslingo-v2`) confirmed.
* [x] Full scope, English-first sequencing, public/private courses, quality gates approved.
* [x] Local-only AI, co-located BC-250 hosting, 30-player target, sequential single-model residency approved.
* [x] iOS v1 set to installable Safari PWA + Capacitor config/scaffold.
* [x] Both co-op boss modes and frozen-roster clan-war scoring approved.
* [x] Free CC0 asset sources ([Kenney](https://kenney.itch.io/), [Quaternius](https://quaternius.com/)) and 3-PDF evaluation selection approved.
* [x] All eight fairness rules (§10.1) and 3-report quarantine threshold approved.
* [x] Rust/WASM architecture amendment applied; Python restricted to offline M8/eval only.
* [x] M0 foundation evidence (190 vectors), M1 baseline (6 real-DB tests, `23.57 MiB` peak RSS), and M2 synthetic probe recorded.
* [x] Original `CL2-CORR-001` IDs (`S-01..S-22`, `A-01..A-06`, `P-01..P-06`, `O-01..O-08`) and `ADR-003 §2.6` `W_base` v1 table restored in full, with new items cleanly assigned `S-23..S-25`, `A-07`, and `P-07`.
* [x] `CL2-PATCH-001` (`ED-1..ED-4`) applied: **Specification is now FROZEN at v1.8.1**.
* [ ] Close `G-1` (M2), `G-2` (M3), `G-4` (M4), `G-6b` (M4/M5), license inventories, and host budget/admission control (`O-01`).
* [ ] Finish M2 persistence, authorization, worker/restart evidence, cgroup limits, measured embeddings, and 3-PDF real-corpus fidelity.
* [ ] Implement M3–M8 with their linked corrective tests; keep M8 blocked until the feasibility probe and egress consent are recorded.

> **Repository Git State Warning:** Commit `b216667` contains the M0/M1 baseline; **M2 working-tree changes remain uncommitted in `/home/mahdi/campuslingo-v2` and must NOT be discarded or overwritten during handoff.** No production deployment, training run, full duel load, or physical-device acceptance is claimed yet.

---

## 17. Approval Ledger and Gate Register

### 17.1 Decision Status
| Item | State | Governing Section in Unified v1.8.1 |
|---|---|---|
| `D-01-A` tier table, aggregation, split protection (`AND` bubble), non-negative version grace | **APPROVED** | §6.2, §9.1–§9.6 |
| `D-02-A` eligibility, `rating_map_version = 2`, penalized `TOO_FAST`, rewards | **APPROVED** | §10.2–§10.6 |
| `D-03` synchronous duel slices, un-truncated `QUICK_BLITZ_CHAIN`, dynamic tail, raid clocks, `W_base` v1 | **APPROVED** | §8.1, §10.4, §11.2 |
| `R-06-A` `ticks_1e4`, integrity-only brownout hysteresis, durable shield state | **APPROVED** | §1.2, §9.3, §9.6 |
| `F-01`/`F-02`/`F-05` inbound/outbound/pair raid caps, concurrent cooldown settlement & notifications | **APPROVED** | §10.5, §11.2 |
| `score_v2` (`i128 -> i64`), `round_comp_bps` window normalization, unified `>=150` promo floor, frozen clan roster | **APPROVED** | §10.6, §11.3, §11.4 |
| `N-01`…`N-05` optional unapproved additions | **NOT ADOPTED** | Require explicit future feature approval |
| Iframe / model-generated JS execution (`A-03`) | **DISABLED** | §8.2, §13.1 |

### 17.2 Gate Register
| Gate | Owner | Status |
|---|---|---|
| **`G-1`** PDF stack: 3 real UTM PDFs, crop/vector overlay fidelity, process-tree RSS, licenses | **M2** | **OPEN** (synthetic partial only) |
| **`G-2`** WASM feature tree, release build, size/optimizer/browser RSS | **M3** | **OPEN** |
| **`G-3`** Unified math / FMA investigation | — | **CLOSED / NOT REQUIRED** |
| **`G-4`** Diagnostic provenance + $\ge 50$-review sequential chains | **M4** | **OPEN** |
| **`G-5`** Arabic alignment runtime overhead | — | **CLOSED-BY-DESIGN** (`CLAUSE_CONCAT`) |
| **`G-6a`** Tier, brownout, rating, clock & `score_v2` specification approval | **M0** | **CLOSED** (Locked in Unified v1.8.1) |
| **`G-6b`** Executable test fixtures for tiers, clocks, `W_base` v1 fixture assertion (`tests/fixtures/w_base_v1.json`), `score_v2`, `round_comp_bps` & collars | **M4 / M5** | **OPEN** |
| **`G-7`** Database migrations for all `S-01..S-25` items (§6.3) | **M1 → M6** | **PARTIAL** (M1 baseline applied) |

**Execution Order for the Next Implementation Session:**
1. Preserve uncommitted M2 work on top of commit `b216667`, implement §7.1 containment & multi-label region pipeline, and close **`G-1`** + ingestion license inventory + **`O-01`** host budget/admission control.
2. Implement **M3** two-pass single-lease generation & `CLAUSE_CONCAT` TTS, closing **`G-2`**, TTS license inventory, and **`P-06`**.
3. Implement **M4** fixed 3D shell (`<=15k` scene / `<=8k` asset triangles) + shield/protection/dormancy tables (`S-19`, `S-23..S-25`) and close **`G-4`** + **`G-6b` (shield fixtures)**.
4. Implement **M5** synchronous-round 90s duels, `QUICK_BLITZ_CHAIN`, `round_comp_bps` normalization, ghosts/bots (`F-03`, `S-15`), crash recovery (`R-07`), and close **`G-6b` (combat/clock fixtures)** + **`O-03`**.
5. Implement **M6** snapshot raids (general cooldown settlement rule), 30-player leagues (`>= 150` trophy promotion floor), bosses, and frozen-roster clan wars (`S-09`, `S-10`, `S-16..S-18`).
6. Implement **M7** Android device verification, iOS PWA, Arabic expansion (`P-01`, `P-02`, `P-07`), HTTPS/WSS ingress (`A-06`), and the blocking 30-player + ingestion load test (`O-06`).
7. Enter **M8** only after passing the local training feasibility probe and §7.4 derived-artifact egress gate.
