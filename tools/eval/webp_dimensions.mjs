// Header-only dimension inspection. No decoder or external process is used.
export function webpDimensions(bytes) {
  if (bytes.length < 12 || bytes.toString('ascii', 0, 4) !== 'RIFF' ||
      bytes.toString('ascii', 8, 12) !== 'WEBP') return null;
  const end = bytes.readUInt32LE(4) + 8;
  if (end !== bytes.length) return null;
  for (let offset = 12; offset + 8 <= end;) {
    const kind = bytes.toString('ascii', offset, offset + 4);
    const length = bytes.readUInt32LE(offset + 4);
    const start = offset + 8;
    if (start + length > end) return null;
    if (kind === 'VP8X' && length >= 10) {
      return [1 + bytes.readUIntLE(start + 4, 3), 1 + bytes.readUIntLE(start + 7, 3)];
    }
    if (kind === 'VP8L' && length >= 5 && bytes[start] === 0x2f) {
      const bits = bytes.readUInt32LE(start + 1);
      return [(bits & 0x3fff) + 1, ((bits >>> 14) & 0x3fff) + 1];
    }
    // Buffer's ASCII decoding clears the high bit: compare binary signatures
    // as bytes, or a valid 0x9d keyframe marker is misread as 0x1d.
    if (kind === 'VP8 ' && length >= 10 && (bytes[start] & 1) === 0 &&
        bytes[start + 3] === 0x9d && bytes[start + 4] === 0x01 && bytes[start + 5] === 0x2a) {
      return [bytes.readUInt16LE(start + 6) & 0x3fff, bytes.readUInt16LE(start + 8) & 0x3fff];
    }
    offset = start + length + (length & 1);
  }
  return null;
}
