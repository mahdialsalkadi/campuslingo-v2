# CampusLingo v2

New mobile-first learning strategy game. This is **not** an upgrade of
`/home/mahdi/campus-lingo`; that project and its data remain read-only references.

## Current stage: M1 authenticated foundation

This repository contains a Rust/WASM domain engine, an authenticated axum
backend with PostgreSQL persistence, private asset delivery, durable jobs, a
separate recovery-delivery worker, generated contracts, and offline evaluation
tools. It does not yet contain a playable game: combat, matchmaking, Redis, AI
workers, and the M5/M7 duel load gates remain open.

Native/WASM tests use the explicitly user-approved f32-aware memory policy:
`abs_error <= 1e-6 + 1e-6 * abs(reference)`. Discrete outcomes stay exact;
absolute-only drift is preserved as a diagnostic. The seeded generation smoke
probe passes; broader model/quality acceptance remains open. See [M0 status](docs/m0-status.md)
and [M1 status](docs/m1-status.md).

## Runtime architecture

| Path | Responsibility |
| --- | --- |
| `crates/domain` | Canonical serde/schemars contracts, official FSRS-6, finite-grid sandbox verification, shared combat validation, WASM exports. |
| `crates/server` | Auth, tenancy, course/content, idempotent reviews and rewards, durable jobs, private assets, recovery delivery worker. |
| `crates/ingestion` | Rust model lease, admission control, owned-process lifecycle, llama-server HTTP probes, optional dynamic `ort` support. |
| `packages/contracts` | Generated JSON Schema/TypeScript and transport validation. |
| `packages/engine` | Browser-compatible WASM bindings from the same Rust domain crate. |
| `tools/eval/python` | Isolated offline corpus, metadata, reference-scheduler, and reference-validation tools only. |
| `scripts` | Build, generation, local audits, benchmarks, and test runners. |

No FastAPI/Python service or separate JavaScript FSRS implementation is used by
the runtime. The official Rust crate release `fsrs = 7.0.0` is explicitly given
`FSRS6_DEFAULT_PARAMETERS`; its empty/default configuration would select FSRS-7.

### Confirmed constraints

- New project: `/home/mahdi/campuslingo-v2`.
- BC-250 hosts online services and local inference; one active ingestion model
  at a time, including GGUF and ONNX stages.
- Target: 30 players / 15 concurrent duels. The M1 probe exercises 30
  authenticated HTTP learners and real PostgreSQL writes, **not** 15 finished
  combat simulations.
- Online Rust process target: under 50 MiB RSS. Database/cache/model/ONNX-worker
  processes are separate budgets; this is not a whole-stack 50 MiB claim.
- English first, Arabic later; public and private courses.
- Web and Android; iOS v1 is an installable Safari PWA with future Capacitor
  configuration/scaffolding, not a locally signed native iOS build.
- Full M0–M8 roadmap, free licensed assets, shared-HP co-op in both modes, weekly
  clan wars, and all eight approved fairness rules remain in scope.

The full approved plan, including the Rust amendment, is saved at:
`/home/mahdi/.opencode/plan/campuslingo-v2-master-plan.md`.

## Toolchains and reproducibility

Verified local baseline: Rust 1.97.0, Node 22.23.1, npm 12.0.2, and an isolated
Python 3.12.14 environment. `Cargo.lock`, `package-lock.json`, and `uv.lock` lock
the resolved dependencies. Python is optional for normal Rust/WASM work.

```sh
npm ci --ignore-scripts
npm run m0:build
cargo test --workspace --locked
npm run contracts:check
npm run typecheck
npm run test:ts
npm run test:wasm
```

The build installs the `wasm32-unknown-unknown` standard library if missing and
downloads **only the pinned official wasm-bindgen CLI archive**, verifies its
published SHA-256, and places it in the project's ignored `artifacts/toolchain`.
It does not automatically download model weights, ONNX Runtime, or assets.
`npm test` combines build, unit, schema, and approved native/WASM parity checks.
Passing those tests does not imply M0 model/content readiness is complete.

### Local evidence commands

```sh
uv sync --locked
npm run m0:environment
npm run m0:corpus
npm run m0:audit
npm run test:reference
node scripts/probe_rust.mjs
node scripts/validate_m0.mjs
```

