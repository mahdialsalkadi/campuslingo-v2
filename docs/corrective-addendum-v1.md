# CampusLingo v2 — Corrective Specification & Remediation Addendum

Document ID: **CL2-CORR-001**
Source version: **1.0**, supplied 2026-11-12
Tracking revision: **2026-11-12**, reconciled with master plan v1.3 and existing evidence
Status: **Incorporated for planning; product proposals, optional additions, and milestone reassignment are not approved**
Governing plan: [campuslingo-v2-master-plan.md](campuslingo-v2-master-plan.md)

This companion retains the supplied addendum's substantive requirements and stable IDs, with explicit reconciliation notes. It is not a verbatim archival copy or a newly approved replacement architecture. The approved Rust/WASM and f32-aware precision amendments remain authoritative. The instruction to update the plan does not by itself approve items the supplied document explicitly marks as proposals.

**Location:** published project copy: `/home/mahdi/campuslingo-v2/docs/corrective-addendum-v1.md`; working plan copy: `/home/mahdi/.opencode/plan/campuslingo-v2-corrective-addendum-v1.md`. The project copy is maintained for the user's requested GitHub publication. The original source blueprint remains unmodified.

## 0. Tracking and evidence rules

- IDs retain their source meanings: D decisions, R Rust/WASM, S schema, A security, P pipeline, F fairness, O operations, H hygiene, N optional, G verification gates. Editorial O-1…O-4 are distinct from operational O-01…O-08.
- Severity is the source priority, not evidence that a defect exists in the current implementation. `BLOCKER` gates the affected capability; a proposed reassignment to M0 is not automatically approved.
- Use `OPEN`, `POLICY-ONLY`, `NEW`, `PARTIAL`, `RESOLVED` or `NOT APPLICABLE` for tracked work. A completed verification gate must record **pass / fail / blocked**, command, revision/input, raw output and limitations. An unrun gate is open, not passed.
- Mark third-party library/model/license/backend claims **[VERIFICATION]** until evidence exists. Do not reproduce the supplied aggregate severity counts as an audited current backlog; many items describe conceptual blueprint DDL, and some are already partly implemented.
- Read current evidence in `docs/m0-status.md`, `docs/m1-status.md`, `artifacts/benchmarks/native-wasm-parity.json`, migrations, grants and tests. Preserve historical reports; this revision did not run builds, model probes, DB tests or device checks.
- Close policy and implementation separately. A written rule, compiled crate, constant, fixture or timeout wrapper alone does not prove end-to-end enforcement.

## A. Executive reconciliation

The addendum identifies three pending product definitions, seven Rust/WASM/runtime concerns, 22 conceptual schema defects, and security/pipeline/fairness/operations work. They are incorporated below without silently changing approved behavior.

Existing work must be credited:

| Area | Evidence already present | What is not established |
| --- | --- | --- |
| FSRS/WASM | `fsrs = 7.0.0` with explicit `FSRS6_DEFAULT_PARAMETERS`, native/browser WASM builds, 190-vector passing report, approximately 363 KiB WASM in M0 documentation | General bit-identical floats, long review chains/boundary completeness, unified `libm`, transitive feature/size/browser-memory audit |
| M1 security/persistence | Six real-PostgreSQL acceptance tests, session actors, private assets, tenant-consistent composite FKs, memberships, immutable reviews/rewards/job events, per-attempt idempotency | Match/session exposure eligibility, full review version metadata, full game economy, complete production ingress/retention/erasure |
| M2 PDF/OCR | Rust Poppler/ImageMagick/tesseract-geometry path; leased local PaddleOCR-VL; synthetic four-page CLI probe with 21 chunks, 21 crops, five duplicates and ~696.8 MiB model peak | Annotated real-corpus acceptance, vector-region coverage, license conclusion, hard subprocess resource/network limits, DB worker/routes, measured embeddings |
| Arabic alignment | Rust/`ort` runtime loading was probed | Arabic TTS, CTC/whisper.cpp choice, alignment API/quality, WER threshold and fallback publication policy |
| Load | M1 HTTP/persistence probe: 30 learners plus instructor, peak online Rust RSS 23.57 MiB | Fifteen complete live duels, live combat under ingestion, device/WAN performance |

`docs/m0-status.md` and `artifacts/benchmarks/m0-status.json` still report overall M0 open/not complete despite passing foundation checks. `docs/m1-status.md` records baseline M1 acceptance. Do not erase either distinction or infer all addendum gates have passed.

## B. Product decisions — explicit approval still required

### D-01 — Total tier table and node-level aggregation

Severity: **BLOCKER (M4)**. State: **POLICY-ONLY; proposal pending**.

The original tier conditions omit states such as `S=30 days, E_u=0.80`. Stability is per card while the presentation tier is per node.

**[PROPOSAL — unchanged substance]**

```text
S_u = min(S_c) over reviewed, non-archived cards of node u
E_u is quantized to 1e-4 before comparison

First match wins:
1. reps_u == 0                       -> Tier 0 (blueprint ruin)
2. E_u < 0.70 OR S_u < 5 days        -> Tier 1 (bronze/common)
3. S_u >= 21 days AND E_u >= 0.85    -> Tier 3 (apex/legendary)
4. otherwise                        -> Tier 2 (reinforced/rare)
```

Tier is presentation/collection progression, never a route around ranked tournament normalization.

