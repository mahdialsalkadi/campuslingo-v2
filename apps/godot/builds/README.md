# CampusLingo: Village Clash — Standalone Linux Release (`x86_64`)

## Quick Start
1. **Offline Village / Demo Mode:**
   ```bash
   ./CampusLingoVillageClash.x86_64
   ```
   Pass `-- --demo-raid` to launch directly into an active mixed-army Raid showcase.

2. **Live `campuslingo-server` Mode:**
   Ensure `campuslingo-server` is reachable at `http://127.0.0.1:18080` (override via `CAMPUSLINGO_SERVER_URL`), then run:
   ```bash
   ./CampusLingoVillageClash.x86_64 -- --live-connect
   ```

## System & Renderer Notes
- Built with Godot `4.3.stable` targeting `gl_compatibility` (OpenGL 3.3 Core / ES 3.0 compatible; verified on Mesa LLVMpipe/X11 and RADV/OpenGL).
- Third-party `$0.00` CC0 / OFL-1.1 / MIT asset attributions are bundled in `LICENSES.md` alongside this binary.
