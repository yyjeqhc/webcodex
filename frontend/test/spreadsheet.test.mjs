import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import vm from 'node:vm';
import { deflateRawSync } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const context = vm.createContext({ Uint8Array, DataView, TextDecoder, Blob, DecompressionStream });
vm.runInContext(readFileSync(resolve(root, 'vendor/sheetjs/xlsx.mini.min.js'), 'utf8'), context);
vm.runInContext(readFileSync(resolve(root, 'src/mcp-apps/spreadsheet-data.js'), 'utf8'), context);
const parser = vm.runInContext('WebCodexSpreadsheet', context);
const XLSX = context.XLSX;
const plain = value => JSON.parse(JSON.stringify(value));
const text = source => new TextEncoder().encode(source);
const parse = async (bytes, name) => plain(await parser.parse(bytes, name));
function workbook(sheet, name = '预算') {
  const book = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(book, sheet, name);
  return new Uint8Array(XLSX.write(book, { type: 'array', bookType: 'xlsx' }));
}

test('CSV preserves identifiers, formula-looking strings, quoted delimiters and untrusted markup', async () => {
  const result = await parse(text('编号,内容\n0007,"hello, world"\n=1+1,<img src=x onerror=alert(1)>'), 'data.csv');
  assert.equal(result.sheets.length, 1);
  const cells = result.sheets[0].cells;
  assert.deepEqual(cells.find(cell => cell[0] === 1 && cell[1] === 0).slice(2, 4), ['0007', '0007']);
  assert.equal(cells.find(cell => cell[0] === 1 && cell[1] === 1)[2], 'hello, world');
  assert.equal(cells.find(cell => cell[0] === 2 && cell[1] === 0)[2], '=1+1');
  assert.equal(cells.find(cell => cell[0] === 2 && cell[1] === 1)[2], '<img src=x onerror=alert(1)>');
});

test('TSV reads Unicode and quoted embedded newlines', async () => {
  const result = await parse(text('名称\t备注\n设计\t"第一行\n第二行"'), 'data.tsv');
  assert.equal(result.sheets[0].rows, 2);
  assert.equal(result.sheets[0].cells.find(cell => cell[0] === 1 && cell[1] === 1)[2], '第一行\n第二行');
});

test('XLSX retains worksheets, absolute offsets, numeric formatting and formula cache', async () => {
  const book = XLSX.utils.book_new();
  const sheet = {
    B3: { t: 'n', v: 12.5, z: '0.00' },
    C3: { t: 'n', v: 15, f: 'B3+2.5' },
    D3: { t: 'n', v: 46299, z: 'yyyy-mm-dd' },
    '!ref': 'B3:D3'
  };
  XLSX.utils.book_append_sheet(book, sheet, '预算');
  XLSX.utils.book_append_sheet(book, XLSX.utils.aoa_to_sheet([['费用', '金额'], ['云资源', 100]]), '费用');
  const result = await parse(new Uint8Array(XLSX.write(book, { type: 'array', bookType: 'xlsx' })), '预算.xlsx');
  assert.deepEqual(result.sheets.map(sheet => sheet.name), ['预算', '费用']);
  assert.equal(result.sheets[0].firstRow, 2);
  assert.equal(result.sheets[0].firstColumn, 1);
  assert.equal(result.sheets[0].cells[0][2], '12.50');
  assert.deepEqual(result.sheets[0].cells[1].slice(2), ['15', '=B3+2.5', false]);
  assert.match(result.sheets[0].cells[2][2], /^\d{4}-\d{2}-\d{2}$/);
});

test('formula without a cached value remains explicit and is never recalculated', async () => {
  const result = await parse(workbook({ A1: { t: 'n', f: '1+2' }, '!ref': 'A1' }), 'formula.xlsx');
  assert.deepEqual(result.sheets[0].cells[0].slice(2), ['—', '=1+2', true]);
});

test('limits reject oversized files, invalid UTF-8, huge ranges and excessive cell text', async () => {
  await assert.rejects(() => parse(new Uint8Array(parser.limits.bytes + 1), 'large.csv'), /5 MiB/);
  await assert.rejects(() => parse(new Uint8Array([0xff]), 'bad.csv'));
  await assert.rejects(() => parse(text('x'), 'wrong.pdf'), /Unsupported/);
  await assert.rejects(() => parse(text('a'.repeat(parser.limits.text + 1)), 'long.csv'), /Cell content/);
  await assert.rejects(() => parse(workbook({ A1: { t: 's', v: 'value' }, '!ref': 'A1:J50002' }), 'huge.xlsx'), /dimensions/);
  await assert.rejects(() => parse(text('not a zip'), 'bad.xlsx'), /archive/);
});

