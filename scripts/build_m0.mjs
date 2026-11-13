import { execFileSync } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { ensureWasmBindgen } from './install_wasm_bindgen.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
function run(command, args) { execFileSync(command, args, { cwd: root, stdio: 'inherit' }); }
const bindingTool = await ensureWasmBindgen();
const targets = execFileSync('rustup', ['target', 'list', '--installed'], { encoding: 'utf8' });
if (!targets.split('\n').includes('wasm32-unknown-unknown')) run('rustup', ['target', 'add', 'wasm32-unknown-unknown']);
run('cargo', ['build', '--release', '--workspace', '--locked']);
run('cargo', ['build', '--release', '-p', 'campuslingo-domain', '--lib', '--target', 'wasm32-unknown-unknown', '--locked', '--config', 'profile.release.strip=false']);
await mkdir(new URL('../packages/engine/generated/', import.meta.url), { recursive: true });
run(bindingTool, ['target/wasm32-unknown-unknown/release/campuslingo_domain.wasm', '--target', 'web', '--out-dir', 'packages/engine/generated']);
run('./node_modules/.bin/esbuild', ['packages/game-client/src/controller.ts', '--bundle', '--format=esm', '--platform=browser', '--outfile=packages/game-client/public/controller.js']);
run('./node_modules/.bin/esbuild', ['packages/game-client/src/shell.ts', '--bundle', '--format=esm', '--platform=browser', '--minify', '--outfile=packages/game-client/public/shell.js']);
run('target/release/export-contracts', []);
run('node', ['scripts/generate_types.mjs']);
run('node', ['scripts/generate_domain_vectors.mjs']);
