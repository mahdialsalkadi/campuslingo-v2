// Original synthetic PDF: non-zero MediaBox/CropBox, all four rotations,
// ASCII glyphs in a math font, a bounded raster and a full-page scan.
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const dir = fileURLToPath(new URL('../tests/fixtures/m2/', import.meta.url));
mkdirSync(dir, { recursive: true });
const objects = [];
const add = (value) => { objects.push(Buffer.isBuffer(value) ? value : Buffer.from(value)); return objects.length; };
const stream = (data, dictionary = '') => Buffer.concat([
  Buffer.from(`<< ${dictionary} /Length ${data.length} >>\nstream\n`), data, Buffer.from('\nendstream'),
]);
add('<< /Type /Catalog /Pages 2 0 R >>');
add('');
const font = add('<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>');
const math = add('<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>');
const raster = Buffer.alloc(32 * 32 * 3);
for (let y = 0; y < 32; y++) for (let x = 0; x < 32; x++) {
  const i = (y * 32 + x) * 3;
  raster[i] = x * 8; raster[i + 1] = y * 8; raster[i + 2] = ((x + y) % 2) * 255;
}
const image = add(stream(raster, '/Type /XObject /Subtype /Image /Width 32 /Height 32 /ColorSpace /DeviceRGB /BitsPerComponent 8'));
const pages = [];
for (let p = 0; p < 6; p++) {
  const content = p < 4
    ? '1 1 1 rg 10 20 600 400 re f\n0 0 1 rg 100 140 200 80 re f\nBT /F1 14 Tf 100 270 Td (if not x != 5:) Tj 0 -18 Td (    return x) Tj ET\nBT /F2 14 Tf 100 240 Td (a) Tj ET\nq 160 0 0 70 360 130 cm /Im1 Do Q\n'
    : p === 4 ? 'q 500 0 0 300 60 70 cm /Im1 Do Q\n'
    : 'BT /F1 14 Tf 100 270 Td (Short digital title) Tj ET\n';
  const contents = add(stream(Buffer.from(content)));
  pages.push(add(`<< /Type /Page /Parent 2 0 R /MediaBox [10 20 610 420] /CropBox [60 70 560 370] /Rotate ${p < 4 ? p * 90 : 0} /Resources << /Font << /F1 ${font} 0 R /F2 ${math} 0 R >> /XObject << /Im1 ${image} 0 R >> >> /Contents ${contents} 0 R >>`));
}
objects[1] = Buffer.from(`<< /Type /Pages /Kids [${pages.map(id => `${id} 0 R`).join(' ')}] /Count ${pages.length} >>`);
const chunks = [Buffer.from('%PDF-1.7\n')];
const offsets = [0];
let length = chunks[0].length;
objects.forEach((object, index) => {
  offsets.push(length);
  const chunk = Buffer.concat([Buffer.from(`${index + 1} 0 obj\n`), object, Buffer.from('\nendobj\n')]);
  chunks.push(chunk); length += chunk.length;
});
chunks.push(Buffer.from(`xref\n0 ${offsets.length}\n0000000000 65535 f \n${offsets.slice(1).map(n => `${String(n).padStart(10, '0')} 00000 n \n`).join('')}trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`));
writeFileSync(`${dir}region-cases.pdf`, Buffer.concat(chunks));