`probe_rust.mjs` builds the native workspace, measures 30 local validation
WebSockets, probes the two existing local GGUFs **sequentially**, and checks a
local ONNX Runtime through `ort`. Model paths/library location are explicit in
that local helper; change them deliberately for another machine.

`validate_m0.mjs` gathers checks and evidence without claiming full sign-off.
Use `--reuse-probes` for a code-only verification run that explicitly reuses
existing resource/model evidence instead of loading models again.

### M1 authentication and persistence

The M1 harness starts a disposable PostgreSQL cluster from local PostgreSQL
binaries; Docker is not required. Set `CAMPUSLINGO_PG_BIN` if the default path
does not exist. It creates roles, migrates, applies grants, and runs six
real-database tests:

```sh
npm run m1:verify
```

The load probe uses the same disposable-cluster pattern, seeds 30 authenticated
sessions and a public course, runs the release server, and reports Rust RSS
separately from PostgreSQL:

```sh
npm run m1:load
```

Both commands remove their private cluster unless `CAMPUSLINGO_KEEP_PG=1` is set.
See [M1 status](docs/m1-status.md) for the measured results and the explicit
limits (not a 15-duel/combat benchmark).

Offline Python evaluations never copy or upload the supplied lecture PDFs.
They are not a production ingestion path. The PyMuPDF dependency is AGPL/commercial
licensed and requires a deployment/distribution decision before any production
reuse; its presence in an offline evaluation environment does not settle that.

## Server safety

M1 requires the four database URLs and role grants described in
[M1 status](docs/m1-status.md). The short version:

```sh
CAMPUSLINGO_ADMIN_DATABASE_URL="$OWNER_URL" \
  cargo run -p campuslingo-server -- migrate
psql "$OWNER_URL" -f scripts/m1_grants.sql

CAMPUSLINGO_AUTH_DATABASE_URL="$AUTH_URL" \
CAMPUSLINGO_APP_DATABASE_URL="$APP_URL" \
CAMPUSLINGO_GRADER_DATABASE_URL="$GRADER_URL" \
CAMPUSLINGO_RECOVERY_KEY="<64 hex>" \
CAMPUSLINGO_COOKIE_SECURE=1 \
CAMPUSLINGO_ALLOWED_ORIGIN="https://campus.example" \
  cargo run --release -p campuslingo-server
```

- Default bind: `127.0.0.1:18080`, configurable with `CAMPUSLINGO_BIND`.
  M1 itself does not refuse a public bind, so terminate TLS in front of it,
  set `CAMPUSLINGO_COOKIE_SECURE=1`, and never expose PostgreSQL or Redis.
- `GET /healthz`: liveness only. `GET /readyz`: checks auth, application, and
  grader pools; returns `503` if any is unavailable.
- Application routes live under `/v1`. Authentication uses a hashed opaque
  session token in an `HttpOnly`, `SameSite=Strict` cookie. Cookie-authenticated
  writes reject a mismatched `Origin`.
- `CAMPUSLINGO_ASSET_DIR` is private (`0700`, UUID files at `0600`). Never serve
  it with a static web server; every download must re-authorize through
  `GET /v1/assets/{id}`.
- Run the recovery worker as a separate process with only
  `CAMPUSLINGO_RECOVERY_DATABASE_URL`, the same key, and an HTTPS delivery
  endpoint. The web process cannot mark outbox rows delivered.
- The M0 validation harness remains loopback-only, requires
  `CAMPUSLINGO_M0_WS=1`, and is selected only when M1 database URLs are absent.
  `/m0/ws` is not a trusted review-submission or game API.

Never publish this scaffold, raw inference/database/cache ports, the private
asset directory, or recovery material to the internet. Combat, multiplayer,
AI workers, and deployment hardening remain later gates.

## Evidence and privacy

- `artifacts/private/`: local paths, PDF hashes/statistics, environment/model
  inventory, synthetic model outputs/logs. Ignored by Git.
- `artifacts/benchmarks/`: generated parity/resource reports. Ignored by Git.
- `tests/domain_vectors.json`: committed synthetic inputs, no lecture content.
- `tests/fsrs_golden_vectors.json`: offline Python-reference data only, not the
  canonical production implementation or runtime scheduling policy.

Do not commit PDFs, model weights, credentials, or private course material.