Before approval: define `reps_u` aggregation and the empty, all-archived, newly added unreviewed-card and invalid-value states. The minimum of an empty set is undefined; the table is total only once valid inputs and those states are defined. Define quantization/tie-breaking rather than treating `1e-4` alone as an algorithm.

Evidence: property/boundary tests over the supported domain asserting exactly one tier, empty/single-card sets, and regression `S=30,E=0.80` → candidate Tier 2. No tier implementation is authorized by inclusion here.

### D-02 — Attempt → rating and independent eligibility gates

Severity: **BLOCKER (M0 parity/M5/M6 in proposed overlay)**. State: **OPEN; proposal pending**.

**[PROPOSAL]** Treat scheduler eligibility and reward eligibility independently:

| Gate | Meaning | False outcome |
| --- | --- | --- |
| FSRS-eligible | Attempt may mutate the learner's memory state | Audit as practice/ineligible; no scheduler transition |
| Reward-eligible | Attempt may grant competitive points/trophies/loot | No competitive reward; scheduler can still run if independently eligible |

A prompt at `R>0.95` may remain FSRS-eligible with diminished rewards; a repeated exposure in one match is neither. The precise high-recall coefficient and practice-XP treatment still need a versioned reward definition; “diminished” does not mean automatically zero.

Candidate FSRS predicate, all required:

1. Authenticated learner has the personal card/memory record and authorized published content version.
2. First exposure of `(card_id, content_version)` in the current match/session.
3. Not practice, tutorial or replay-spectator context.
4. Trusted server-side presentation/answer timing evidence exists.
5. No previous FSRS transition for that card in the match, even if a content version changed.

Candidate rating mapping:

| Observation | Rating |
| --- | --- |
| Incorrect / timeout / abandon | 1 Again |
| Correct after hint, or partial sandbox accuracy | 2 Hard |
| Correct, first attempt, within the interaction's expected window | 3 Good |
| Correct, first attempt, no hint, latency ≤0.5 × expected window | 4 Easy |

Required refinements **before approval**:

- Specify ordered precedence: the Easy case overlaps Good; hint/partial cases must not accidentally qualify for Easy. Define fully correct but slow, repeated local tries, partial credit versus incorrect, timeout versus partially solved, and abandon versus network failure.
- Define session boundaries outside matches and durable exposure/transition keys. A new attempt UUID must not bypass anti-farming. One `(card,version)` exposure rule and one card-level transition rule are separate constraints.
- “Owns the card” means the learner's memory/progression record, not ownership of shared course content; enrollment and first-use creation must be defined. A client render acknowledgment is untrusted and cannot establish a physically observed render; specify server release/readiness/receipt timing and anti-delay limits.
- Set `expected_window` per archetype/modifier and accommodations. Do not use a universal `<4s`; version latency/network handling. Define how accommodations interact with challenge/combat budgets without unfairly shortening an accommodated learner's effective window.
- Derive ratings on the server. Client preview/library rating inputs are not permission for HTTP/WS callers to submit trusted grades.
- Combat overcharge (`2.0x` intent) and FSRS rating are independent outputs. Overcharge never implies Easy.
- **[VERIFICATION]** Verify official FSRS-6 same-day/short-term behavior and actual API/version before selecting a deferral rule. Existing complete-day inputs do not by themselves prove the right product policy for repeated same-day sessions.
- Persist `rating_map_version`, scheduler/scoring/content/parameter versions, both eligibility flags and reason codes per audit event.

Current implementation delta: M1 computes correct→3/wrong→1, SQL accepts only ratings 1/3, and each distinct answered attempt produces a scheduler update plus one `course_practice_xp`. It rejects duplicate submissions of the **same** attempt, not all distinct repeat exposures. This is foundation evidence, not closure of the eight-rule anti-farming requirement.

Evidence: interaction-trace golden fixtures; adversarial client-rating/timing tests; duplicate requests and distinct second-exposure tests; DB concurrency/restart tests proving at most one eligible transition per card/match and independently correct rewards.

### D-03 — Briefing, challenge and combat clocks

Severity: **BLOCKER (M5)**. State: **POLICY-ONLY; proposal pending**.

**[PROPOSAL]**

| Context | Narration | Challenge clock | Combat time charged |
| --- | --- | --- | --- |
| Live duel | Pre-match lobby, or ≤3s skippable in-match toast | 6–12s per type; pod interaction ≤10s | Challenge window only |
| Async raid/scout | 15–20s permitted, skippable | Separate from briefing | Not time-competitive |
| Solo/boss/practice | 15–20s permitted, skippable | Untimed or generous | None |

The already-approved principle stands: no forced longer narration may consume a competitive interaction window; mute/reduced motion cannot disadvantage play.

Before numeric approval, define whether live narration is a nonblocking overlay or an equal server-scheduled noncompetitive phase. A player skipping audio must not gain extra actions/time or pause the opponent's clock. Reconcile the 90s wall clock, challenge charging, network timing and D-02 accessibility accommodations.

Evidence: context-specific briefing/window assertions and muted/skipped/reduced-motion playthroughs with identical authoritative opportunities and outcomes.

## C. Rust/WASM and runtime register

