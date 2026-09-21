import fs from 'node:fs/promises';
export class FakeBackend {
  mode = 'local-cdp';
  calls = [];
  timeOrigin = 1;
  tabs = [
    { targetId: 'AAAA', tabId: 't1', title: 'about:blank', url: 'about:blank', type: 'page', active: true },
    { targetId: 'BBBB', tabId: 't2', title: 'Existing tab', url: 'https://example.test/private?q=secret#fragment', type: 'page', active: false },
  ];
  tree = '- textbox "Name" [ref=e1]\n- button "Greet" [ref=e2]\n- heading "Title" [ref=e3]\n- checkbox "Agree" [ref=e4]\n- combobox "Plan" [ref=e5]';
  refs = { e1: { role: 'textbox', name: 'Name' }, e2: { role: 'button', name: 'Greet' }, e3: { role: 'heading', name: 'Title' },
    e4: { role: 'checkbox', name: 'Agree' }, e5: { role: 'combobox', name: 'Plan' } };
  interactiveTree = undefined;
  interactiveRefs = undefined;
  async version() { return '0.38.1'; }
  async shutdown() {}
  async run(tokens, effect = false, attach = true) {
    this.calls.push({ tokens: [...tokens], effect, attach });
    if (tokens[0] === 'tab') {
      if (tokens[1] === 'list') return { tabs: structuredClone(this.tabs) };
      if (tokens[1] === 'new') {
        const targetId = 'CC' + this.tabs.length;
        for (const t of this.tabs) t.active = false;
        this.tabs.push({ targetId, tabId: 't' + (this.tabs.length + 1), title: 'New page', url: tokens[2], type: 'page', active: true });
        return { targetId, url: tokens[2] };
      }
      if (tokens[1] === 'close') { this.tabs = this.tabs.filter(t => t.targetId !== tokens[2]); if (this.tabs[0]) this.tabs[0].active = true; return { closed: true }; }
      for (const t of this.tabs) t.active = t.targetId === tokens[1];
      return {};
    }
    if (tokens[0] === 'eval') return { result: JSON.stringify({ url: this.tabs.find(t => t.active).url, timeOrigin: this.timeOrigin }) };
    if (tokens[0] === 'snapshot') {
      const compact = tokens.includes('-i') && this.interactiveTree !== undefined;
      return { snapshot: compact ? this.interactiveTree : this.tree,
        refs: structuredClone(compact && this.interactiveRefs !== undefined ? this.interactiveRefs : this.refs) };
    }
    if (tokens[0] === 'open') { this.tabs.find(t => t.active).url = tokens[1]; this.timeOrigin++; return {}; }
    if (tokens[0] === 'reload') { this.timeOrigin++; return {}; }
    if (tokens[0] === 'screenshot') {
      const png = Buffer.alloc(24);
      Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]).copy(png); png.writeUInt32BE(800, 16); png.writeUInt32BE(600, 20);
      await fs.writeFile(tokens[1], png); return {};
    }
    return {};
  }
}
