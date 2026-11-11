#!/usr/bin/env node
// Generate the M2 annotated English fixture. The PDF is deliberately small and
// deterministic: two digital pages, one image-only OCR page, and one blank page.
// The annotation file is the ground truth used by the Rust acceptance tests.
//
// This is synthetic acceptance evidence, not a claim that the user's real
// lecture corpus has been annotated or that OCR is perfect.

import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const directory = path.join(root, 'tests', 'fixtures', 'm2');
mkdirSync(directory, { recursive: true });

const digitalPageOne = [
  'Photosynthesis converts light energy into chemical energy.',
  'The reaction yield is 42%, not 24%.',
  'Energy is measured in joules (J).',
  'Formula: E = mc^2',
  'The sample has 3 cells.',
  'The sample has 8 cells.',
];

const digitalPageTwo = [
  'Photosynthesis converts light energy into chemical energy.',
  'The cell membrane controls what enters and leaves the cell.',
  'The cell membrane controls what enters and leaves a cell.',
  'The force is zero.',
  'The force is not zero.',
  'The mass is 5 kg.',
  'The mass is 5 g.',
  'The expression is x^2.',
  'The expression is x^3.',
  'The measured value is 1,000 units.',
  'The measured value is 1000 units.',
  'The reaction yield is 24%, not 42%.',
];

const ocrLines = [
  'The reaction yield is 42%, not 24%.',
  'Energy is measured in joules (J).',
  'Formula: E = mc^2',
];

const imagePath = path.join(directory, 'ocr-page.jpg');
const image = spawnSync(
  'magick',
  [
    '-size',
    '1200x420',
    'xc:white',
    '-font',
    'DejaVu-Sans',
    '-pointsize',
    '40',
    '-fill',
    'black',
    '-annotate',
    '+70+100',
    ocrLines[0],
    '-annotate',
    '+70+210',
    ocrLines[1],
    '-annotate',
    '+70+320',
    ocrLines[2],
    '-colorspace',
    'sRGB',
    '-type',
    'TrueColor',
    '-quality',
    '92',
    imagePath,
  ],
  { stdio: 'inherit' },
);
if (image.error) throw image.error;
if (image.status !== 0) throw new Error(`ImageMagick exited ${image.status}`);

function jpegSize(bytes) {
  if (bytes[0] !== 0xff || bytes[1] !== 0xd8) throw new Error('not a JPEG');
  let offset = 2;
  while (offset + 9 < bytes.length) {
    if (bytes[offset] !== 0xff) throw new Error('invalid JPEG marker');
    const marker = bytes[offset + 1];
    const length = bytes.readUInt16BE(offset + 2);
    if ([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf].includes(marker)) {
      return { height: bytes.readUInt16BE(offset + 5), width: bytes.readUInt16BE(offset + 7) };
    }
    offset += 2 + length;
  }
  throw new Error('JPEG dimensions not found');
}

const jpeg = readFileSync(imagePath);
const imageSize = jpegSize(jpeg);

function escapePdf(text) {
  return text.replaceAll('\\', '\\\\').replaceAll('(', '\\(').replaceAll(')', '\\)');
}

function textStream(lines) {
  const commands = ['BT', '/F1 18 Tf', '72 700 Td'];
  lines.forEach((line, index) => {
    if (index > 0) commands.push('0 -38 Td');
    commands.push(`(${escapePdf(line)}) Tj`);
  });
  commands.push('ET');
  return commands.join('\n') + '\n';
}

const streams = {
  4: textStream(digitalPageOne),
  7: textStream(digitalPageTwo),
  9: 'q 540 0 0 189 36 520 cm /Im1 Do Q\n',
  12: '',
};