| ID | Severity / current state | Remediation and closure evidence |
| --- | --- | --- |
| R-01 | BLOCKER M2; **PARTIAL** | Chosen implementation path is Rust + local Poppler layout/render/image tools, ImageMagick WebP and tesseract line geometry, with leased PaddleOCR text. Verify coordinates, image **and vector** region fidelity, encrypted/page-count handling, no-network execution, hard limits and peak process-tree memory on all three representative PDFs. Record licenses/distribution conclusion. Synthetic evidence does not close G-1. |
| R-02 | BLOCKER M3/M7; **OPEN** | Record CTC forced alignment via Rust `ort` versus `whisper.cpp` local HTTP before implementing the Arabic audio path. Verify real forced-alignment API if claimed; ASR word estimates alone are not that capability. Keep `faster-whisper` only as offline oracle under `tools/eval/`. Verify voice/model licensing and Arabic quality. |
| R-03 | BLOCKER M0 in source; **PARTIAL, buildability already evidenced** | Official FSRS-6 compiles to WASM with default features disabled. Finish feature-tree/dependency/size/browser-memory audit and explicit clock/entropy/iteration tests. No tokio/sqlx/fs/network in domain logic; explicit time and seeded RNG if needed. Audit actual wasm `getrandom` feature rather than assuming that dependency implies runtime randomness. Any inference-only fallback/fork is conditional on a real failure and separate review, not automatically equivalent to the approved official implementation. |
| R-04 | HIGH; **OPEN investigation** | Identify actual upstream and domain `powf/exp/ln` call paths, potential FMA differences and supported math hooks. Investigate the same pure-Rust math backend on both targets and compare ULPs. A direct `libm` dependency does not replace transitive crate math. No promised ≤1 ULP result, silent upstream fork or scheduler replacement; approved tolerance remains unchanged. |
| R-05 | HIGH; **PARTIAL, magnitude ambiguity resolved** | Existing largest serialized error is ~8e-5 at ~327.47 days, so it passes the approved allowance (~3.2847e-4). Preserve all 33 absolute-only diagnostics; add automatic reference/ULP/review-chain and computation-path reporting. Independent one-step fixtures are not ≥50-review chains. See evidence table below. |
| R-06 | HIGH; **OPEN mechanism; hysteresis proposal pending** | Define canonical quantization and tie-breaking before discrete rounding/comparisons; add ±1 ULP fixtures at ties and all quantization boundaries. Candidate days and `E_u` step is `1e-4`, not an adopted scheduler change. Hysteresis enter `<0.695`, exit `>=0.705` requires explicit approval; otherwise retain `<0.70` brownout. Quantization cannot guarantee exact outcomes for arbitrary drifts across every boundary; deterministic/server-authoritative handling must be shown. |
| R-07 | HIGH; **OPEN** | Durable match intent at start, versioned bounded snapshots and selected restart behavior: resume within a defined grace window or void with zero reward/trophy delta. Transactional idempotent settlement and kill/restart tests. Define treatment of already committed learning transitions separately from match rewards; do not delete immutable review history on a void. |

### R-01 candidate comparison retained as verification, not a fresh undecided stack

| Candidate | Claims to verify if comparison/replacement becomes necessary |
| --- | --- |
| `pdfium-render` / PDFium | Exact wrapper/binary licenses, redistribution/platform support, text/coordinate/vector fidelity and footprint |
| MuPDF bindings | Exact AGPL/commercial terms and deployment obligations; no inferred compatibility with the project's eventual license |
| Poppler CLI | Exact tool licenses and deployment/distribution obligations; subprocess isolation is not an exemption. Current path uses `pdftohtml -xml`, `pdfinfo`, `pdfimages`, `pdftoppm`. |
| Pure Rust `pdf-extract` / `lopdf` | Layout/vector capabilities and whether they suffice for the representative diagrams; do not assert inadequacy without a spike |

No network/model download or replacement stack is authorized by this list. The earlier accepted local synthetic fixture remains useful; it must not be relabeled annotated real lecture data.

### R-05 evidence reconciliation

Source: existing `artifacts/benchmarks/native-wasm-parity.json`, observed 2026-11-11T16:15:03.154Z. **No parity rerun during this revision.** Values below are the report's serialized decimals; f32 ULP distances are derived from their corresponding positive normal f32 values, not from an assumption that decimal error equals an integer ULP count.

| Vector / field | Native reference | WASM | Serialized abs error | Approved allowance | Derived f32 ULP distance |
| --- | --- | --- | --- | --- | --- |
| 69 stability/interval | 21.9448 | 21.944805 | ~5e-6 | 2.29448e-5 | 3 |
| 79 stability/interval | 11.122906 | 11.122905 | ~1e-6 | 1.2122906e-5 | 1 |
| 114 stability/interval | 184.63446 | 184.6344 | ~6e-5 | 1.8563446e-4 | 4 |
| 115 stability/interval | 327.47098 | 327.4709 | ~8e-5 | 3.2847098e-4 | 3 |

All 33 recorded absolute-only deviations pass the current combined policy; the historical report has zero acceptance differences. The source calculation `|ref|>=79` for an `8e-5` error is correct, and the recorded maximum satisfies it. The table is representative, not the requested automated table for every diagnostic.

`scripts/generate_domain_vectors.mjs` generates independent single-transition review cases, not sequential review chains. Each listed review fixture has one transition from an explicitly seeded memory state. Upstream arithmetic chain length has not been instrumented. G-4 remains open for complete automatic diagnostic provenance and expanded histories; no evidence supports calling these 50-review chains.

