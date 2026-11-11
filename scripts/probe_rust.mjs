/** Build/probe workflow; each process completes before the next model starts. */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
for (const [command, args] of [
  ['cargo', ['build', '--release', '--workspace', '--locked']],
  ['node', ['scripts/benchmark_server.mjs']],
  ['node', ['scripts/benchmark_models.mjs']],
  ['cargo', ['run', '--release', '-p', 'campuslingo-ingestion', '--features', 'onnx', '--locked', '--', 'onnx-probe', '--library', '/usr/lib/libonnxruntime.so.1.29.0']],
]) {
  execFileSync(command, args, { cwd: root, stdio: 'inherit' });
}
