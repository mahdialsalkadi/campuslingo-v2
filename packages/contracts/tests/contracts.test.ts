import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { validateClientCommand, validatePublicQuestion, validateSandboxStructure } from '../src/index.js';

const sandbox = JSON.parse(readFileSync(new URL('../../../tests/fixtures/sandbox.valid.json', import.meta.url), 'utf8'));

test('the shared canonical sandbox fixture passes structural validation', () => {
  assert.equal(validateSandboxStructure(sandbox), true, JSON.stringify(validateSandboxStructure.errors));
});

test('arbitrary code, unknown execution modes, and unknown operations are rejected', () => {
  assert.equal(validateSandboxStructure({ ...sandbox, threejs_code: 'fetch("https://example.invalid")' }), false);
  const altered = structuredClone(sandbox);
  altered.targets[0].expression.op = 'eval';
  assert.equal(validateSandboxStructure(altered), false);
});

test('the generated schema rejects empty operation arguments before execution', () => {
  const altered = structuredClone(sandbox);
  altered.targets[0].expression.args = [];
  assert.equal(validateSandboxStructure(altered), false);
});

test('clients cannot supply trusted identity, damage, rewards, or grades', () => {
  const command = { type: 'deploy', protocol_version: '1.0.0', match_id: '00000000-0000-4000-8000-000000000001', sequence: 0, card_id: '00000000-0000-4000-8000-000000000002', lane: 0 };
  assert.equal(validateClientCommand(command), true);
  for (const field of ['player_id', 'damage', 'rating', 'rewards']) {
    assert.equal(validateClientCommand({ ...command, [field]: 100 }), false);
  }
});

test('public questions cannot accidentally contain grading secrets', () => {
  const question = { id: '00000000-0000-4000-8000-000000000001', prompt: 'Which?', options: ['A', 'B'] };
  assert.equal(validatePublicQuestion(question), true);
  assert.equal(validatePublicQuestion({ ...question, correct_index: 0 }), false);
});
