import { spawn } from 'node:child_process';
import { copyFile } from 'node:fs/promises';
import { constants } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const executable = fileURLToPath(new URL('../target/release/campuslingo-ingestion', import.meta.url));
const candidates = [
  { role: 'generator', path: '/home/mahdi/campus-lingo/models/qwen3.5-4b-instruct-q4_k_m.gguf' },
  { role: 'router', path: '/home/mahdi/models/Jev-Style-Qwen3.5-2B-Decision-Q4_K_M.gguf' },
];
const selectedRole = process.argv.includes('--role') ? process.argv[process.argv.indexOf('--role') + 1] : null;
if (selectedRole && !['generator', 'router'].includes(selectedRole)) throw Error('Unknown model role');
const models = selectedRole ? candidates.filter(model => model.role === selectedRole) : candidates;

for (const model of models) {
  const snapshot = Date.now();
  for (const extension of ['json', 'log']) {
    const name = extension === 'json' ? `benchmark-${model.role}` : `llama-${model.role}`;
    try {
      await copyFile(`${root}/artifacts/private/${name}.${extension}`, `${root}/artifacts/private/${name}.initial.${extension}`, constants.COPYFILE_EXCL);
    } catch (error) {
      if (!['EEXIST', 'ENOENT'].includes(error.code)) throw error;
    }
  }
  for (const extension of ['json', 'log']) {
    const name = extension === 'json' ? `benchmark-${model.role}` : `llama-${model.role}`;
    try { await copyFile(`${root}/artifacts/private/${name}.${extension}`, `${root}/artifacts/private/${name}.before-${snapshot}.${extension}`, constants.COPYFILE_EXCL); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  // Deliberately sequential: do not replace with Promise.all.
  const result = await new Promise(resolve => {
    const child = spawn(executable, ['probe', '--model', model.path, '--role', model.role, '--port', '18083'], { cwd: root, stdio: 'inherit' });
    child.once('error', error => { console.error(error); resolve(1); });
    child.once('exit', code => resolve(code ?? 1));
  });
  if (result !== 0) {
    console.error(`${model.role}: probe could not start or finish; no other process was stopped`);
    process.exitCode = 1;
  }
}
