# Build, budget and unattended execution

## Engine decision

Use Godot 4.x Standard with typed GDScript for an original Windows x86_64 game. The connected Godot tool reported 4.7.stable.official.5b4e0cb0f during preparation. The official archive lists 4.7.2 as the latest stable patch; 4.8 is a beta. Recheck at launch, use a compatible stable editor/export-template pair and record the exact version. Do not update or close another running project.

Godot fits this bounded original 3D platformer: editable text scenes/resources, character collision APIs, animation state machines, native export and command-line checks. It does not import the SM64 decompilation as a ready Godot game. A direct PC-port mod would retain more original implementation but does not meet this brief's original-public-build objective.

Use Compatibility rendering initially for broad desktop reach and fast startup. Test lighting quality against the concept; record any switch to Forward+ before extensive content production. Do not use physics/render settings from an unrelated project without verification.

## Project structure

    game/
      project.godot
      scenes/{player,hub,worlds,enemies,ui,tests}/
      scripts/{movement,camera,progression,save,audio,tools}/
      resources/{movement,worlds,missions,enemies}/
      assets/{characters,environments,ui,audio}/
      tests/
      export_presets.cfg
    Docs/
      DEVLOG.md, DECISIONS.md, STATE.md
      STORY.md, STATS.md, RECORDINGS.md, LICENSES.md
      progress/, reviews/
    data/
      story.json, stats.json, costs.jsonl, asset_register.csv
    Recordings/          local video masters and review clips
    builds/              local exported game packages
    private_research/    ignored original-game research, if later supplied

Keep public game assets separated from private research by both path and import rules. Do not copy the task's entire folder into an export.

## Runtime requirements

CharacterBody3D with explicit motion state; SpringArm3D or equivalent camera collision; AnimationTree/AnimationPlayer; data-driven mission resources; stable collectible IDs; versioned save data with atomic writes and a recovery backup; original minimal UI; audio buses; debug test routes disabled in normal release.

The first playable route is: title → new game → hub → W01 → complete one task → save → exit → relaunch → continue → return to hub. Maintain that route after every integration batch.

## Combined asset ceiling

USD 100 total across Higgsfield and fal.ai. No automatic top-up, subscription purchase or separate additional provider allowance.

| Envelope | Ceiling |
| --- | ---: |
| Concept/reference preparation, including this brief | $10 |
| Hero, NPC and priority enemy meshes | $30 |
| World kits and landmark props | $25 |
| Original audio, UI and small missing assets | $10 |
| Repairs and contingency | $25 |
| Total | $100 |

Three preparation concepts completed with a quoted total of 6 Higgsfield credits. The CLI did not report billed USD or billed credit reconciliation. Those values remain unknown, not zero. Reserve the entire $10 preparation envelope until a documented account conversion or bill reconciles it; the initial production reservation is therefore at most $90.

For every production job, record UTC, asset/stage, provider, exact model, job ID, quoted billing unit, unit quantity, maximum reserved USD, billed USD/credits when known, local output hash and acceptance verdict. Count queued jobs against the ceiling before submission. Account-wide balance changes can include unrelated jobs; do not attribute them automatically.

If a provider quotes only credits, establish a documented conversion for this account. Otherwise use a provider with a verifiable dollar quote or continue free procedural work. The $0.01-per-credit fal response alone does not establish a mesh price.

No more than two paid attempts at the same asset stage without a recorded new approach. Timeouts require resuming the saved job. Unknown outcome requires history lookup, not blind resubmission. At the spending limit, stop paid generation and continue local implementation/testing.

The asset budget excludes the owner's existing coding-agent subscription. It does not authorize metered LLM API purchases. Record actual model/session consumption if available; a subscription allowance is not a dollar bill.

## Suggested first-night schedule

Time boxes guide priority; they are not promises. Use the clock and leave a resume state.

| Elapsed | Focus |
| --- | --- |
| 0–45 min | M0: inventory, plan, capture smoke test, empty-scene screenshot, budget record. Bound REA/reference preflight to this period. |
| 45 min–3 h | M1: movement lab, camera, test replays, first native Windows build |
| 3–6 h | M2: complete W01, save/reload, hero and two enemy families |
| 6–10.5 h | Expand toward M3/M4 only after existing routes pass. Prefer distinct playable worlds to a large empty map. |
| Final 90 min | Fix blockers, export, record final route, build story/numbers, verify the handoff package |

If a milestone takes longer, preserve the order and reduce content completed this run. Never reduce the full-campaign specification silently. Continue on a later run from the saved state.

## Commands to adapt after locating the actual executable

    godot --headless --path game --editor --quit
    godot --headless --path game --export-release "Windows Desktop" ../builds/MrMak64.exe
    godot --path game

These are documented command shapes, not commands executed by this preparation. Godot is available through the connected tool but was not found on the shell PATH. Locate the executable before using CLI commands. Install matching export templates if absent; do not mistake a successful editor import for an exported-game test.

## Source and handoff

Commit this task only at milestones and at least every hour, with what changed and what was verified. Keep a last-known-working build. Respect the parent repo's existing sync instructions; a source sync is not public game publication.

Deliver Windows executable/package, source, controls/readme, known issues, licence notices, hashes, native screenshots and clips, an HTML story report and accurate numbers. Large binaries/video remain local unless a dedicated storage destination is authorized.

Sources: [Godot releases](https://godotengine.org/download/archive/), [CharacterBody3D](https://docs.godotengine.org/en/stable/classes/class_characterbody3d.html), [Windows export](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_windows.html).
