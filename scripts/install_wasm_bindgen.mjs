import { createHash } from 'node:crypto';
import { access, chmod, mkdir, readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const version = '0.2.130';
const directory = new URL(`../artifacts/toolchain/wasm-bindgen-${version}/`, import.meta.url);
export const executable = fileURLToPath(new URL('wasm-bindgen', directory));

export async function ensureWasmBindgen() {
  try {
    await access(executable);
    if (execFileSync(executable, ['--version'], { encoding: 'utf8' }).trim() === `wasm-bindgen ${version}`) return executable;
  } catch { /* Install only this version into this project's ignored tool directory. */ }
  if (process.platform !== 'linux' || process.arch !== 'x64') throw Error('Provide wasm-bindgen 0.2.130 for this platform manually');
  await mkdir(directory, { recursive: true });
  const name = `wasm-bindgen-${version}-x86_64-unknown-linux-musl.tar.gz`;
  const url = `https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${version}/${name}`;
  const [archiveResponse, checksumResponse] = await Promise.all([fetch(url), fetch(`${url}.sha256sum`)]);
  if (!archiveResponse.ok || !checksumResponse.ok) throw Error('Cannot fetch official wasm-bindgen artifact/checksum');
  const bytes = Buffer.from(await archiveResponse.arrayBuffer());
  if (bytes.length > 32 * 1024 * 1024) throw Error('Unexpectedly large wasm-bindgen archive');
  const expected = (await checksumResponse.text()).trim().split(/\s+/)[0];
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (!/^[a-f0-9]{64}$/.test(expected ?? '') || actual !== expected) throw Error('wasm-bindgen checksum mismatch');
  const archive = new URL(name, directory);
  await writeFile(archive, bytes, { mode: 0o600 });
  const listing = execFileSync('tar', ['-tzf', fileURLToPath(archive)], { encoding: 'utf8' }).trim().split('\n');
  if (listing.some(path => path.startsWith('/') || path.split('/').includes('..'))) throw Error('Unsafe archive path');
  execFileSync('tar', ['-xzf', fileURLToPath(archive), '--directory', fileURLToPath(directory), '--strip-components=1', '--no-same-owner']);
  await chmod(executable, 0o700);
  await writeFile(new URL('provenance.json', directory), JSON.stringify({ url, sha256: actual, version }, null, 2) + '\n');
  if (execFileSync(executable, ['--version'], { encoding: 'utf8' }).trim() !== `wasm-bindgen ${version}`) throw Error('Unexpected binding tool version');
  return executable;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) console.log(await ensureWasmBindgen());
