import assert from 'node:assert/strict';
import test from 'node:test';
import { webpDimensions } from './webp_dimensions.mjs';

function riff(kind, payload) {
  const bytes = Buffer.alloc(20 + payload.length + (payload.length & 1));
  bytes.write('RIFF');
  bytes.writeUInt32LE(bytes.length - 8, 4);
  bytes.write('WEBP', 8);
  bytes.write(kind, 12);
  bytes.writeUInt32LE(payload.length, 16);
  payload.copy(bytes, 20);
  return bytes;
}

test('lossy VP8 binary keyframe marker retains its high bit', () => {
  const payload = Buffer.from('0000009d012a2005eb03', 'hex');
  assert.deepEqual(webpDimensions(riff('VP8 ', payload)), [1312, 1003]);
  payload[3] = 0x1d;
  assert.equal(webpDimensions(riff('VP8 ', payload)), null);
});

test('extended and lossless WebP dimensions', () => {
  const extended = Buffer.alloc(10);
  extended.writeUIntLE(874, 4, 3);
  extended.writeUIntLE(1469, 7, 3);
  assert.deepEqual(webpDimensions(riff('VP8X', extended)), [875, 1470]);
  const lossless = Buffer.alloc(5);
  lossless[0] = 0x2f;
  lossless.writeUInt32LE(874 | (1469 << 14), 1);
  assert.deepEqual(webpDimensions(riff('VP8L', lossless)), [875, 1470]);
});

test('invalid signatures, lengths and truncated chunks fail closed', () => {
  assert.equal(webpDimensions(Buffer.alloc(0)), null);
  const bytes = riff('VP8 ', Buffer.from('0000009d012a2005eb03', 'hex'));
  assert.equal(webpDimensions(bytes.subarray(0, -1)), null);
  bytes.writeUInt32LE(1000, 16);
  assert.equal(webpDimensions(bytes), null);
});
