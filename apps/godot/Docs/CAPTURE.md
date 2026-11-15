# Capture and story contract

Mandatory from M0. Stefan explicitly requested the World of Oldcraft approach: frequent actual screen captures, process recordings, failures and a chronological story suitable for a full video.

## Evidence types must stay distinct

1. **Actual game-window screenshot:** capture the displayed native game window, including its real HUD. Prefer this to a whole-desktop capture containing unrelated apps.
2. **Actual editor/process screenshot:** Godot or Blender while working on this project. Record what was being changed; exclude terminals with secrets and unrelated windows.
3. **Native viewport/story camera:** the same production scene, fixed camera/lighting/seed for comparison. Label it as a fixed-camera capture.
4. **Real-time gameplay recording:** exported game or native play mode at normal speed. Record game audio only; no microphone or unrelated desktop audio.
5. **Deterministic movie/fly-through:** useful cinematic evidence; identify it as offline capture. It does not prove real-time FPS or a human playtest.
6. **Generated concept:** keep in concepts/. Never place it in a gameplay/progress gallery without that explicit label.

## Capture cadence

| Trigger | Required evidence |
| --- | --- |
| M0, before gameplay implementation | Actual empty project/window, initial plan and tool inventory; first captured file opened and inspected |
| Every significant visual or behavioural change | Before/after feature capture, same framing where possible; caption describing the real change |
| At least every 5 minutes while actively working on a feature | Native feature capture plus an actual visible game/editor screen capture when available |
| Every 15 minutes during world building | Four fixed wide views of the active world; record world/seed/camera IDs |
| Every asset stage | Concept → clean reference → raw mesh → cleanup → rig → animation → in-game appearance |
| Every important motion change | 10–30 seconds of normal-speed native motion, including transitions |
| Every gate | 60–120 second continuous gold-path clip, current build ID, storyboard and report update |
| Every failure/rejection | Preserve failure image/clip, symptom, attempted fix and subsequent result |
| End of run | 3–5 minute uncut representative route, 45–90 second cinematic, before/after board, chronological making-of and evidence index |

Optional during unattended visible work: one actual project-window frame every 60 seconds for a process time-lapse. The supplied window recorder supports this. Keep the five-minute and fifteen-minute authored captures even when automatic frames exist.

## Tooling first

At M0 implement a Godot capture utility that can save the main viewport after a rendered frame, choose a fixed story camera without changing the saved gameplay scene, and append the evidence manifest. Use the production rendering path.

The included tools/capture_window.py captures a visible foreground window belonging to an explicitly selected PID, using the screen pixels. It refuses a minimized/background window and logs a gap instead of stealing focus. It has not been validated on the future game window; run and inspect its first capture before relying on it.

For motion use the available native window/recorder route, or Godot Movie Maker for deterministic clips. A headless run without a rendered image is not visual evidence. Fixed-fps Movie Maker can run slower than real time; label it correctly.

Do not restart unrelated applications or take over the owner's desktop. During owner activity, use isolated project-native capture and log unavailable actual-window frames. Never capture an unrelated window to fill a quota.

## Story camera registry

Create data/story_cameras.json before world art:

- hero_front: fixed neutral hero pose and lighting.
- movement_side: the same test track, profile, input trace and timing.
- hub_wide: fixed view of the mail terminal.
- world_A / world_B / world_C / world_D: four fixed wide views per world.
- enemy_lab: fixed scale, route and camera for each family.
- ui_save: the same menu/save slot state with no personal identifiers.

Store position, rotation, projection/FOV, world, lighting preset and seed. Never overwrite a camera transform to make a comparison prettier; add a new camera/version and disclose the difference.

## Files and metadata

    Docs/progress/<feature>/<UTC timestamp>__<caption>.png
    Docs/progress/_world/<world>/<camera>/<UTC timestamp>.png
    Recordings/<run>/<milestone>/<UTC timestamp>__<action>.mp4
    data/evidence.jsonl
    data/story.json
    data/stats.json

Each evidence row records UTC, local timezone, run ID, milestone, feature, type, caption, source revision/build hash, file path, source SHA-256, width/height, duration/fps where applicable, camera/seed/input trace, expected outcome, observed outcome, defects and reviewer status.

Record actual capture time. Never backdate a recreated screenshot. Append corrections rather than rewriting old observations. A generated image can be logged as concept evidence but never as gameplay.

## Gaps, disk and restart

- Write a heartbeat every minute while automated capture is expected. After two missed five-minute feature intervals, record the gap and repair capture before extensive new visual work.
- A crash, inactive scene, minimized window or owner desktop activity produces a gap record with start/end/reason. Do not fill missing time with duplicated frames.
- Resume from the latest event sequence after restart; filenames and IDs must not collide.
- Keep at least 10 GB free. Stop high-volume recording if that reserve is threatened; keep event logs and occasional small stills, record the gap, and continue safe coding.
- Do not delete original evidence automatically. PNG originals and recording masters remain local; WebP previews and indexes are versioned.
- Bound routine capture storage to a planned 5 GB per first-night run, excluding explicitly selected final masters. Reconcile before extending.

## Editing the story

For every chapter: what we wanted → what visibly failed → what changed → what the next verified capture shows. Include owner feedback only when it exists; keep timestamps and distinguish owner statements from agent observations.

Recommended chapters: empty project; first movement; first hero import; broken animation and fix; first world route; first enemy encounter; first saved objective; expansion; performance/visual polish; final playable build.

At every gate update The result and The numbers in the existing Workspace card. Before footage exists, show honest empty states. The result page begins with representative native gameplay once available, followed by chronological chapters. The numbers page separates wall time, active/unattended/paused time, tokens, provider credits, billed USD, paid failures and unknowns.

The supplied data/story.json and data/stats.json are initial records, not inferred progress. Append actual events and measured counters. When production begins, update build_report.py's preparation-only headings, lead footage, headline counters and registry description before rebuilding the report; its initial empty-state copy deliberately does not infer completion from a log entry. In the standalone kit, maintain the included HTML pages directly using their existing shared styling.

Export shot lists with filename and in/out timecodes. Produce per-feature sequences and a master making-of. Call a consistent cadence record a time-lapse only when the timestamps support it; otherwise label it an edited screenshot sequence. Keep failures in the source archive.

## Final evidence acceptance

- Every claimed milestone links to an opened, inspected native capture.
- Every movement claim includes a normal-speed clip, not only stills.
- At least one actual game-window capture per gate; unavailable screen capture is explicitly reported.
- All fifteen world claims, if made, have their own evidence and route test.
- Generated concepts, offline movies and real-time gameplay are visibly labelled.
- Every media file opens, seeks where relevant, and has a download link.
- Shareable media is reviewed for unrelated desktop content, account details and secrets.
- Captions and logs are in English. UTC is canonical; the report can display Bangkok UTC+7.

Reference: World of Oldcraft result card in workspace/2026-09-26_world-of-oldcraft, and its original task section 18 in workspace/2026-09-24_card442-opus55-24h-mmo/kit/Docs/TASK.md. Adapt the structure; do not copy its game facts, results, costs or Unity-specific commands.

Storage follows processes/workspace-video-storage.md. Preserve publication masters; optimize eligible review copies once and retain hashes/receipts. [Godot Movie Maker documentation](https://docs.godotengine.org/en/stable/tutorials/animation/creating_movies.html).
