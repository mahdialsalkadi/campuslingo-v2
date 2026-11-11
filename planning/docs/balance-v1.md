# CampusLingo — Balance v1

Date: 2026-11-12

**Authority:** [CL2-MASTER-UNIFIED-v1.8.1](../campuslingo_v2_master_plan.md), with [implementation decisions](../implementation_decisions.md). Both are in the sole authoritative planning directory, `/home/mahdi/Downloads/plan`.

**Status: PARTIAL — not a released balance configuration.** This document gathers the frozen values and identifies missing values before M5. `FROZEN` means specified by the master; `DECIDED` means an implementation interpretation recorded in the decision log; `OPEN` means no production default has been approved. An escalation cannot be closed merely by switching to Build mode. No formula, predicate, milestone ownership or numeric contract in the frozen master is changed here.

## 1. Trophies and Competitive Contributions

| Parameter | Value / status | Touches (§/ID) |
| --- | --- | --- |
| Trophy deltas for live win, loss and draw; match-end tiebreak | **OPEN** — explicit per-result values and victory/tie conditions are required | §11.1; `S-16` |
| Ghost/bot trophy eligibility and deltas | **OPEN** — do not assume the same rewards as a human duel; honest labels and permissioned replay selection remain mandatory | §11.1; `F-03`; `S-15` |
| Base raid/revenge attacker gain and defender loss | **OPEN** — a daily loss cap does not define the award for one raid | §10.5; §11.2; `F-02` |
| Maximum daily trophy loss from raids | **FROZEN: 45** per defender/day; settlement must apply only the remaining allowance | §10.5; `F-02` |
| Daily inbound / outbound / unordered-pair raid caps | **FROZEN: 3 / 10 / 1**, with revenge included in the pair cap | §10.5; `F-01`; `F-02` |
| Raid against a target protected at settlement | **FROZEN: zero** attacker trophy gain, defender loss, and league/clan points; eligible learning remains and practice XP may apply | §11.2; `S-08`; `S-16` |
| Cap use for that blocked concurrent settlement | **FROZEN:** consumes one attacker outbound raid, not a defender inbound slot | §11.2; `F-02` |
| Post-raid cooldown / revenge window | **FROZEN: 6h / 48h**, revenge single-use | §10.5; §11.2 |
| Practice XP cap | **FROZEN: 100/day**; not trophies or league/clan points | §10.2; §11.2 |
| Rewarded exposure caps | **FROZEN: fewer than 3** prior rewarded exposures per card/day and **fewer than 60** per course/day; user-local 04:00 rollover | §10.2; `S-08`; `S-10` |
| Weekly promotion minimum | **FROZEN: 150 earned trophies** for every bracket size; this is not a per-match trophy delta | §11.3; `S-10` |
| `score_v2` | **FROZEN:** exact §10.6 `i128 -> i64` function, base 10 and 3.0× clamp before ties-even rounding; keep its units separate from trophy settlement | §10.6; `D-02-A`; `S-16` |
| Rating-1 competitive dispatch | **ESCALATE (`DEC-008`):** the frozen function accepts ratings 1–4 and can return a positive result at rating 1. Do not silently implement a success-only caller gate | §0.3; §10.2; §10.6; `G-6b` |
| `W_cut_at_M1(r)` | **DECIDED (`DEC-007`):** maximum of the two actual primary live forms' cutoffs at 1/1, including Deep-Analysis drill overrides and actual Sudden-Death forms | §8.1; §10.4; §10.6 |
| Duel expansion normalization | **FROZEN:** `round_comp_bps` clamped to 10000–20000; retain the exact integer calculation and final rounding from §10.6 | §10.6; `G-6b` |
| Clan raw raid points | **FROZEN: 25**, or **38** for revenge, then the frozen-roster eligibility/normalization rules; these are not trophy deltas | §11.4 |

All outcome and ledger writes must be atomic and idempotent. `VOID_SERVER_INTERRUPTION` preserves immutable reviews, has zero trophy/loot delta, and refunds entry energy/costs under §11.5. Do not equate a review's learning contribution with a match result or award pending trophy values by default.

## 2. Energy

| Parameter | Value / status | Touches (§/ID) |
| --- | --- | --- |
| Initial energy, maximum energy and base regeneration | **OPEN** — supply values, fixed-point scale and regeneration rounding rules before combat integration | §11.1; `A-07` |
| Match length and final phase | **FROZEN: 90s**, with **2×** energy regeneration during the final **30s** | §11.1 |
| Primary card energy cost | **FROZEN range: 1..=6**, deterministically derived from archetype/modifier complexity; the exact 18-pair mapping is **OPEN** | §6.3; `S-20` |
| Bonus chain eligibility | **FROZEN:** regenerated energy at least **2 elixir**, at most **one bonus per slice**, distinct unused card, and its full personal cutoff must fit in the remaining slice | §10.4; `S-08` |
| Exact chain debit/refund behavior | **OPEN** — an eligibility floor is not a complete debit/refund specification | §10.4; `S-16`; `S-20` |
| Penalized voids and `TOO_FAST` | **FROZEN:** the first two transport-loss voids and first two too-fast events consume deployed energy; third+ follow the Again/counter-strike rule. Preserve the separate counters/cases from §10.3 | §10.3 |
| Selection phase | **DECIDED (`DEC-001`): 1500ms** maximum wait, capped at remaining match time, inside `t_match`; normal energy and in-flight combat continue. Timeout alone creates no review or extra penalty | §10.4; §11.1; `O-03` |
| Mute / narration skip | **FROZEN: zero** clock or energy advantage; live toast at most **2.5s**, non-blocking and skippable | §11.1 |