## D. Schema remediation and migration applicability — all 22 IDs

The source defects refer to blueprint §5, not an instruction to migrate the old project. New-project migrations are `crates/storage/migrations/0001_foundation.sql` and `0002_recovery_delivery.sql`; role grants live in `scripts/m1_grants.sql`. Preserve applied migration history and existing immutable events. Planning the following work is not applying it.

| ID | Severity | Required correction | Current applicability / migration plan |
| --- | --- | --- | --- |
| S-01 | HIGH | For populated tables: add nullable → backfill/validate → `SET NOT NULL` | Fresh M1 schema initialized tenant fields at creation. No old-data migration is authorized. Apply staged backfills to future populated-table additions, including new review metadata. |
| S-02 | HIGH | Never treat `ADD COLUMN IF NOT EXISTS vector(1024)` as converting 1536 dimensions | No embedding column in new M1 schema and no inherited vectors. Choose measured dimensions in M2. Future changes use model-versioned new column/table, re-embed, validate and swap; never pad/mix. |
| S-03 | MEDIUM | Scope-aware vector retrieval and real ANN index, not just B-tree prefilter | Verify installed pgvector/version before migration; select dimension and distance metric; add HNSW/cosine where justified, partial on eligible non-quarantined content. Test query plan, recall, filters and private scopes. Blueprint sample predicate omitted quarantine and must not be copied unchanged. |
| S-04 | HIGH | No shared empty-string fingerprint; versioned canonicalization and scoped uniqueness | Current M2 has ordered critical-token hashes and conservative exact/near-text comparisons. Add DB canonical chunks/source occurrences with tenant/course or explicitly authorized reuse scope. Evaluate NFKC/Arabic marks/tatweel/digit normalization without erasing unit case, formula powers or meaningful distinctions. No blind unit conversion/diacritic deletion in the safety fingerprint. |
| S-05 | LOW | Use supported UUID generation | M1 already uses built-in `gen_random_uuid()`; no `uuid-ossp` requirement. Retain this in new tables. |
| S-06 | HIGH | Actor/player FKs and valid winner constraints | Existing M1 user references exist; match tables are future M5 work. Add actor FKs and explicit human/ghost/bot/draw/void semantics. A simple winner-IN-two-users check is insufficient for null/bot identities and SQL null behavior. |
| S-07 | HIGH | Review history must not cascade away with cards/nodes | Existing review references are non-cascading, blocking destructive cascades through attempts/content. Retain immutable references; add soft-delete/archive and reviewed account pseudonymization policy before erasure features. |
| S-08 | HIGH | Full review scope, before/after, attempt/idempotency, content/scheduler/scoring/rating-map versions, eligibility | M1 has unique `attempt_id`, scope, actor and before/after states. Add immutable version/parameter/eligibility/reason metadata and match/session exposure identity via new migrations. Do not fabricate historical model/rating provenance; use explicit known legacy-policy labels or unknown markers with justified backfill. |
| S-09 | BLOCKER M6 | Interaction sources must cover all required modes | No complete mode enum in M1. Define/version practice/tutorial/spectator, live/ghost/bot duel, raids/revenge, solo/co-op bosses and league contexts before M5/M6 writes depend on it. Retain source labels `PRACTICE`, `SOLO_BOSS`, `COOP_BOSS`, `LEAGUE` or a reviewed canonical equivalent. |
| S-10 | HIGH | Append-only grants/triggers plus retention/erasure rules | M1 has immutable triggers and runtime SELECT/INSERT-only history grants. Keep these; retention/pseudonymization/backup deletion remain open. Trigger guarantees apply to runtime roles, not an all-powerful DB owner who can alter schema. |
| S-11 | HIGH | Version simulations/content rather than `node_id UNIQUE` | Future M3 schema: unique `(node_id, content_version)`, immutable published versions, explicit current pointer and tenant-consistent FKs. |
| S-12 | HIGH | No executable `threejs_code` in ordinary declarative records | M1 does not introduce such a column; domain uses restricted JSON/AST. Keep normal-path code absent or impossible by execution-mode constraints. Any iframe/code path needs explicit approval and A-03 isolation. |
| S-13 | HIGH | Quarantine/report evidence per content version, not node counter | Add reports with authenticated reporter/enrollment proof, unique report keys, independence checks and audited quarantine decisions. Require ≥3 independent eligible reports or harness failure as already approved; preserve other versions and memory records. |
| S-14 | HIGH | Match lifecycle, opponent kind, versions and settlement identity | Add M5 match intents/status (including in-progress/completed/voided), HUMAN/GHOST/BOT labels, protocol/content/scoring versions, snapshot lineage and unique settlement key. |
| S-15 | MEDIUM | Bounded replay events and usable ghost retrieval index | Add event/byte caps, retention/archive/TTL and permission/eligibility checks. Index course/type/recent completed eligible matches; stable tie-breaks and version-compatible selection. Historical actions must be adapted, not replayed as trusted damage. |
| S-16 | HIGH | Scope leagues/brackets, tiebreaks, rollover audit | Add bracket and membership/standing tables with `(league_scope, week, user)` uniqueness or equivalent reviewed key, partial-bracket rules, versioned tiebreaks and idempotent rollover. |
| S-17 | HIGH | Append-only economy and cosmetic inventory with unique grant keys | M1 already has minimal unique review-linked practice-XP ledger. Extend additively for gold/trophies/loot/cosmetics and source-specific settlement keys; do not call the full economy implemented or grant it from repeated practice attempts. |
| S-18 | MEDIUM | Notification devices/preferences/delivery | Add opt-in device subscriptions, preferences, delivery/outbox/idempotency and caps/quiet-hours/expiry. Recovery outbox is not a general notification subsystem. |
| S-19 | HIGH | Full per-user FSRS-6 parameter version/history and explicit precision | No magic per-card decay in new schema. Current fixed-default adapter uses f32; SQL `REAL` can represent f32 exactly, but round trips/serialization/restart must be proven. Choose/document f32 persistence or reviewed DOUBLE storage, parameter-vector validation/sign/version/trained-at and historical association. Storage precision alone cannot fix cross-target math. |
| S-20 | MEDIUM | Valid archetype/modifier pairs, derived/versioned costs and deterministic layout | Domain validates supported AST/templates; full content/scene schema is future work. Add enum/pair constraints and deterministic collision-safe grid assignment. Source suggests cost from `(archetype,tier)`; review balance and apply ranked normalized tier/levels, never personal mastery as ranked power. |
| S-21 | HIGH | Tenant-consistent composite FKs plus authorization | M1 already has composite course/node/challenge/scope FKs and actor-based course RLS. Extend every new source/run/crop/chunk/content/match relationship; test cross-tenant and cross-course mismatches even when UUIDs exist. |
| S-22 | HIGH | Membership/enrollment, roles and publication rights | Already implemented in M1 with separate tenant/course memberships and rights-attested public publication. Preserve course-level distinctions and actor-bound checks when adding new domains. |

