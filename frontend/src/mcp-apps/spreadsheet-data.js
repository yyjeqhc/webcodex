/* Data extraction only. This source and SheetJS run in a disposable Worker. */
const WebCodexSpreadsheet = (() => {
  const limits = Object.freeze({ bytes: 5 * 1024 * 1024, expanded: 32 * 1024 * 1024, entries: 4096, sheets: 32, rows: 50000, columns: 256, cells: 200000, text: 32768 });
  const fail = message => { throw new Error(message); };

  const crcTable = Array.from({ length: 256 }, (_, index) => {
    for (let bit = 0; bit < 8; bit++) index = (index >>> 1) ^ (index & 1 ? 0xedb88320 : 0);
    return index >>> 0;
  });
  function updateCRC(crc, bytes) {
    for (const byte of bytes) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 255];
    return crc >>> 0;
  }

  function checkZip(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const u16 = offset => view.getUint16(offset, true);
    const u32 = offset => view.getUint32(offset, true);
    let end = bytes.length - 22;
    const start = Math.max(0, end - 65535);
    while (end >= start && (u32(end) !== 0x06054b50 || end + 22 + u16(end + 20) !== bytes.length)) end--;
    if (end < start) fail('Invalid XLSX archive');
    const count = u16(end + 10), size = u32(end + 12), offset = u32(end + 16);
    if (u16(end + 4) || u16(end + 6) || u16(end + 8) !== count || count > limits.entries || count === 65535 || offset + size !== end) fail('Unsupported or excessive XLSX archive');
    let cursor = offset, expanded = 0;
    const entries = [];
    for (let index = 0; index < count; index++) {
      if (cursor + 46 > end || u32(cursor) !== 0x02014b50) fail('Invalid XLSX directory');
      const flags = u16(cursor + 8), method = u16(cursor + 10), crc = u32(cursor + 16);
      const compressed = u32(cursor + 20), uncompressed = u32(cursor + 24), local = u32(cursor + 42);
      const next = cursor + 46 + u16(cursor + 28) + u16(cursor + 30) + u16(cursor + 32);
      if ((flags & 1) || ![0, 8].includes(method) || local + 30 > offset || next > end || compressed === 0xffffffff || uncompressed === 0xffffffff) fail('Encrypted or unsupported XLSX archive');
      if (u32(local) !== 0x04034b50 || local + 30 + u16(local + 26) + u16(local + 28) + compressed > offset) fail('Invalid XLSX entry');
      expanded += uncompressed;
      if (expanded > limits.expanded) fail('XLSX expanded content exceeds 32 MiB');
      const data = local + 30 + u16(local + 26) + u16(local + 28);
      if (u16(local + 6) !== flags || u16(local + 8) !== method || (!(flags & 8) && (u32(local + 14) !== crc || u32(local + 18) !== compressed || u32(local + 22) !== uncompressed))) fail('Inconsistent XLSX entry metadata');
      if (method === 0 && compressed !== uncompressed) fail('Invalid stored XLSX entry size');
      entries.push({ data, compressed, uncompressed, method, crc });
      cursor = next;
    }
    if (cursor !== end) fail('Invalid XLSX directory length');
    return entries;
  }

  async function verifyEntries(bytes, entries) {
    let total = 0;
    for (const entry of entries) {
      if (entry.method === 0) {
        total += entry.uncompressed;
        const crc = updateCRC(0xffffffff, bytes.subarray(entry.data, entry.data + entry.compressed));
        if (((crc ^ 0xffffffff) >>> 0) !== entry.crc) fail('XLSX entry CRC does not match its content');
        continue;
      }
      let decompressor;
      try { decompressor = new DecompressionStream('deflate-raw'); }
      catch (_) { fail('This Host cannot safely decompress XLSX files'); }
      // Stream and discard before SheetJS parses; verify actual sizes and CRCs.
      const reader = new Blob([bytes.subarray(entry.data, entry.data + entry.compressed)]).stream().pipeThrough(decompressor).getReader();
      let size = 0, crc = 0xffffffff;
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          size += value.byteLength; total += value.byteLength;
          if (total > limits.expanded) fail('XLSX expanded content exceeds 32 MiB');
          crc = updateCRC(crc, value);
        }
        if (size !== entry.uncompressed) fail('XLSX decompressed size does not match its directory');
        if (((crc ^ 0xffffffff) >>> 0) !== entry.crc) fail('XLSX entry CRC does not match its content');
      } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
    }
  }

  function parseDelimited(input, delimiter) {
    const cells = [];
    let row = 0, column = 0, columns = 0, value = '', quoted = false, closed = false, textSize = 0;
    function field() {
      if (row >= limits.rows || column >= limits.columns) fail('Worksheet dimensions exceed reader limits');
      if (value) {
        if (cells.length >= limits.cells) fail('Workbook exceeds 200,000 populated cells');
        textSize += value.length * 2;
        if (textSize > limits.expanded) fail('Workbook cell text exceeds reader limits');
        cells.push([row, column, value, value, false]);
      }
      columns = Math.max(columns, ++column); value = ''; closed = false;
    }
    for (let index = 0; index < input.length; index++) {
      const char = input[index];
      if (quoted) {
        if (char === '"') {
          if (input[index + 1] === '"') { value += '"'; index++; }
          else { quoted = false; closed = true; }
        } else value += char;
      } else if (char === delimiter) field();
      else if (char === '\r' || char === '\n') {
        field(); row++; column = 0;
        if (char === '\r' && input[index + 1] === '\n') index++;
      } else {
        if (closed) fail('Unexpected text after a quoted CSV/TSV field');
        if (char === '"' && value === '') quoted = true;
        else value += char;
      }
      if (value.length > limits.text) fail('Cell content exceeds reader limits');
    }
    if (quoted) fail('Unclosed quoted CSV/TSV field');
    if (input.length && !/[\r\n]$/.test(input) || column || value || closed) { field(); row++; }
    return { sheets: [{ name: 'Sheet1', firstRow: 0, firstColumn: 0, rows: row, columns, cells }] };
  }

  async function parse(bytes, filename) {
    if (!(bytes instanceof Uint8Array) || bytes.length > limits.bytes) fail('Spreadsheet exceeds 5 MiB');
    const extension = filename.split('.').pop().toLowerCase();
    if (!['csv', 'tsv', 'xlsx'].includes(extension)) fail('Unsupported spreadsheet format');
    if (extension === 'xlsx') await verifyEntries(bytes, checkZip(bytes));
    // sheetRows truncates absolute row numbers, not the size of the used range.
    // Sparse parsing retains high coordinates without allocating rows up to them.
    const options = { type: 'array', dense: false, cellHTML: false, cellFormula: true, cellText: true, cellNF: false, cellStyles: false, bookVBA: false };
    if (extension !== 'xlsx') {
      // Never autodetect other document formats from user-controlled CSV text.
      const input = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      return parseDelimited(input, extension === 'tsv' ? '\t' : ',');
    }
    const workbook = XLSX.read(bytes, options);
    if (workbook.SheetNames.length > limits.sheets) fail('Workbook exceeds 32 worksheets');
    let count = 0, textBytes = 0;
    const sheets = workbook.SheetNames.map(name => {
      const source = workbook.Sheets[name];
      const ref = source['!ref'];
      if (!ref) return { name, firstRow: 0, firstColumn: 0, rows: 0, columns: 0, cells: [] };
      const range = XLSX.utils.decode_range(ref);
      const rows = range.e.r - range.s.r + 1, columns = range.e.c - range.s.c + 1;
      if (![range.s.r, range.s.c, range.e.r, range.e.c, rows, columns].every(Number.isSafeInteger) || range.s.r < 0 || range.s.c < 0 || rows < 1 || columns < 1 || rows > limits.rows || columns > limits.columns || range.e.r >= 1048576 || range.e.c >= 16384) fail('Worksheet dimensions exceed reader limits');
      const cells = [];
      for (const address of Object.keys(source)) {
        if (address.startsWith('!')) continue;
        if (!/^[A-Z]+[1-9][0-9]*$/.test(address)) fail('Invalid worksheet cell coordinate');
        const { r: row, c: column } = XLSX.utils.decode_cell(address);
        if (!Number.isSafeInteger(row) || !Number.isSafeInteger(column) || row < range.s.r || row > range.e.r || column < range.s.c || column > range.e.c) fail('Cell coordinate is outside its worksheet range');
        const cell = source[address];
        if (!cell || cell.t === 'z') continue;
        if (++count > limits.cells) fail('Workbook exceeds 200,000 populated cells');
        const formula = cell.f === undefined ? '' : '=' + String(cell.f);
        const missingResult = !!formula && cell.v === undefined;
        const display = missingResult ? '—' : cell.t === 'e' ? String(cell.w ?? XLSX.utils.format_cell(cell) ?? '#ERROR!') : String(cell.w ?? cell.v ?? '');
        const raw = cell.t === 'e' ? display : cell.v === undefined ? '' : String(cell.v);
        if ([display, raw, formula].some(value => value.length > limits.text)) fail('Cell content exceeds reader limits');
        textBytes += display.length + raw.length + formula.length;
        if (textBytes > limits.expanded) fail('Workbook cell text exceeds reader limits');
        cells.push([row - range.s.r, column - range.s.c, display, formula || raw, missingResult]);
      }
      cells.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
      return { name, firstRow: range.s.r, firstColumn: range.s.c, rows, columns, cells };
    });
    return { sheets };
  }

  function columnName(index) {
    let name = '';
    for (index++; index > 0; index = Math.floor((index - 1) / 26)) name = String.fromCharCode(65 + (index - 1) % 26) + name;
    return name;
  }
  function visibleRange(scroll, size, itemSize, count) {
    const first = Math.max(0, Math.floor(scroll / itemSize) - 1);
    return [first, Math.min(count, Math.ceil((scroll + size) / itemSize) + 1)];
  }
  return { parse, checkZip, columnName, visibleRange, limits };
})();
