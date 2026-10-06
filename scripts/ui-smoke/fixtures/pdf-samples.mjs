import { deflateSync } from 'node:zlib';

// Original, deterministic PDF fixtures. No downloaded documents or system fonts.
// Keep the object writer here: these tiny samples must be inspectable and portable.
function document(pageSpecs, { chinese = false, image = false } = {}) {
  const objects = [];
  const add = value => { objects.push(Buffer.from(value)); return objects.length; };
  const put = (id, value) => { objects[id - 1] = Buffer.from(value); };
  const stream = (dictionary, bytes) => Buffer.concat([
    Buffer.from(`<< ${dictionary} /Length ${bytes.length} >>\nstream\n`), bytes, Buffer.from('\nendstream'),
  ]);
  const catalog = add(''), pages = add('');
  const font = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  let cjkFont, raster;
  if (chinese) {
    const zhong = add(stream('', Buffer.from('600 0 0 0 600 900 d1 45 w 100 300 400 400 re S 300 80 m 300 860 l S')));
    const wen = add(stream('', Buffer.from('600 0 0 0 600 900 d1 45 w 270 860 m 340 800 l S 80 730 m 520 730 l S 180 650 m 240 380 330 200 520 80 c S 440 650 m 370 380 270 200 80 80 c S')));
    const unicode = add(stream('', Buffer.from('/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /FixtureUnicode def /CMapType 2 def 1 begincodespacerange <00> <FF> endcodespacerange 2 beginbfchar <01> <4E2D> <02> <6587> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end')));
    cjkFont = add(`<< /Type /Font /Subtype /Type3 /FontBBox [0 0 600 900] /FontMatrix [.001 0 0 .001 0 0] /CharProcs << /zhong ${zhong} 0 R /wen ${wen} 0 R >> /Encoding << /Type /Encoding /Differences [1 /zhong /wen] >> /FirstChar 1 /LastChar 2 /Widths [600 600] /Resources << >> /ToUnicode ${unicode} 0 R >>`);
  }
  if (image) {
    const rgb = Buffer.alloc(32 * 32 * 3);
    for (let y = 0; y < 32; y++) for (let x = 0; x < 32; x++) {
      const color = x < 16 ? (y < 16 ? [220, 40, 40] : [40, 180, 60]) : (y < 16 ? [40, 60, 220] : [220, 180, 40]);
      rgb.set(color, (y * 32 + x) * 3);
    }
    raster = add(stream('/Type /XObject /Subtype /Image /Width 32 /Height 32 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode', deflateSync(rgb)));
  }
  const ids = pageSpecs.map(({ width = 400, height = 600, rotate = 0, label }, index) => {
    const commands = image
      ? 'q 200 0 0 200 100 200 cm /Im1 Do Q'
      : `0.15 0.45 0.8 rg 35 35 ${width - 70} 25 re f 0 g BT /F1 20 Tf 40 ${height - 60} Td (${label || `PAGE ${index + 1}`}) Tj ET`
        + (chinese ? ` BT /F2 48 Tf 40 ${height - 140} Td <0102> Tj ET` : '');
    const content = add(stream('', Buffer.from(commands)));
    return add(`<< /Type /Page /Parent ${pages} 0 R /MediaBox [0 0 ${width} ${height}] /Rotate ${rotate} /Resources << /Font << /F1 ${font} 0 R ${cjkFont ? `/F2 ${cjkFont} 0 R` : ''} >> ${raster ? `/XObject << /Im1 ${raster} 0 R >>` : ''} >> /Contents ${content} 0 R >>`);
  });
  put(catalog, `<< /Type /Catalog /Pages ${pages} 0 R >>`);
  put(pages, `<< /Type /Pages /Count ${ids.length} /Kids [${ids.map(id => `${id} 0 R`).join(' ')}] >>`);
  const parts = [Buffer.from('%PDF-1.7\n%fixture\n')], offsets = [0];
  let length = parts[0].length;
  objects.forEach((object, index) => {
    offsets.push(length);
    const part = Buffer.concat([Buffer.from(`${index + 1} 0 obj\n`), object, Buffer.from('\nendobj\n')]);
    parts.push(part); length += part.length;
  });
  parts.push(Buffer.from(`xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}trailer\n<< /Size ${objects.length + 1} /Root ${catalog} 0 R >>\nstartxref\n${length}\n%%EOF\n`));
  return Buffer.concat(parts);
}

export const pdfSamples = {
  mixed: document([{ label: 'PORTRAIT TEXT' }, { width: 900, height: 400, label: 'LANDSCAPE TEXT' }, { rotate: 90, label: 'ROTATED TEXT' }]),
  chinese: document([{ label: 'EMBEDDED CJK' }], { chinese: true }),
  scan: document([{}], { image: true }),
  long: document(Array.from({ length: 32 }, () => ({}))),
  wide: document(Array.from({ length: 8 }, () => ({ width: 30000, height: 1500 }))),
  malformed: Buffer.from('%PDF-1.7\nnot a valid document'),
};