FSRS formula note retained: `factor(d)=0.9^(-1/d)-1` and `R(t)=(1+factor*t/S)^(-d)` do give `R(S)=0.9` for the stated mathematical domain. Verify official library sign/parameter representation; the issue is storage/scope/convention, not this algebraic identity.

## E. Security register

| ID | Severity / state | Correction and evidence |
| --- | --- | --- |
| A-01 | BLOCKER M1; **PARTIAL** | M1 assets already require session/course authorization and private delivery. New M2 crops must use authorized endpoints or deliberately scoped short-lived signed URLs; UUID/static tenant paths are insufficient. Test anonymous, outsider, wrong-course, revoked membership and cache behavior. No public static directory. |
| A-02 | BLOCKER M1; **PARTIAL** | M1 uses opaque hashed sessions and session-bound actors, not legacy email/caller identity. Extend to WS upgrades and every command. Prove impersonation fails even when payload contains a real other user's UUID; connection/session revocation handling belongs in protocol tests. |
| A-03 | HIGH; **OPEN, optional code path disabled** | Opaque iframe origin `"null"` is not authentication. Per-session nonce, `event.source`, strict schema/version/sequence and payload caps; appropriate separate-origin/header CSP and sandbox without `allow-same-origin`. Do not enable code execution without approval. |
| A-04 | HIGH; **OPEN** | GLTF/GLB validator rejects external/unexpected resource URIs/extensions, excessive textures/geometry/buffers and decompression expansion. Test zero unexpected fetches and decoder resource bounds; static is not inherently safe. |
| A-05 | HIGH; **PARTIAL** | Unify documented entrypoint/stage limits, actual during-read output caps, wall/CPU/RSS/pixel/expansion limits, process killing/reaping, no-network policy, encryption/malformed-PDF detection and bounded retries/dead-letter/manual retry visibility. Audit vector-only “blank” classification, crop caps, low-confidence geometry and OCR truncation. |
| A-06 | HIGH; **PARTIAL** | Production HTTPS/WSS, DB/cache/inference kept private, separate role secrets, per-actor/account and IP limits, bounded queues/pools, explicit ingress/origin/session controls. Existing M1 per-IP auth throttle and local probes are not full production deployment. |

Specific M2 checks to plan, not claims of fixes: `pdfinfo` currently lacks the same timeout as other tools; `output()` buffers before post-hoc cap checks; subprocess timeout/drop does not itself establish kill/reap; PDF validation reads whole files; encrypted text parsing and raster limits need hostile fixtures; image-list failure defaults must not silently produce blank pages; max crop counts/dimensions must not silently discard content; OCR response length/finish reason and memory during inference need validation. Resume needs versioned configuration identity and digests, not only file existence. The full process tree, not only llama-server, contributes to ingestion memory.

## F. Content pipeline and Arabic