Ranked combat levels are tournament-standardized. Personal FSRS state or collection tier must not change ranked power; selecting a deterministic cost map must preserve that rule (§10.1).

## 3. Damage, HP and Counter-Strike

| Parameter | Value / status | Touches (§/ID) |
| --- | --- | --- |
| Unit, tower and lane HP | **OPEN** — define named entities, integer/fixed-point units, caps and death/end conditions | §11.1; `A-07` |
| Base damage, attack cadence, movement/range and projectile behavior | **OPEN** — define the per-unit table and deterministic rounding/ordering | §11.1; `A-07` |
| Shield-to-combat damage conversion | **OPEN** — preserve §9's memory/protection equations; explicitly define conversion, clamping and rounding before use in combat | §9; §11.1; `R-06-A` |
| Overcharge trigger and multiplier | **FROZEN: 2.0×** for the Easy case in §10.3 | §10.3; §11.1 |
| Overcharge duration / shield-piercing composition / stacking | **OPEN** — no implicit coefficient or expiry may be invented | §10.3; §11.1 |
| Counter-strike trigger | **FROZEN:** the live Case-1 events from §10.3, including the specified penalized third+ events; do not grant it for Case 0 | §10.3; §11.1 |
| Counter-strike duration, strength, target, stacking and expiry | **OPEN** — a trigger alone does not specify an executable window or damage bonus | §10.3; §11.1 |
| Boss HP, contribution damage and chest tiers | **OPEN** M6 configuration; atomic HP reduction and exactly-once claims remain required | §11.4; `S-17` |

Combat/grading remain server-authoritative. Neither a client result nor a balance constant can bypass content validation, exposure eligibility, match state or settlement authorization.

## 4. `F-04` Competition-Projection Allowlist

Only the fields listed below may be projected, and only after authentication, competition opt-in, course/content authorization and applicable consent checks. A listed field is not automatically public. Default-deny all unlisted fields and all private-corpus derivatives without the §7.4 permission. This list does not approve an instructor console or memory heatmap.

| Surface | Allowlisted fields within that surface's authorized scope | Excluded fields |
| --- | --- | --- |
| Competition profile / league | Public competitor ID or pseudonym, selected public display name, league tier/bracket/rank, aggregate competition trophies/points | Email, private student identifiers, session/device/IP data, accommodation multiplier, private memory state |
| Opted-in raid discovery | Competition identity, coarse `raid_available` flag, and course/sector labels the viewer is already entitled to see | `e_u_ticks`, `s_u_ticks`, stability/difficulty, recall/reps, due/review timestamps, grace/dormancy reasons or expiry, personal memory-tier history |
| Current match, participants only | Match/round IDs, shared slice deadline, own legal hand, normalized unit/tower/lane HP, energy and combat/deployment events required for play, labeled opponent kind | Opponent's personal FSRS/parameter/review records, personal grading cutoff, multiplier, RTT model, reason for accommodations |
| Consented authorized ghost playback | Honest ghost/bot label, version-compatible legal actions and authorized match presentation | Raw review logs, private answers/source derivatives without permission, recall/stability histories |
| Notifications | Aggregate count/digest and authorized competition labels; existing **2/day** cap and **22:00–08:00** quiet hours still apply | Memory metrics, personal protection/accommodation details, repeated opponent-targeting alerts |

Observable shared clocks and combat events are not a claim of perfect non-inference. Never add explicit individual accommodation/recall fields to make implementation convenient. A learner's own private memory endpoint is separate from competition projections.

## 5. Closure and Open Decisions

- **Before M3:** retain separate card/FSRS identity for each `AXIOM_PACK` sub-prompt (`DEC-003`), and the existing 8.0s Stage-3 base-window minimum in `DEC-002`; do not stretch a 6.0s or 4.0s frozen base silently.
- **Before M4:** implement `DEC-004`'s durable shield columns and resolve `DEC-005`'s user-global tenant ownership. Adding `tenant_id` must not create independent per-tenant bubbles or reset global counters.
- **Before M5:** approve the OPEN trophy/energy/damage/HP/counter-strike configuration and resolve `DEC-008` through the frozen-spec escalation process. `DEC-007` remains DECIDED; Build mode is not approval of missing balance values.
- **Before M6:** implement the `DEC-009` study-session identity and rollover tests, complete `DEC-002`'s raid-drill slot mapping, and test cap races, blocked settlement, boss HP and claim retries.
- The window-table portion of `G-6b` covers 19 fixture rows, exact labels/ratios, no duplicate/extra pairs, base eligibility and the five 10s live overrides with cutoffs **12000 / 14500 / 17000ms**. It does not close full rating, round scheduling, scoring or shield/tier acceptance.
- Prove `F-04` through negative response-schema, cross-user/course/tenant, replay, notification and log tests. A written allowlist is not implementation evidence.

No combat/economy implementation or full milestone closure is claimed by this file.
