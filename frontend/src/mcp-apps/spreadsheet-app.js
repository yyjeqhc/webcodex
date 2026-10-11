(() => {
  const el = id => document.getElementById(id);
  const maxBytes = 5 * 1024 * 1024, chunkBytes = 512 * 1024;
  const pending = new Map();
  let nextId = 0, initialized = false, disposed = false, generation = 0;
  let source = null, loadingEpoch = -1, worker = null, workbook = null, sheetIndex = -1;
  let cells = new Map(), selected = [0, 0], frame = null;
  const sheetPositions = new Map();
  const rowHeight = 32, columnWidth = 144, gutter = 44;
  const { columnName, visibleRange } = WebCodexSpreadsheet;
  const status = message => { el('status').textContent = message; };
  const retry = visible => { el('retry').hidden = !visible; };
  function notify(method, params) {
    if (!disposed) parent.postMessage({ jsonrpc: '2.0', method, params }, '*');
  }
  function rpc(method, params, timeout = 20000) {
    if (disposed) return Promise.reject(new Error('Reader closed'));
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(new Error('Host request timed out')); }, timeout);
      pending.set(id, { method, resolve, reject, timer });
      parent.postMessage({ jsonrpc: '2.0', id, method, params }, '*');
    });
  }
  function theme(context) {
    if (context?.theme === 'light' || context?.theme === 'dark') document.documentElement.style.colorScheme = context.theme;
    if (context?.displayMode) document.documentElement.dataset.mode = context.displayMode;
    const variables = context?.styles?.variables;
    if (variables && typeof variables === 'object') {
      for (const [key, value] of Object.entries(variables)) {
        if (/^--[a-zA-Z0-9-]{1,100}$/.test(key) && typeof value === 'string' && value.length <= 256) document.documentElement.style.setProperty(key, value);
      }
    }
    scheduleRender();
  }
  function privateMetadata(result, key) {
    for (const candidate of [result, result?.result, result?.toolResult, result?.tool_result]) {
      const metadata = candidate?._meta?.[key] ?? candidate?.meta?.[key];
      if (metadata && typeof metadata === 'object') return metadata;
    }
    return null;
  }
  function envelope(result, privateKey) {
    for (const candidate of [result, result?.result, result?.toolResult, result?.tool_result]) {
      if (!candidate || typeof candidate !== 'object') continue;
      if (typeof candidate.success === 'boolean') return candidate;
      const structured = candidate.structuredContent ?? candidate.structured_content;
      if (structured && typeof structured === 'object') return structured;
      const privateResult = privateKey && privateMetadata(candidate, privateKey);
      if (privateResult) return privateResult;
      for (const block of candidate.content || []) {
        if (block.type === 'text' && typeof block.text === 'string' && block.text.length <= 400000) {
          try { const parsed = JSON.parse(block.text); if (typeof parsed?.success === 'boolean') return parsed; } catch (_) {}
        }
      }
    }
    return null;
  }
  function cancelReads() {
    for (const [id, entry] of pending) if (entry.method === 'tools/call') {
      clearTimeout(entry.timer); pending.delete(id); entry.reject(new Error('Spreadsheet selection changed'));
    }
  }
  function invalidateSource(message) {
    generation++; cancelReads(); stopWorker(); clearView(); source = null; retry(false);
    status(String(message).slice(0, 512));
  }
  function takeSource(result) {
    if (disposed) return;
    const value = envelope(result, 'webcodex/spreadsheetSource');
    if (!value?.success) { invalidateSource(value?.error || 'This Host did not deliver a readable spreadsheet source'); return; }
    const output = value.output;
    if (!output || typeof output.project !== 'string' || !output.project || output.project.length > 512
      || typeof output.path !== 'string' || !output.path || output.path.length > 512 || !/\.(csv|tsv|xlsx)$/i.test(output.path)
      || typeof output.name !== 'string' || !output.name || output.name.length > 4096
      || !Number.isSafeInteger(output.bytes) || output.bytes > maxBytes || output.bytes < 0 || !/^[0-9a-f]{64}$/.test(output.sha256)) {
      invalidateSource('Invalid spreadsheet source'); return;
    }
    const key = JSON.stringify([output.project, output.path, output.bytes, output.sha256]);
    if (source?.key === key) return;
    generation++; cancelReads(); stopWorker(); clearView();
    source = { key, project: output.project, path: output.path, name: output.name, bytes: output.bytes, sha256: output.sha256 };
    el('filename').textContent = output.name;
    retry(false); status('Loading spreadsheet…');
    if (initialized) void load();
  }
  function clearView() {
    workbook = null; cells.clear(); sheetPositions.clear(); sheetIndex = -1; selected = [0, 0];
    el('sheets').replaceChildren(); el('cells').replaceChildren(); el('columns').replaceChildren();
    el('address').textContent = ''; el('value').textContent = ''; el('grid').hidden = true;
    el('viewport').scrollTop = 0; el('viewport').scrollLeft = 0;
  }
  function stopWorker() { if (worker) { worker.cancel(); worker = null; } }
  function parse(bytes, filename) {
    return new Promise((resolve, reject) => {
      let url;
      try {
        url = URL.createObjectURL(new Blob([el('spreadsheet-worker').textContent], { type: 'text/javascript' }));
        worker = { instance: new Worker(url), cancel: null };
      } catch (_) { if (url) URL.revokeObjectURL(url); reject(new Error('This Host cannot start the spreadsheet parser')); return; }
      URL.revokeObjectURL(url);
      const current = worker;
      const timer = setTimeout(() => { finish(); reject(new Error('Spreadsheet parsing exceeded 10 seconds')); }, 10000);
      const finish = () => { clearTimeout(timer); current.instance.terminate(); if (worker === current) worker = null; };
      current.cancel = () => { finish(); reject(new Error('Spreadsheet selection changed')); };
      current.instance.onmessage = event => { finish(); event.data.error ? reject(new Error(event.data.error)) : resolve(event.data.workbook); };
      current.instance.onerror = () => { finish(); reject(new Error('Spreadsheet parser failed')); };
      current.instance.postMessage({ bytes: bytes.buffer, filename }, [bytes.buffer]);
    });
  }
  async function load() {
    if (!initialized || !source || disposed || loadingEpoch === generation) return;
    const expected = source, epoch = generation;
    loadingEpoch = epoch;
    try {
      // Match the generic App transport: one absolute budget, scaled by Host round trips.
      const deadlineMs = Math.min(15 * 60000, Math.max(120000, 60000 + Math.ceil(expected.bytes / chunkBytes) * 20000));
      const bytes = new Uint8Array(expected.bytes), deadline = performance.now() + deadlineMs;
      for (let offset = 0; offset < bytes.length;) {
        const remaining = deadline - performance.now();
        if (remaining <= 0) throw new Error('Spreadsheet transfer timed out; retry this version');
        const reply = await rpc('tools/call', { name: 'read_app_artifact_chunk', arguments: {
          project: expected.project, path: expected.path, sha256: expected.sha256, bytes: expected.bytes, byte_offset: offset,
        } }, remaining);
        if (disposed || epoch !== generation) return;
        const value = envelope(reply);
        if (!value?.success) throw new Error(value?.output?.error_kind === 'snapshot_changed'
          ? 'Spreadsheet content changed; request a new reader' : value?.error || 'Host did not deliver the spreadsheet segment');
        const page = value.output?.artifact_chunk;
        const encoded = privateMetadata(reply, 'webcodex/artifactChunk')?.content_base64;
        const length = Math.min(chunkBytes, bytes.length - offset), next = offset + length;
        if (!page || page.project !== expected.project || page.path !== expected.path || page.sha256 !== expected.sha256
          || page.bytes_total !== bytes.length || page.byte_offset !== offset
          || typeof encoded !== 'string' || encoded.length !== Math.ceil(length / 3) * 4
          || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded))
          throw new Error('Host did not deliver complete spreadsheet file data');
        const binary = atob(encoded);
        if (binary.length !== length || page.complete !== (next === bytes.length)
          || page.next_byte_offset !== (page.complete ? null : next)) throw new Error('Invalid spreadsheet segment continuation');
        for (let index = 0; index < length; index++) bytes[offset + index] = binary.charCodeAt(index);
        offset = next;
      }
      const digest = await crypto.subtle.digest('SHA-256', bytes);
      const sha = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
      if (performance.now() >= deadline) throw new Error('Spreadsheet transfer timed out; retry this version');
      if (sha !== expected.sha256) throw new Error('Spreadsheet content changed; request a new reader');
      if (disposed || epoch !== generation) return;
      const parsed = await parse(bytes, expected.name);
      if (disposed || epoch !== generation) return;
      workbook = parsed;
      workbook.sheets.forEach((sheet, index) => {
        const button = document.createElement('button');
        button.type = 'button'; button.className = 'sheet-tab'; button.setAttribute('role', 'tab');
        button.id = 'sheet-' + index; button.setAttribute('aria-controls', 'grid');
        button.textContent = sheet.name;
        button.addEventListener('click', () => showSheet(index));
        el('sheets').append(button);
      });
      if (!workbook.sheets.length) throw new Error('Workbook has no worksheets');
      retry(false); showSheet(0);
    } catch (error) {
      if (!disposed && epoch === generation) { clearView(); retry(true); status('Unable to open: ' + String(error.message || error).slice(0, 512)); }
    } finally { if (loadingEpoch === epoch) loadingEpoch = -1; }
  }
  function sheet() { return workbook?.sheets[sheetIndex]; }
  function showSheet(index) {
    if (!workbook?.sheets[index] || index === sheetIndex) return;
    const viewport = el('viewport');
    if (sheet()) sheetPositions.set(sheetIndex, { selected, top: viewport.scrollTop, left: viewport.scrollLeft });
    sheetIndex = index;
    const current = sheet();
    const position = sheetPositions.get(index);
    cells = new Map(current.cells.map(cell => [cell[0] + ':' + cell[1], cell]));
    el('grid').hidden = false;
    el('grid').setAttribute('aria-labelledby', 'sheet-' + index);
    Array.from(el('sheets').children).forEach((button, position) => button.setAttribute('aria-selected', String(position === index)));
    el('viewport').setAttribute('aria-labelledby', 'sheet-' + index);
    el('viewport').setAttribute('aria-rowcount', String(current.rows));
    el('viewport').setAttribute('aria-colcount', String(current.columns));
    el('spacer').style.width = gutter + current.columns * columnWidth + 'px';
    el('spacer').style.height = Math.max(1, current.rows * rowHeight) + 'px';
    // Selection content can change the document scrollbar and viewport width.
    select(position?.selected[0] ?? 0, position?.selected[1] ?? 0, false);
    // Size first, then clamp explicitly: old virtual cells may still overflow the spacer.
    viewport.scrollTop = Math.min(position?.top ?? 0, Math.max(0, current.rows * rowHeight - viewport.clientHeight));
    viewport.scrollLeft = Math.min(position?.left ?? 0, Math.max(0, gutter + current.columns * columnWidth - viewport.clientWidth));
    scheduleRender();
  }
  function select(row, column, reveal) {
    const current = sheet();
    if (!current) return;
    selected = [Math.max(0, Math.min(current.rows - 1, row)), Math.max(0, Math.min(current.columns - 1, column))];
    const cell = cells.get(selected[0] + ':' + selected[1]);
    el('address').textContent = current.rows ? columnName(current.firstColumn + selected[1]) + (current.firstRow + selected[0] + 1) : '';
    el('value').textContent = cell?.[3] || cell?.[2] || '';
    status(current.rows ? `${current.rows.toLocaleString()} rows × ${current.columns} columns${cell?.[4] ? ' · Formula has no cached result' : ''}` : 'Empty worksheet');
    if (reveal) {
      const viewport = el('viewport');
      const top = selected[0] * rowHeight, left = selected[1] * columnWidth;
      if (top < viewport.scrollTop) viewport.scrollTop = top;
      else if (top + rowHeight > viewport.scrollTop + viewport.clientHeight) viewport.scrollTop = top + rowHeight - viewport.clientHeight;
      if (left < viewport.scrollLeft) viewport.scrollLeft = left;
      else if (left + columnWidth > viewport.scrollLeft + viewport.clientWidth - gutter) viewport.scrollLeft = left + columnWidth - viewport.clientWidth + gutter;
    }
    el('cells').querySelectorAll('button').forEach(button => {
      const active = Number(button.dataset.row) === selected[0] && Number(button.dataset.column) === selected[1];
      button.setAttribute('aria-selected', String(active)); button.tabIndex = active ? 0 : -1;
    });
    el('viewport').tabIndex = el('cells').querySelector('[aria-selected="true"]') ? -1 : 0;
  }
  function scheduleRender() {
    if (disposed || frame !== null) return;
    frame = requestAnimationFrame(() => { frame = null; render(); });
  }
  function render() {
    const current = sheet(), viewport = el('viewport');
    if (!current || disposed) return;
    const hadFocus = viewport.contains(document.activeElement);
    const [startRow, endRow] = visibleRange(viewport.scrollTop, viewport.clientHeight, rowHeight, current.rows);
    const [startColumn, endColumn] = visibleRange(viewport.scrollLeft, viewport.clientWidth, columnWidth, current.columns);
    const content = document.createDocumentFragment(), headers = document.createDocumentFragment();
    for (let column = startColumn; column < endColumn; column++) {
      const header = document.createElement('div'); header.className = 'column-header';
      header.textContent = columnName(current.firstColumn + column);
      header.style.left = gutter + column * columnWidth - viewport.scrollLeft + 'px';
      headers.append(header);
    }
    el('columns').replaceChildren(headers);
    for (let row = startRow; row < endRow; row++) {
      const rowNode = document.createElement('div'); rowNode.setAttribute('role', 'row'); rowNode.setAttribute('aria-rowindex', String(row + 1));
      const number = document.createElement('div'); number.className = 'row-header'; number.setAttribute('aria-hidden', 'true');
      number.textContent = String(current.firstRow + row + 1);
      number.style.top = row * rowHeight + 'px'; number.style.left = viewport.scrollLeft + 'px';
      rowNode.append(number);
      for (let column = startColumn; column < endColumn; column++) {
        const cell = cells.get(row + ':' + column), button = document.createElement('button');
        const address = columnName(current.firstColumn + column) + (current.firstRow + row + 1);
        button.type = 'button'; button.className = 'cell'; button.setAttribute('role', 'gridcell');
        button.setAttribute('aria-label', address + ': ' + (cell?.[2] || 'empty'));
        button.setAttribute('aria-rowindex', String(row + 1)); button.setAttribute('aria-colindex', String(column + 1));
        button.dataset.row = String(row); button.dataset.column = String(column);
        button.style.top = row * rowHeight + 'px'; button.style.left = gutter + column * columnWidth + 'px';
        button.textContent = cell?.[2] || '';
        const active = row === selected[0] && column === selected[1];
        button.setAttribute('aria-selected', String(active)); button.tabIndex = active ? 0 : -1;
        rowNode.append(button);
      }
      content.append(rowNode);
    }
    el('cells').replaceChildren(content);
    const active = el('cells').querySelector('[aria-selected="true"]');
    // Keep one stable keyboard entry when virtualization removes the selection.
    viewport.tabIndex = active ? -1 : 0;
    if (hadFocus) (active || viewport).focus({ preventScroll: true });
    notify('ui/notifications/size-changed', { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight });
  }
  el('viewport').addEventListener('scroll', scheduleRender, { passive: true });
  el('cells').addEventListener('click', event => {
    const button = event.target.closest('button[data-row]');
    if (button) select(Number(button.dataset.row), Number(button.dataset.column), false);
  });
  el('viewport').addEventListener('keydown', event => {
    const directions = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
    if (directions[event.key]) {
      event.preventDefault(); const [row, column] = directions[event.key];
      select(selected[0] + row, selected[1] + column, true); render();
      el('cells').querySelector('[aria-selected="true"]')?.focus({ preventScroll: true });
    } else if (event.key === 'Delete' || event.key === 'Backspace' || (event.key.toLowerCase() === 'v' && (event.ctrlKey || event.metaKey))) event.preventDefault();
  });
  el('retry').addEventListener('click', () => {
    if (!disposed && initialized && source && loadingEpoch < 0) {
      retry(false); status('Loading spreadsheet…'); void load();
    }
  });
  const resizeObserver = new ResizeObserver(scheduleRender);
  resizeObserver.observe(el('viewport')); resizeObserver.observe(document.body);
  function teardown() {
    if (disposed) return;
    disposed = true; generation++; stopWorker();
    if (frame !== null) cancelAnimationFrame(frame);
    for (const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(new Error('Reader closed')); }
    pending.clear(); clearView(); source = null; resizeObserver.disconnect();
  }
  addEventListener('message', event => {
    if (event.source !== parent || disposed) return;
    const message = event.data;
    if (!message || message.jsonrpc !== '2.0') return;
    if (pending.has(message.id)) {
      const entry = pending.get(message.id); clearTimeout(entry.timer); pending.delete(message.id);
      message.error ? entry.reject(new Error(String(message.error.message || 'Host error'))) : entry.resolve(message.result);
    } else if (message.method === 'ui/notifications/tool-result') takeSource(message.params);
    else if (message.method === 'ui/notifications/host-context-changed') theme(message.params);
    else if (message.method === 'ui/resource-teardown') {
      teardown(); if ('id' in message) parent.postMessage({ jsonrpc: '2.0', id: message.id, result: {} }, '*');
    }
  });
  addEventListener('pagehide', teardown, { once: true });
  rpc('ui/initialize', { protocolVersion: '2026-01-26', appInfo: { name: 'webcodex-spreadsheet-reader', version: '1.0.0' }, appCapabilities: {} })
    .then(result => { if (disposed) return; initialized = true; theme(result?.hostContext); notify('ui/notifications/initialized', {}); if (source) void load(); })
    .catch(error => status('Host unavailable: ' + String(error.message).slice(0, 512)));
})();