| ID | Severity | Remediation / evidence |
| --- | --- | --- |
| P-01 | HIGH | Compare normalized intended Arabic script with ASR transcript using a held-out, language-aware WER gate. Define threshold, technical-token checks, regeneration cap and publication policy. Proportional fallback must be explicitly `degraded` with provenance in `AudioManifest`, not “forced aligned.” Low WER alone does not prove timing accuracy; measure alignment error separately. |
| P-02 | HIGH | Evaluate optional diacritization/technical-term pronunciation before TTS, with original and pronunciation text retained and reviewed for unchanged meaning. Record quality and failures; no silent scientific-text rewriting. |
| P-03 | MEDIUM | Fit calibration per task/language on held-out labels; report ECE and coverage/error tradeoffs. Verify raw logits/probability APIs for selected router. Do not claim justified per-question temperature scaling without labels. |
| P-04 | MEDIUM | Separate source/chunk identity from generated-bundle versions. Revalidate earlier outputs under new validators/models with provenance; reuse only eligible versions rather than forcing wholesale regeneration or bypassing source checks. |
| P-05 | HIGH | Restrict DSL to analyzable supported domains; reject unsupported/nonfinite/unreachable cases. Current finite slider grids are exhaustively verified up to 4096 states, not merely sampled. Retain that proof boundary; continuous extensions need sound interval/reachability evidence or another reviewed verifier. |
| P-06 | MEDIUM | Define minimum touch-feasible target width/step/tolerance in normalized units, then measure on named screens/input methods. Prove valid targets are reachable by finger/keyboard and accommodations without changing scientific correctness. |

## G. Fairness, privacy and anti-abuse details

All eight master-plan fairness principles remain approved. The following numeric/behavioral details are not supplied approved values:

| ID | Severity | Required definition / evidence |
| --- | --- | --- |
| F-01 | HIGH | Define new-player bubble duration/review threshold and initial raid eligibility. Evaluate excluding unreviewed cards from `I_u`; define empty/all-unreviewed sets and recognize that changing the aggregation formula needs review, not a silent correction. |
| F-02 | HIGH | Choose/version per-attacker↔defender daily cap, daily loss cap, cooldowns, revenge expiry and anti-collusion/anti-farming checks; concurrency and reset/timezone tests. |
| F-03 | MEDIUM | Define legal ghost/bot adaptation to current state, deterministic invalid-action handling and honest labels. No historical damage replay against a different state. |
| F-04 | HIGH | Allowlisted competition fields and optional pseudonymous display; never public per-student recall/stability history in scanners, replays, notifications or logs. Aggregate/instructor views, if later approved, need separate privacy rules. |
| F-05 | MEDIUM | Numeric notification caps, opt-outs, deduplication/expiry and quiet hours with explicit tenant/user timezone resolution. Raid/revenge delivery must not become a harassment mechanism. |

## H. Operations, performance and platform

| ID | Severity / state | Required evidence |
| --- | --- | --- |
| O-01 | HIGH; **PARTIAL** | One-model lease exists; reconcile full resident set and workload admission with Rust <50 MiB online target, DB/cache/workers/PDF/image/model processes, host reserve and overlapping unified GPU memory. Measure peak/high-water memory, in-request watchdog, unload/reap/resource release and behavior under live load. Never sum model estimates as simultaneous residency. |
| O-02 | HIGH; **PARTIAL** | M1 durable job claims/leases/retries/events exist and M2 has local page checkpoints. Add durable DB-linked stage records, integrity/version-aware restart and visible terminal/dead-letter states/manual retry. Interrupt model/render/crop/persist steps and prove no missing source, unauthorized files, duplicate effects or stale-lease commits. |
| O-03 | HIGH; **OPEN** | Specify heartbeat, reconnect grace, snapshot cadence, queue/payload caps, per-actor rate limits, backpressure and sequence-gap resync with numeric budgets. State machine and two-client adversity tests. |
| O-04 | HIGH; **PARTIAL feasibility, no backend comparison closure** | Existing Vulkan visibility and some inference are evidence only of those probes. Compare CPU/Vulkan cold/warm latency, quality, peak memory, contention and unload; ROCm only if hardware/runtime actually support it. Do not defer feasibility until after committing to that backend. |
| O-05 | MEDIUM; **OPEN** | Named-device frame percentiles, draw calls, textures/geometry, decompression, asset load and sustained thermal/power budget. Evaluate KTX2/BasisU, meshopt/Draco, dynamic resolution/2D fallback with accessibility parity; no universal 60 FPS claim. |
| O-06 | MEDIUM; **PARTIAL local tests** | Blocking CI for available contracts/parity/sandbox/authorization gates; add crash/two-client/15-duel gates when implemented, mandatory for M5/M7 acceptance. Stage tracing without private content/secrets. HTTP load is not duel load. |
| O-07 | MEDIUM; **OPEN** | Versioned retention windows, account pseudonymization/erasure and backup policy consistent with immutable learning/economy history; restore drill and recovery outbox purge/key-rotation plan. Do not promise both irrevocable personal-data retention and full deletion. |
| O-08 | LOW; **PARTIAL pins** | Existing Rust/Node/evaluation lockfiles credited. Pin chosen Next/static host, React/R3F, Capacitor, PostgreSQL and runtimes before adoption after compatibility/security review. Source claims about framework release dates/version lag remain verification, not reasons for automatic migration. |

## I. Internal contradictions and editorial status

| ID | Source contradiction | Treatment in master v1.3 |
| --- | --- | --- |
| H-01 | Absolute-only `1e-6` in §9 versus approved combined policy | Corrected to `1e-6 + 1e-6*abs(reference)`; discrete outcomes remain exact, quantization mechanism pending review, diagnostics retained. Editorial contradiction resolved, numerical boundary work remains open. |
| H-02 | `services/core-ai/eval/benchmark_en_ar.py` suggests old production layout | Corrected to `tools/eval/benchmark_en_ar.py` or equivalent offline reference entrypoint; no claim that the file already exists. |
| H-03 | “Layout proposed, not created” versus implementation begun | Corrected target-layout wording and added existing storage/WASM paths; implementation state points to `docs/m0-status.md`, `docs/m1-status.md` and dated M2 handoff. |
| H-04 | Blueprint still names Python/TS runtime authorities | Master precedence explicitly supersedes those paths, public static assets and startup DDL. Original blueprint annotation is deferred because it is outside the Plan-mode write boundary; do not report that file edited. |

