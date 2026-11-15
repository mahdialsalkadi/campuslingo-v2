# Acceptance matrix

Use four states: planned, implemented, agent-verified, owner-reviewed. An implemented file is not proof of a working feature. Automated route tests are not novice playtests.

## Functional

- New game → hub → world → objective → save → quit → continue → hub works in the exported Windows build.
- Every movement group in MOVEMENT.md has explicit state transitions, collision checks and recorded normal-speed evidence.
- Keyboard/mouse and one real controller are tested where available. Synthetic controller input alone is labelled.
- Death, checkpoint restart, pause, scene changes, water exit and dialog cannot leave controls stuck.
- Each objective has a unique ID, trigger, success condition, feedback and persistent save state.
- Each complete world has a main route, a return route, two optional secrets, meaningful objectives, working checkpoints and no unavoidable softlock.
- Full campaign: 15 independently tested worlds, all 120 objectives, three boss encounters and an ending. Otherwise label the actual partial scope.
- Save migration/recovery handles a missing or damaged save gracefully.
- No internet or generation provider is required at runtime.

## Visual and audio

- Compare native frames with the concept; list the three largest remaining visual defects per gate.
- Consistent material palette and scale, deliberate normals, foot contact, no floating props, no missing textures.
- Hero and hazards read at gameplay distance; landings remain visible during jumps.
- Camera does not clip through nearby obstacles or hide the critical landing.
- Animation loops, landings, damage and action interruptions work at normal speed.
- Music and sound cues are original/licensed, mixed sensibly and controlled by working settings.
- HUD fits 1280×720 and 1920×1080; text remains legible and prompts match the active input.

## Performance and packaging

- Record test machine, build revision, renderer, resolution and settings.
- Target 60 fps at 1080p in normal gameplay, including a representative busy world. Report median and 95th-percentile frame time over at least 60 seconds.
- Do not use fixed-fps offline recording as performance evidence.
- No recurring runtime errors, broken resource paths or hidden dependency on editor files.
- Test a fresh user-data folder and an exported build outside the source directory.
- Package only approved runtime assets and required notices. Include controls, known issues and checksums.

## Movement comparison

No reference data: mark parity unmeasured. Reference data exists: retain method, version, input traces, measurement uncertainty and differences. Passing selected metrics never becomes a claim of exact whole-game equivalence.

## Evidence and release

The CAPTURE.md contract is a separate acceptance area. Missing evidence remains a defect. RIGHTS.md release review remains pending until the actual package is inspected. The preparation agent does not grant legal clearance.
