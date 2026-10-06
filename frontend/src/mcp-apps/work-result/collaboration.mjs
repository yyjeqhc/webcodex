// Display-only transcript renderer. IDs are UI keys, never execution authority.
const KINDS = { guidance: 'Guidance', question: 'Question', note: 'Note', todo: 'Request',
  answer: 'Answer', decision: 'Decision', risk: 'Risk', progress: 'Progress', proposal: 'Proposal' };
export function createCollaborationRenderer({document, root, formatAge, deliveryLabel}) {
  const cache = new Map();
  const node = (tag, className) => { const item = document.createElement(tag); item.className = className; return item; };
  return function render(rows, available) {
    const byId = new Map(rows.map(row => [row.message_id, row]));
    const answered = new Set(rows.map(row => row.reply_to_message_id).filter(Boolean));
    const nearEnd = !Number.isFinite(root.scrollHeight) || root.scrollHeight - root.scrollTop - root.clientHeight < 48;
    const height = root.scrollHeight || 0, top = root.scrollTop || 0;
    const beforeFirst = root.children[0]?.getAttribute('data-message-id');
    const keep = new Set();
    for (const row of rows) {
      let entry = cache.get(row.message_id);
      if (!entry) {
        const article = node('article', 'message');
        article.setAttribute('data-message-id', row.message_id);
        const header = node('div', 'message-meta'), author = node('strong', ''), time = node('time', '');
        const quote = node('blockquote', 'message-reply'), copy = node('div', 'message-copy'), footer = node('div', 'message-meta');
        header.append(author, time); article.append(header, quote, copy, footer);
        entry = {article, author, time, quote, copy, footer, fingerprint: null}; cache.set(row.message_id, entry);
      }
      const reply = byId.get(row.reply_to_message_id);
      const fingerprint = JSON.stringify([row, reply?.message, answered.has(row.message_id)]);
      if (entry.fingerprint !== fingerprint) {
        entry.article.className = `message ${row.source === 'operator' ? 'user' : 'agent'} kind-${row.kind || 'note'}`;
        entry.author.textContent = row.source === 'operator' ? 'You → This Window'
          : row.source === 'window' ? 'This Window → You'
          : row.direction === 'outbound' ? 'This Window → Peer' : 'Peer → This Window';
        if (row.peer_id) entry.author.title = row.peer_id;
        entry.time.setAttribute('datetime', new Date(row.created_at_ms).toISOString());
        entry.time.title = new Date(row.created_at_ms).toLocaleString();
        entry.quote.hidden = !row.reply_to_message_id;
        const quote = row.reply_to_message_id ? 'Reply to · ' + (reply ? reply.message.slice(0, 180) : 'Earlier message') : '';
        if (entry.quote.textContent !== quote) entry.quote.textContent = quote;
        // ACK/receipt changes never recreate the body or erase a reader's selection.
        if (entry.copy.textContent !== row.message) entry.copy.textContent = row.message;
        const kind = node('span', 'message-kind'); kind.textContent = KINDS[row.kind] || 'Note';
        entry.footer.replaceChildren(kind);
        for (const text of [row.priority === 'high' ? 'High priority' : null, deliveryLabel(row),
          answered.has(row.message_id) ? 'Reply received' : null,
          row.context_session_id ? 'Session · ' + row.context_session_id.slice(8, 14) : null]) {
          if (!text) continue;
          const badge = node('span', 'message-state'); badge.textContent = text; entry.footer.append(badge);
        }
        entry.fingerprint = fingerprint;
      }
      entry.time.textContent = formatAge(row.created_at_ms);
      keep.add(entry.article);
    }
    for (const item of [...root.children]) if (!keep.has(item)) item.remove();
    for (const [id] of cache) if (!byId.has(id)) cache.delete(id);
    rows.forEach((row, index) => {
      const item = cache.get(row.message_id).article;
      if (root.children[index] !== item) root.insertBefore(item, root.children[index] || null);
    });
    if (!rows.length) {
      const empty = node('div', 'empty'); empty.textContent = available ? 'No messages yet' : 'Collaboration unavailable'; root.replaceChildren(empty);
    }
    if (beforeFirst && rows[0]?.message_id !== beforeFirst && byId.has(beforeFirst)) root.scrollTop = top + Math.max(0, root.scrollHeight - height);
    else if (nearEnd) root.scrollTop = root.scrollHeight;
  };
}
