import { compile } from 'json-schema-to-typescript';
import { readdir, readFile, writeFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = new URL('../', import.meta.url);
const directory = new URL('packages/contracts/schema/', root);
const target = new URL('packages/contracts/src/generated/', root);
const check = process.argv.includes('--check');
if (!check) await mkdir(target, { recursive: true });
for (const name of (await readdir(directory)).filter(name => name.endsWith('.json')).sort()) {
  const schema = JSON.parse(await readFile(new URL(name, directory), 'utf8'));
  const content = await compile(schema, name.replace('.json', ''), {
    bannerComment: '/* Generated from canonical Rust/serde JSON Schema. Do not edit. */',
    additionalProperties: false,
    unreachableDefinitions: true,
  });
  const path = new URL(name.replace('.json', '.ts'), target);
  if (check) {
    if (await readFile(path, 'utf8').catch(() => '') !== content) {
      throw Error(`Type drift: ${fileURLToPath(path)}; run npm run contracts:generate`);
    }
  } else await writeFile(path, content);
}
console.log(check ? 'Generated types match schemas' : 'Generated TypeScript types');
