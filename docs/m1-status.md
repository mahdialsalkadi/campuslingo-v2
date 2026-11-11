# M1 status: authenticated application and persistence foundation

M1 is implemented and its real-database acceptance suite passes. This is a
security and persistence foundation, not a playable product: combat,
matchmaking, Redis, AI workers, and the M5/M7 duel load gates are still absent.
The evidence below is reported as measured, including the limits of what was
not measured.

## Implemented controls

- Separate PostgreSQL roles and pools for authentication, application data,
  grading secrets, durable jobs, and recovery delivery. The migration owner is
  used only by the `migrate` command.
- Row-level security on tenancy, course, content, attempt, scheduler, economy,
  asset, and job tables. Helper predicates read metadata through
  `SECURITY DEFINER` functions to avoid recursive policies.
- Opaque 32-byte session tokens stored only as SHA-256 hashes. Cookies are
  `HttpOnly`, `SameSite=Strict`, `Path=/`, optional `Secure`, seven-day expiry.
  Writes reject a mismatched browser `Origin`.
- Argon2 password hashing and verification serialized behind one permit so
  concurrent login attempts cannot multiply memory use. Per-IP authentication
  throttling is in place.
- Recovery requests accept unknown addresses with the same `202` response and
  create no row for them. Known addresses get a 32-byte token stored only as a
  hash, a 15-minute expiry, and an encrypted outbox entry.
- A separate `campuslingo-recovery-worker` process claims outbox rows with a
  lease, decrypts them, posts to a validated webhook, and marks delivery. The
  web process cannot mark rows delivered.
- Tenants, memberships, private/public courses, explicit publication with a
  rights attestation, and separate authoring versus studying authorization.
- Public challenge projections contain no correct index, explanation, or answer
  state. Grading secrets live in `challenge_answers`, which only the grader role
  may read.
- Review and reward writes are one transaction and idempotent per attempt:
  exactly one `review_logs` row and one `reward_ledger` row, FSRS-6 updated
  once. Same-answer retries return the stored result; a different answer after
  completion is a conflict.
- Durable jobs use UUID idempotency keys, payload hashes, `FOR UPDATE SKIP
  LOCKED` claims, lease tokens, bounded retries, progress/stage updates, and one
  result row. Completed or conflicting payloads cannot create extra work.
- Private assets: authorization before the body is read, streamed hashing and
  writing, 80 MiB limit, media-type allow-list, UUID filenames, `0600` files,
  re-authorization on every download, and `private, no-store`, `nosniff`, and
  digest `ETag` headers.
- Versioned migrations (`0001_foundation.sql`, `0002_recovery_delivery.sql`),
  a one-shot `migrate` subcommand, liveness/readiness separation, and structured
  request logs that omit cookies, bodies, emails, and query strings.
- Append-only triggers reject updates and deletes to review, reward, and job
  event history even for the migration owner.

## Role boundaries

| Role | Allowed | Deliberately denied |
| --- | --- | --- |
| `campuslingo_owner` | Owns schema; runs migrations; `BYPASSRLS` | Never used by the HTTP server or workers |
| `campuslingo_auth` | `users`, `sessions`, `recovery_tokens`, `recovery_outbox` insert/select | Courses, tenants, content, grading, economy |
| `campuslingo_app` | Tenants, courses, membership, content, assets, attempts, job enqueue/read | `users`, `challenge_answers`, `fsrs_cards`, `review_logs`, `reward_ledger` |
| `campuslingo_grader` | Grading secrets, attempts, cards, review/reward append | User identities, course authoring, job mutation |
| `campuslingo_worker` | Durable jobs, results, events; `BYPASSRLS` | Credentials are not given to the HTTP process |
| `campuslingo_recovery` | `recovery_outbox` select/update | Users, sessions, token hashes, course data |

## Acceptance evidence

Command:

```sh
npm run m1:verify
# equivalent: node scripts/run_m1_db_tests.mjs
```

Latest result: **6/6 tests passed in 21.88 s** against a disposable PostgreSQL
16.2 cluster with SCRAM authentication and the least-privilege grants.

| Test | What it proves |
| --- | --- |
| `sessions_recovery_and_origin_controls` | Tokens are stored hashed; cookie flags; tampered/expired sessions fail; wrong password gives no cookie; unknown recovery addresses create no row; recovery revokes sessions and is single-use; extra registration fields cannot impersonate; cross-origin writes are rejected. |
| `course_isolation_rls_and_publication_controls` | Outsiders get `404` (not existence leaks); anonymous reads get `401`; public projections omit answers; RLS hides rows; tenant staff does not automatically read course content; application/auth/grader roles lack each other's data; publication requires rights attestation; enrollment enables practice; students cannot update courses. |
| `review_and_reward_are_exactly_once` | Correct answer gives rating 3; retry returns `duplicate: true`; different answer is `409`; exactly one review/reward/card update per attempt; wrong answer gives rating 1; expired attempts fail; a fresh router (restart case) does not append rewards; audit rows are append-only; RLS hides other learners' economy rows. |
| `jobs_are_idempotent_and_leases_are_enforced` | Duplicate idempotency key returns the same job; changed payload is `409`; outsider is forbidden; `SKIP LOCKED` claims; stale leases cannot heartbeat; progress is visible; duplicate completion is `409`; three failures end as `failed` with two retry events; expired leases are reclaimed; two workers claim different rows; non-managers cannot read job details. |
| `assets_are_private_streamed_and_authorized` | A 100 KiB upload passes the 64 KiB JSON limit and is hashed/streamed; file is `0600`; student upload is denied without an orphan; instructor upload works; active media types are rejected; empty uploads are rejected; authorized downloads have exact bytes and safe headers; outsiders/anonymous are denied; asset RLS hides rows; publication enables authenticated reads; a missing file returns `500` without a path leak. |
| `recovery_delivery_is_leased_then_single_use` | The outbox claim is leased; the webhook receives the expected identity/token payload; a delivered row is idle on the next claim and records one attempt; the delivered token confirms once, then `401`; the new password works; the recovery role cannot read users/sessions/recovery tokens; the web auth role cannot mark delivery. |

## Workspace gates

Latest run after the M1 changes:

- `cargo fmt --check`: passed.
- `cargo clippy --workspace --all-targets`: passed with no warnings.
- `cargo test --workspace`: 21 tests passed; the 6 M1 database tests are
  intentionally `#[ignore]` and run only through the harness.
- TypeScript contract tests: 5/5 passed.
- WASM tests: 2/2 passed over 190 shared inputs with the approved f32 policy.
- Offline Python reference tests: 41/41 passed; `ruff check` passed.
- Contract schema drift and TypeScript type checking: passed.
- `npm run m0:build`: native, WASM, and generated TypeScript contracts built.

## M1 load and resource probe

Command:

```sh
npm run m1:load
# workload: 30 authenticated learners + 1 instructor, 15 s, 350 ms think time
```

The probe starts its own PostgreSQL cluster and the release server, seeds real
sessions and a public course, then drives `create_attempt`, `answer_attempt`,
`read_content`, `practice_question`, private asset downloads, and job
enqueue/read over HTTP. Sessions are seeded directly to avoid the authentication
throttle; authentication itself is covered by the acceptance suite.

| Measurement | Result |
| --- | --- |
| Requests / duration | 3,430 in 15 s (228.67 req/s) |
| HTTP statuses | `200`: 2,180; `201`: 1,250; no other status |
| Errors | 0 |
| Latency p50 / p95 / p99 / max | 8.01 ms / 17.31 ms / 46.04 ms / 82.56 ms |
| Rust server RSS, idle / peak | 5.36 MiB / 23.57 MiB |
| Rust target, peak < 50 MiB | passed |
| PostgreSQL RSS, idle / peak (separate) | 87.2 MiB / 410.7 MiB, shared buffers counted per backend |
| Readiness during/after load | HTTP 200 |