## J. Optional additions — not adopted

| ID | Candidate | Approval boundary |
| --- | --- | --- |
| N-01 | Instructor console: class-level heatmap, corrections, moderation, exam scheduling | New surface and privacy permissions need explicit feature/milestone approval; not implied by instructor upload APIs. |
| N-02 | LTI 1.3 / university SSO | Separate integration/security scope; no account/provider registration authorized. |
| N-03 | Co-op “repair ally” | Optional positive-sum mode beyond already-approved shared bosses; define learning/reward rules before adoption. |
| N-04 | Exam mode with higher desired retention | Scheduling policy, not a new recall curve; requires explicit approval/versioning. |
| N-05 | Additional accessibility options: timed accommodations, colorblind palettes, dyslexia-friendly font | Existing 2D/keyboard/touch/subtitles/mute/reduced-motion accessibility stays mandatory. Newly specified options and D-02 timing semantics remain pending; listing N-05 under M7 does not approve them. |

## K. Withdrawn/recalibrated scaling recommendations

For the approved 30 players / 15 live duels, do not introduce monthly review-log partitioning, a Redis pub/sub horizontal-scaling backplane, or precomputed shield tables merely by asserting they are performance necessities. Measure first. A periodic shield scan/cache may instead be justified for brownout notification semantics. Ranked normalization governs card power/cost fairness; unranked balance still needs explicit rules and measurements. These observations do not remove the approved durable audit, Redis coordination or live-load gates.

## L. Proposed M0 addendum verification gates

The following are tracked now; **assignment as new M0 acceptance gates is pending Part M approval**. Credit existing evidence, but never mark an unrun extension passed.

| Gate | Required action/artifact | Current evidence / remaining work |
| --- | --- | --- |
| G-1 | PDF stack: three representative PDFs, extraction/crop fidelity, peak process-tree RSS and license conclusion | OPEN, partial synthetic path/probes; real selection exists, annotated real-corpus/vector/license/hardening evidence incomplete |
| G-2 | FSRS-6 WASM feature tree, locked release build, size/optimizer/browser-resource report | OPEN audit; documented build/parity/approx.363 KiB baseline already passed. Size optimization report must identify tool version and pre/post artifacts. |
| G-3 | Unified math/FMA investigation and before/after ULP report | OPEN; no common `libm` experiment run. Upstream-change path and approval required before production adoption. |
| G-4 | Every diagnostic's reference magnitude/error/ULP/chain provenance and expanded review histories | OPEN extension; historical tolerance-coverage ambiguity resolved by existing report. Preserve all vectors, add long chains/boundary cases. |
| G-5 | Recorded Arabic alignment runtime decision and measured capabilities/licenses | OPEN, no choice made; production Python prohibited. Choosing an ASR engine is not proof of supplied-script forced alignment. |
| G-6 | Explicit approval of tier/rating/eligibility/clock definitions and golden fixtures | PENDING USER APPROVAL D-01/D-02/D-03; record resolved edge cases rather than sign off incomplete tables |
| G-7 | Migration/remediation plan for all 22 S items, no startup DDL | Planning coverage provided in Part D; implementation/DB acceptance remain separate. Approve ordering and produce exact migrations/backfills/tests outside Plan mode. |

Correct package/artifact examples for a later implementation-capable session; these commands were **not run** during this edit:

```sh
cargo tree -e features --target wasm32-unknown-unknown -p campuslingo-domain --locked
cargo build -p campuslingo-domain --lib --target wasm32-unknown-unknown --release --locked --config profile.release.strip=false
# Actual raw artifact: target/wasm32-unknown-unknown/release/campuslingo_domain.wasm
# Existing build/bindings workflow: scripts/build_m0.mjs; packages/engine/generated/
# If wasm-opt is available, select compatible wasm-bindgen/target features,
# record its version, write a separate optimized copy and rerun parity.
npm run test:wasm
```

Do not paste source `-p domain` commands without correcting the actual package name. Keep raw and optimized artifacts separate; a smaller binary is not proof of client RSS, feature support or exact parity.

## M. Proposed milestone assignment — reproduced for approval

This is the supplied dependency/closure proposal, **not an approved reschedule**. Earlier feasibility decisions must still precede code that relies on them even when final quality evidence belongs later. Existing successful acceptance remains historical evidence; new requirements need their own delta tests.

| Milestone | Proposed items to close |
| --- | --- |
| M0 | D-01/D-02/D-03 definitions; R-01…R-06; H-01…H-04; G-1…G-7; S-19 parameter representation |
| M1 | A-01/A-02/A-06; S-01/S-02/S-05…S-08/S-10/S-21/S-22; O-02 foundation |
| M2 | R-01 implementation; S-03/S-04; P-04/P-05; A-05; O-01; O-02 extraction durability continues |
| M3 | R-02; P-01…P-03/P-06; S-11/S-12/S-20 |
| M4 | D-01 implementation; R-06; S-13; O-05 |
| M5 | D-02/D-03 implementation; R-07; S-14/S-15; A-03/A-04; O-03 |
| M6 | S-09/S-16/S-17; F-01…F-05 |
| M7 | O-04/O-06/O-07/O-08; S-18; **N-05 only if approved** |
| M8 | Unchanged: local SFT/GRPO and independent learning-retention evidence; inference is not training support |