test('ZIP preflight rejects declared expansion bombs and encrypted entries before parsing', async () => {
  const bytes = workbook(XLSX.utils.aoa_to_sheet([['x']]));
  let directory = 0;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  while (directory < bytes.length - 4 && view.getUint32(directory, true) !== 0x02014b50) directory++;
  const expanded = bytes.slice();
  new DataView(expanded.buffer).setUint32(directory + 24, parser.limits.expanded + 1, true);
  await assert.rejects(() => parse(expanded, 'bomb.xlsx'), /32 MiB/);
  const encrypted = bytes.slice();
  new DataView(encrypted.buffer).setUint16(directory + 8, 1, true);
  await assert.rejects(() => parse(encrypted, 'encrypted.xlsx'), /Encrypted/);
});

test('visible window stays bounded for large sheets and coordinates retain Excel labels', async () => {
  assert.deepEqual(plain(parser.visibleRange(320000, 352, 32, 50000)), [9999, 10012]);
  assert.deepEqual(plain(parser.visibleRange(0, 352, 32, 0)), [0, 0]);
  assert.equal(parser.columnName(0), 'A');
  assert.equal(parser.columnName(26), 'AA');
  assert.equal(parser.columnName(16383), 'XFD');
});


test('compressed XLSX parses and dishonest expanded-size headers cannot bypass the actual limit', async () => {
  const book = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(book, XLSX.utils.aoa_to_sheet([['正常压缩文件', 42]]), '数据');
  const valid = new Uint8Array(XLSX.write(book, { type: 'array', bookType: 'xlsx', compression: true }));
  assert.equal((await parse(valid, 'compressed.xlsx')).sheets[0].cells[0][2], '正常压缩文件');
  // A small archive lying about a 32 MiB expansion must fail before SheetJS runs.
  const compressed = deflateRawSync(Buffer.alloc(parser.limits.expanded + 1, 97));
  const local = Buffer.alloc(31), directory = Buffer.alloc(47), end = Buffer.alloc(22);
  local.writeUInt32LE(0x04034b50); local.writeUInt16LE(8, 8);
  local.writeUInt32LE(compressed.length, 18); local.writeUInt32LE(1, 22); local.writeUInt16LE(1, 26); local[30] = 120;
  directory.writeUInt32LE(0x02014b50); directory.writeUInt16LE(8, 10);
  directory.writeUInt32LE(compressed.length, 20); directory.writeUInt32LE(1, 24); directory.writeUInt16LE(1, 28); directory[46] = 120;
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(1, 8); end.writeUInt16LE(1, 10);
  end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(local.length + compressed.length, 16);
  await assert.rejects(() => parse(new Uint8Array(Buffer.concat([local, compressed, directory, end])), 'dishonest.xlsx'), /32 MiB/);
});


test('CSV first cells cannot be autodetected as HTML, XML or SYLK', async () => {
  for (const value of ['<img src=x onerror=alert(1)>', 'ID;literal', '<?xml version=1?>']) {
    const result = await parse(text(value + ',0007'), 'text.csv');
    assert.equal(result.sheets[0].cells[0][2], value);
    assert.equal(result.sheets[0].cells[1][2], '0007');
  }
});

test('delimited text keeps empty coordinates, CRLF, escaped quotes and rejects broken quoting', async () => {
  const result = await parse(text(',"a""b"\r\n0007,"line one\r\nline two"\r\n'), 'quoted.csv');
  assert.equal(result.sheets[0].rows, 2);
  assert.equal(result.sheets[0].columns, 2);
  assert.deepEqual(result.sheets[0].cells[0].slice(0, 3), [0, 1, 'a"b']);
  assert.equal(result.sheets[0].cells[2][2], 'line one\r\nline two');
  assert.equal((await parse(text(''), 'empty.csv')).sheets[0].rows, 0);
  await assert.rejects(() => parse(text('"open'), 'bad.csv'), /Unclosed/);
  await assert.rejects(() => parse(text('"closed"unexpected'), 'bad.csv'), /Unexpected/);
});