The PostgreSQL figure is the sum of postmaster and backend RSS. Shared buffers
are mapped into multiple processes, so it is a conservative separate figure, not
unique physical memory. The Rust figure is the online process only, measured
from `/proc/<pid>/status`.

This is **not** a 15-duel or 90-second combat benchmark. M1 has no combat loop,
matchmaker, Redis contention, AI worker, or device/WAN latency. Do not promote
this result to the M5/M7 gate.

## Operating M1

### 1. Create roles and database

Roles are deployment objects, not migration SQL. Use a secret manager or an
interactive prompt; the example below is illustrative only.

```sql
CREATE ROLE campuslingo_owner LOGIN PASSWORD '...' BYPASSRLS;
CREATE ROLE campuslingo_auth LOGIN PASSWORD '...';
CREATE ROLE campuslingo_app LOGIN PASSWORD '...';
CREATE ROLE campuslingo_grader LOGIN PASSWORD '...';
CREATE ROLE campuslingo_worker LOGIN PASSWORD '...' BYPASSRLS;
CREATE ROLE campuslingo_recovery LOGIN PASSWORD '...';
CREATE DATABASE campuslingo OWNER campuslingo_owner;
```

### 2. Apply migrations and grants

```sh
CAMPUSLINGO_ADMIN_DATABASE_URL='postgres://campuslingo_owner:...@127.0.0.1/campuslingo' \
  cargo run -p campuslingo-server -- migrate
psql "$OWNER_URL" -f scripts/m1_grants.sql
```

`scripts/m1_grants.sql` revokes `PUBLIC` schema access and grants only the
table/function privileges listed above. Re-run it after adding a migration that
introduces new objects.

### 3. Server environment

| Variable | Required | Meaning |
| --- | --- | --- |
| `CAMPUSLINGO_BIND` | no | Listen address, default `127.0.0.1:18080` |
| `CAMPUSLINGO_AUTH_DATABASE_URL` | yes | `campuslingo_auth` pool (max 4) |
| `CAMPUSLINGO_APP_DATABASE_URL` | yes | `campuslingo_app` pool (max 8) |
| `CAMPUSLINGO_GRADER_DATABASE_URL` | yes | `campuslingo_grader` pool (max 4) |
| `CAMPUSLINGO_RECOVERY_KEY` | for recovery | 64 hex characters; encrypts outbox rows |
| `CAMPUSLINGO_ASSET_DIR` | no | Private asset directory, default `artifacts/private/assets` |
| `CAMPUSLINGO_COOKIE_SECURE` | production | Set `1` behind HTTPS |
| `CAMPUSLINGO_ALLOWED_ORIGIN` | production | Exact browser origin; otherwise `Origin` must match `Host` |

Start the server only behind TLS. `GET /healthz` is liveness; `GET /readyz`
checks all three pools and returns `503` when any is unavailable. Request logs
contain method, path, status, and duration only.

### 4. Recovery worker

Run as a separate process with only the recovery role:

```sh
CAMPUSLINGO_RECOVERY_DATABASE_URL='postgres://campuslingo_recovery:...@127.0.0.1/campuslingo' \
CAMPUSLINGO_RECOVERY_KEY='<same 64-hex key>' \
CAMPUSLINGO_RECOVERY_DELIVERY_URL='https://mail-relay.internal.example/recovery' \
  cargo run --release -p campuslingo-server --bin campuslingo-recovery-worker
```

Use `--once` to drain available rows and exit. The delivery endpoint must use
HTTPS; loopback HTTP is accepted for tests. Redirects are disabled and the
request has a 10-second timeout. The JSON body is:

```json
{"user_id":"...","email":"...","token":"<64 hex>","expires_in_seconds":900}
```

The relay must treat the token as a secret, must not log the body, and should
deliver it to the account address without putting it in referrer-bearing links.
Claims lease for 120 seconds, retry at most 8 times with 5–300 s backoff, and
deliver at least once. A crash after the relay accepts a message can duplicate
the email; the token itself is still single-use at confirmation.

### 5. Asset directory

- Directory: `0700`, owned by the service user; files: `0600`, UUID names.
- Never expose the directory through a static web server or object-store public
  bucket. Downloads must go through `GET /v1/assets/{id}` for re-authorization.
- Uploads are authorized before the body is read and streamed while hashing.
  A crash between the file rename and the database insert can leave an orphan
  file; a database row whose file is missing returns `500` and must be
  reconciled by an operator. Temporary `.uploading` files left by a hard kill
  are also operator cleanup.
- No antivirus, content-disarm, or PDF sanitization is implemented. The MIME
  allow-list is not a malware scanner.

## Recovery token lifecycle and key custody

1. `POST /v1/auth/recover` normalizes the address, always returns `202`, and
   writes nothing for an unknown address.
2. For a known address, the server generates 32 random bytes, stores only
   `SHA-256(token)` with a 15-minute expiry, invalidates the previous unconsumed
   token, and encrypts `{user_id, email, token}` into `recovery_outbox` with
   XChaCha20-Poly1305.
3. The recovery worker claims the row, decrypts it, posts it to the relay, and
   marks `delivered_at`. The server's auth role cannot mark it delivered.
4. `POST /v1/auth/recover/confirm` locks the unconsumed, unexpired token row,
   updates the password, marks the token consumed, and deletes every session in
   one transaction. A replay returns `401`.

Current custody limitation: the web server and the recovery worker share the
same symmetric key. Either process can decrypt any retained outbox row. The key
is supplied only by environment/secret manager, never stored in PostgreSQL or
logs. A future version should use asymmetric envelope encryption so the web
process can encrypt without being able to decrypt, and needs a documented key
rotation/re-encryption procedure. Delivered outbox rows are retained with no
automatic purge yet; add a retention job that deletes delivered rows after a
short, documented window.

## Explicit M1 limitations and open gates

- No combat, matchmaking, 90-second duels, Redis, AI ingestion worker, or full
  30-player/15-duel acceptance. The load probe is an HTTP/persistence probe.
- The ingestion job API and durable job state exist, but no general job worker
  process is started by the server. Job claiming/lease/retry behavior is tested
  through the library. The recovery worker is the only separate worker shipped.
- Authentication throttling is an in-memory per-IP window (20 requests/minute,
  one process). It is not a distributed limiter and does not include per-account
  lockout. Multi-instance deployment needs a shared limiter.
- No email verification, MFA, device/session management UI, or account deletion.
- No admin UI or operator endpoint for membership/role changes beyond the
  authenticated HTTP API.
- No automatic recovery outbox retention, no key rotation, and no asymmetric
  key split.
- No upload quotas, per-user rate limits, malware scanning, or PDF sanitization.
- M1 does not claim production readiness, TLS termination, backups, monitoring,
  or a deployment topology. Those are later operational gates.

## Related files

- `crates/storage/migrations/0001_foundation.sql` — schema, RLS, audits, grading
  secret function.
- `crates/storage/migrations/0002_recovery_delivery.sql` — outbox lease and
  attempt counters.
- `scripts/m1_grants.sql` — least-privilege grants.
- `scripts/run_m1_db_tests.mjs` — disposable cluster, roles, migrations, tests.
- `scripts/benchmark_m1.mjs` — seeded HTTP load and RSS probe.
- `crates/server/tests/m1.rs` — six acceptance tests.
- `crates/server/src/recovery.rs` and `src/bin/recovery_worker.rs` — delivery
  validation, leasing, and the separate process.