Reconcile before approving: R-02 is named in G-5/M0 but also M3/M7; GPU feasibility and dependency pins cannot wait until M7; interaction sources may be needed before M6; O-02 spans foundation through real worker restart; Arabic quality may be English-first M3 then full M7. P-05's supported discrete verifier already exists and should not be replaced just to satisfy an illustrative interval-arithmetic suggestion. Optional additions never become required via a row in this table.

## N. Approved policies retained, not re-litigated

Tenant/course authorization; independent ≥3-report threshold or harness failure; DAG cycle rejection; authoritative server actions/time/sequencing; practice/eligible separation as a principle; no magic per-card FSRS decay; one-model lease and verified release; ranked normalization; raid opt-in/bounds; honest ghost/bot labels; memory privacy; private uploads and permissioned training; Whisper timestamp versus forced-alignment distinction; 60 FPS/6.5 GB/4s as measured targets rather than guarantees; no trusted offline ranked queues; migration discipline including 1536→1024 incompatibility; and evidence-based completion remain authoritative. These are resolved **as policy**, not a statement that every implementation/test is done.

## O. Editorial edits and approval ledger

| Editorial ID | Requested edit | Applied treatment |
| --- | --- | --- |
| O-1 | Fix §9 absolute-only tolerance | Approved combined tolerance now explicit. Do not claim canonical quantization implemented or universally sufficient; R-06 mechanism remains reviewed/pending. |
| O-2 | Replace production `faster-whisper` Arabic row | Rust `ort` CTC versus `whisper.cpp` choice explicitly open; Python offline only; quality/license/API evidence required. |
| O-3 | Record Rust-compatible PDF stack/license gate in §7.1 | Current Poppler/ImageMagick path recorded with remaining vector/real-corpus/safety/license evidence; no stale claim M2 cannot start because no candidate exists. |
| O-4 | Add §17 tracking/precedence | Added §17 and this companion link under the plan directory, not a nonexistent project-doc link. Proposals and verification remain distinguished. |

Still requiring explicit approval/refinement:

- D-01 tier aggregation/table, including empty/archived/invalid inputs.
- D-02 eligibility/rating mapping, timing trust, session keys, same-day behavior and independent reward policy.
- D-03 numeric clock budgets and skippable narration/accommodation fairness.
- R-06 hysteresis; review quantization/tie-breaking before any rounding/threshold changes.
- Part M/G-1…G-7 milestone assignment overlay.
- Any N-* optional feature and any scheduler fork/new fallback dependency or new product coefficients beyond existing approvals.

No implementation, migration, dependency installation or model download is authorized by this planning register alone. A later implementation-capable session must still respect the approval ledger above.

## Appendix A. SQL-sketch safety notes

The supplied appendix is conceptual and must not be pasted over working M1 controls:

1. `tenant_id = current_setting('app.tenant_id')::uuid` is not sufficient authorization when callers can influence the setting. Use trusted session actor transaction context, membership/course predicates, consistent read/write policies, least-privilege pools and composite FKs. Missing context must fail closed; settings are not secrets.
2. Existing `immutable_audit()` triggers and runtime history grants already implement append-only protection. Retain them; agree retention/pseudonymization before erasure features. Migration-owner privileges are not a malicious-superuser security boundary.
3. Adding several `NOT NULL` columns to populated `review_logs` without valid backfills recreates S-01. `attempt_id` is already unique in M1; extending idempotency must use a reviewed scope and D-02 match/card transition constraints, not a redundant globally trusted caller key.
4. ANN must be based on selected model/dimension/operator/version and actual eligibility/quarantine predicates, with scoped filtered-query tests. `embedding IS NOT NULL` alone does not exclude quarantined content. Permission filtering must not be deferred until after disclosing results.
5. Ghost selection needs authorized scope, eligible completed status, opponent/replay consent, version compatibility and bounded retention as well as an index. Null/draw/bot/winner semantics require explicit constraints.

## Appendix B. Golden-vector coverage checklist

- [ ] First review and valid/invalid initial memory inputs.
- [ ] Same-day repeats through the selected official short-term or explicitly reviewed deferral path.
- [ ] Sequential histories ≥50 reviews, with chain length and parameter-set identity recorded.
- [ ] Large elapsed intervals and near-zero stability; integer elapsed scheduling versus continuous shield-time distinction.
- [ ] Rounding ties ±1 ULP and quantization midpoints on both targets; exact discrete outcomes.
- [ ] `E_u` exactly at and around `0.70`; proposed hysteresis boundaries only if approved.
- [ ] Empty/single-card/all-archived node sets and reviewed/unreviewed transitions.
- [ ] DAG depth/size limits, cycle rejection and cross-course prerequisites.
- [ ] Server interaction traces for hints/partial/incorrect/timeout/abandon/slow/accommodated outcomes.
- [ ] Same-attempt retry and distinct second exposure, concurrent settlement and restart: no extra transitions/rewards.

Existing fixtures provide partial coverage; unchecked means the full expanded corrective gate is not established, not that no test exists.