test('small XLSX ranges at high absolute rows keep values, formulas and original coordinates', async () => {
  for (const row of [50001, 50002, 60000, 1048576]) {
    const result = await parse(workbook({
      ['B' + row]: { t: 'n', v: 42 },
      ['C' + row]: { t: 'n', v: 43, f: 'B' + row + '+1' },
      '!ref': 'B' + row + ':C' + row,
    }), 'offset.xlsx');
    assert.deepEqual(result.sheets[0], {
      name: '预算', firstRow: row - 1, firstColumn: 1, rows: 1, columns: 2,
      cells: [[0, 0, '42', '42', false], [0, 1, '43', '=B' + row + '+1', false]],
    });
  }
  await assert.rejects(() => parse(workbook({
    A1: { t: 's', v: 'first' }, A60000: { t: 's', v: 'last' }, '!ref': 'A1:A60000',
  }), 'oversized-span.xlsx'), /dimensions/);
});

test('stored XLSX entry content must match its CRC before parsing', async () => {
  const valid = workbook(XLSX.utils.aoa_to_sheet([['ORIGINAL']]));
  assert.equal((await parse(valid, 'valid.xlsx')).sheets[0].cells[0][2], 'ORIGINAL');
  const damaged = valid.slice();
  const position = Buffer.from(damaged).indexOf('ORIGINAL');
  assert.ok(position >= 0);
  damaged.set(text('MUTATED!'), position);
  await assert.rejects(() => parse(damaged, 'damaged.xlsx'), /CRC/);
});

test('compressed XLSX CRC is checked even when its size and deflate stream remain valid', async () => {
  const book = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(book, XLSX.utils.aoa_to_sheet([['value', 42]]), '数据');
  const valid = new Uint8Array(XLSX.write(book, { type: 'array', bookType: 'xlsx', compression: true }));
  assert.equal((await parse(valid, 'valid.xlsx')).sheets[0].cells[1][2], '42');
  const damaged = valid.slice(), view = new DataView(damaged.buffer);
  const end = damaged.length - 22, directory = view.getUint32(end + 16, true);
  assert.equal(view.getUint16(directory + 10, true), 8);
  const local = view.getUint32(directory + 42, true);
  const wrongCRC = (view.getUint32(directory + 16, true) ^ 1) >>> 0;
  view.setUint32(directory + 16, wrongCRC, true);
  view.setUint32(local + 14, wrongCRC, true);
  await assert.rejects(() => parse(damaged, 'damaged.xlsx'), /CRC/);
  const mismatchedHeader = valid.slice();
  new DataView(mismatchedHeader.buffer).setUint32(local + 14, wrongCRC, true);
  await assert.rejects(() => parse(mismatchedHeader, 'mismatched.xlsx'), /metadata|CRC/);
});


test('sparse maximum worksheet visits only stored cells, including both range edges', async () => {
  const bytes = workbook({ A1: { t: 's', v: 'start' }, IV50000: { t: 's', v: 'end' }, '!ref': 'A1:IV50000' });
  const originalRead = XLSX.read;
  let visits = 0;
  XLSX.read = (...args) => {
    const book = originalRead(...args);
    for (const name of book.SheetNames) book.Sheets[name] = new Proxy(book.Sheets[name], {
      get(sheet, key) {
        if (typeof key === 'string' && /^[A-Z]+[1-9][0-9]*$/.test(key) && ++visits > 16)
          throw new Error('Sparse worksheet scanned empty coordinates');
        return Reflect.get(sheet, key);
      },
    });
    return book;
  };
  try {
    const sheet = (await parse(bytes, 'sparse.xlsx')).sheets[0];
    assert.equal(sheet.rows, 50000); assert.equal(sheet.columns, 256);
    assert.deepEqual(sheet.cells.map(cell => cell.slice(0, 4)), [[0, 0, 'start', 'start'], [49999, 255, 'end', 'end']]);
    assert.ok(visits <= 16);
  } finally { XLSX.read = originalRead; }
});

test('error cells show readable errors in values while formula errors retain their formula', async () => {
  const sheet = (await parse(workbook({ A1: { t: 'e', v: 7 }, B1: { t: 'e', v: 7, f: '1/0' }, '!ref': 'A1:B1' }), 'errors.xlsx')).sheets[0];
  assert.deepEqual(sheet.cells[0].slice(2), ['#DIV/0!', '#DIV/0!', false]);
  assert.deepEqual(sheet.cells[1].slice(2), ['#DIV/0!', '=1/0', false]);
});
