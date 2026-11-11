# G-6b — Window-Table Verification Evidence

Date: 2026-11-12

**Scope: window-table sub-gate PASS; full G-6b remains OPEN.** This records checks against the authoritative [master v1.8.1](../campuslingo_v2_master_plan.md), not a new plan or an amendment to its numeric contracts.

## Inputs

| Input | Location / SHA-256 |
| --- | --- |
| Frozen master, unchanged | `../campuslingo_v2_master_plan.md` — `404caf2db246269b854ae573a02b3e2adfd5fa49d7c38788c6ba3397c21cc588` |
| Complete fixture | [w_base_v1.json](../w_base_v1.json) — `4cb7bd0d93a201f235ffcee760a5a7e0681edb3c5885609b10095293132d2ec6` |
| Test harness | [w_base_v1.rs](../w_base_v1.rs) — `cbac868b778282b0a6b8753ebda81cd32d0275775ad86ff9529a79008722c203` |

The fixture and harness were byte-compared with `/home/mahdi/campuslingo-v2/tests/fixtures/w_base_v1.json` and `/home/mahdi/campuslingo-v2/crates/domain/tests/w_base_v1.rs` before running the executable tests. The compiled implementation is `/home/mahdi/campuslingo-v2/crates/domain/src/windows.rs`. Project code is execution evidence, not a separate planning authority.

## Checks and Results

| Check / command | Result |
| --- | --- |
| Independent exact-decimal comparison of fixture rows with master §8.1 | PASS: all 19 pairs, base windows and M=1.0/M=1.5 cutoffs match; M=1.25 verified from exact integer 5/4 arithmetic |
| Pinned versions/counts/multipliers | PASS: version 1; 18 base / 1 chain / 13 live base / 5 overrides; `m1_0=1/1`, `m1_25=5/4`, `m1_5=3/2` |
| Row keys, uniqueness and compiled/fixture set equality | PASS: exact cutoff key sets, no duplicate pairs, no extra or missing compiled pair |
| Deep-Analysis mode fields and actual live cutoffs | PASS: five base forms are not live-eligible, each override is `Some(10000)`, live cutoffs exactly 12000/14500/17000ms |
| `S-19` verification wording | PASS: exactly three references in the unchanged master, lines 284 (§6.3), 670 (§14 M4), 745 (§17.2 execution step 3) |
| `cargo test -p campuslingo-domain --test w_base_v1 --locked` | PASS: 6 tests, 0 failures, 0 ignored |
| `cargo test -p campuslingo-domain --lib windows::tests --locked` | PASS: 2 tests, 0 failures, 0 ignored |
| Decision register and local links | PASS: ten distinct DEC IDs, eight columns including `Touches (§/ID)`, and valid relative balance/master links within `/home/mahdi/Downloads/plan` |

The five deliberate-drift test cases reject duplicate fixture pairs, extra cutoff multiplier keys, extra compiled pairs, wrongly live-enabled `PARETO_BALANCE` base behavior, and a missing Deep-Analysis live override. Unit tests reject unknown pairs, unsupported integer ratios, non-finite stored multipliers and adjacent unapproved f32 values.

## Limits and Remaining Work

- This is current native window/adapter evidence only, not a new WASM parity, database, live combat or load measurement.
- Full `G-6b` still requires its tier/shield, rating, round-clock and scoring integration fixtures under the master’s M4/M5 ownership.
- [DEC-007](../implementation_decisions.md) remains DECIDED. `DEC-008` remains ESCALATE; `DEC-005` and the full `DEC-002` raid-slot mapping remain OPEN. No status was closed merely because these tests passed.
- [balance-v1.md](balance-v1.md) lists unresolved trophy/energy/HP/damage/counter-strike values; none were invented or approved by this verification.
- The master, patch, fixture and harness were not modified in this update. Only this evidence file, the missing balance register and the decision log's local links/evidence references were added or updated.
- The fixture is present locally and its content is linked above. No commit, GitHub upload, new migration or project-source change is claimed by this update.