const objects = new Map();
objects.set(1, '<< /Type /Catalog /Pages 2 0 R >>');
objects.set(2, '<< /Type /Pages /Kids [3 0 R 6 0 R 8 0 R 11 0 R] /Count 4 >>');
objects.set(3, '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>');
objects.set(4, `<< /Length ${Buffer.byteLength(streams[4])} >>\nstream\n${streams[4]}endstream`);
objects.set(5, '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>');
objects.set(6, '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>');
objects.set(7, `<< /Length ${Buffer.byteLength(streams[7])} >>\nstream\n${streams[7]}endstream`);
objects.set(8, '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im1 10 0 R >> >> /Contents 9 0 R >>');
objects.set(9, `<< /Length ${Buffer.byteLength(streams[9])} >>\nstream\n${streams[9]}endstream`);
objects.set(10, `<< /Type /XObject /Subtype /Image /Width ${imageSize.width} /Height ${imageSize.height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n${jpeg.toString('latin1')}\nendstream`);
objects.set(11, '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 12 0 R >>');
objects.set(12, `<< /Length ${Buffer.byteLength(streams[12])} >>\nstream\n${streams[12]}endstream`);

const chunks = [];
let offset = 0;
function push(value) {
  const bytes = Buffer.isBuffer(value) ? value : Buffer.from(value, 'latin1');
  chunks.push(bytes);
  offset += bytes.length;
}
push('%PDF-1.7\n%\xE2\xE3\xCF\xD3\n');
const offsets = new Map();
for (let id = 1; id <= 12; id += 1) {
  offsets.set(id, offset);
  push(`${id} 0 obj\n${objects.get(id)}\nendobj\n`);
}
const xref = offset;
const xrefLines = ['xref', '0 13', '0000000000 65535 f '];
for (let id = 1; id <= 12; id += 1) {
  xrefLines.push(`${String(offsets.get(id)).padStart(10, '0')} 00000 n `);
}
xrefLines.push('trailer', '<< /Size 13 /Root 1 0 R >>', 'startxref', String(xref), '%%EOF', '');
push(xrefLines.join('\n'));
writeFileSync(path.join(directory, 'annotated-english.pdf'), Buffer.concat(chunks));

const annotations = {
  fixture_version: '1.0.0',
  provenance:
    'Synthetic English fixture authored for M2 acceptance. Text pages are digital; page 3 is a raster image rendered with ImageMagick. This does not annotate the user-supplied lecture corpus.',
  pdf: 'annotated-english.pdf',
  page_size_pt: [612, 792],
  pages: [
    { number: 1, route: 'digital', lines: digitalPageOne },
    { number: 2, route: 'digital', lines: digitalPageTwo },
    { number: 3, route: 'ocr', lines: ocrLines, image: imageSize },
    { number: 4, route: 'blank', lines: [] },
  ],
  dedup: {
    must_merge: [
      {
        kind: 'exact',
        text: 'Photosynthesis converts light energy into chemical energy.',
        pages: [1, 2],
      },
      {
        kind: 'near',
        left: 'The cell membrane controls what enters and leaves the cell.',
        right: 'The cell membrane controls what enters and leaves a cell.',
      },
      {
        kind: 'exact_across_routes',
        text: 'The reaction yield is 42%, not 24%.',
        pages: [1, 3],
      },
    ],
    must_not_merge: [
      {
        reason: 'number',
        left: 'The sample has 3 cells.',
        right: 'The sample has 8 cells.',
      },
      {
        reason: 'number_order',
        left: 'The reaction yield is 42%, not 24%.',
        right: 'The reaction yield is 24%, not 42%.',
      },
      {
        reason: 'formula',
        left: 'The expression is x^2.',
        right: 'The expression is x^3.',
      },
      {
        reason: 'unit',
        left: 'The mass is 5 kg.',
        right: 'The mass is 5 g.',
      },
      {
        reason: 'negation',
        left: 'The force is zero.',
        right: 'The force is not zero.',
      },
    ],
  },
  expectations: {
    minimum_digital_crops: 4,
    minimum_ocr_crops: 2,
    minimum_ocr_token_recall: 0.8,
    crop_media_type: 'image/webp',
    max_single_model_rss_bytes: 1073741824,
  },
};
writeFileSync(
  path.join(directory, 'annotations.json'),
  JSON.stringify(annotations, null, 2) + '\n',
);

console.log(
  JSON.stringify({
    pdf: path.join(directory, 'annotated-english.pdf'),
    annotations: path.join(directory, 'annotations.json'),
    image: imageSize,
    jpeg_bytes: jpeg.length,
  }),
);
