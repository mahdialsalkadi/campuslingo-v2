import { Ajv2020 } from 'ajv/dist/2020.js';
import { createRequire } from 'node:module';
import type { FormatsPlugin } from 'ajv-formats';
import sandboxSchema from '../schema/SandboxSpec.json' with { type: 'json' };
import commandSchema from '../schema/ClientCommand.json' with { type: 'json' };
import publicQuestionSchema from '../schema/PublicQuestion.json' with { type: 'json' };

export type { SandboxSpec } from './generated/SandboxSpec.js';
export type { LearningUnitBundle } from './generated/LearningUnitBundle.js';
export type { ClientCommand } from './generated/ClientCommand.js';
export type { AudioManifest } from './generated/AudioManifest.js';

const ajv = new Ajv2020({ allErrors: true, strict: true });
for (const [name, maximum] of [['uint8', 255], ['uint16', 65535], ['uint32', 4294967295]] as const) {
  ajv.addFormat(name, { type: 'number', validate: value => Number.isInteger(value) && value >= 0 && value <= maximum });
}
ajv.addFormat('float', { type: 'number', validate: Number.isFinite });
ajv.addFormat('double', { type: 'number', validate: Number.isFinite });
const addFormats = createRequire(import.meta.url)('ajv-formats') as FormatsPlugin;
addFormats(ajv);

// Structural validation for transport. Publication additionally requires the
// Python business-rule checks and exhaustive sandbox execution harness.
export const validateSandboxStructure = ajv.compile(sandboxSchema);
export const validateClientCommand = ajv.compile(commandSchema);
export const validatePublicQuestion = ajv.compile(publicQuestionSchema);
