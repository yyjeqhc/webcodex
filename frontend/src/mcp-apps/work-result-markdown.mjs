import MarkdownIt from "markdown-it";

const parser = new MarkdownIt({ html: false, linkify: false, typographer: false, maxNesting: 32 });
// Preview links stay inert, even when their destination is safe to display.
parser.validateLink = url => /^https?:\/\//i.test(url) && !/[\u0000-\u0020\u007f]/.test(url);
const tags = new Set(["p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "ul", "ol", "li", "em", "strong", "s", "table", "thead", "tbody", "tr", "th", "td"]);

export function render(target, source, document) {
  target.replaceChildren();
  let count = 0;
  const text = (parent, value) => {
    const node = document.createElement("span"); node.textContent = value; parent.append(node);
  };
  const walk = (items, root) => {
    const stack = [root];
    for (const token of items) {
      if (++count > 12000) throw new Error("Markdown display limit");
      const parent = stack[stack.length - 1];
      if (token.hidden) continue;
      if (token.type === "inline") { walk(token.children || [], parent); continue; }
      if (token.type === "image") {
        text(parent, `[图片：${token.content || "图片"} · 外部图片未加载]`); continue;
      }
      if (token.type === "link_open") {
        const node = document.createElement("span"); node.className = "markdown-link";
        const url = token.attrGet("href");
        if (url && parser.validateLink(url)) node.setAttribute("title", `${url} · 仅显示链接，不打开外部资源`);
        parent.append(node); stack.push(node); continue;
      }
      if (token.type === "link_close") { if (stack.length > 1) stack.pop(); continue; }
      if (token.nesting === -1) { if (tags.has(token.tag) && stack.length > 1) stack.pop(); continue; }
      if (token.nesting === 1 && tags.has(token.tag)) {
        const node = document.createElement(token.tag);
        if (token.tag === "ol") {
          const start = Number(token.attrGet("start"));
          if (Number.isSafeInteger(start) && start > 0) node.setAttribute("start", String(start));
        }
        parent.append(node); stack.push(node); continue;
      }
      if (["fence", "code_block"].includes(token.type)) {
        const pre = document.createElement("pre"); const code = document.createElement("code");
        code.textContent = token.content; pre.append(code); parent.append(pre); continue;
      }
      if (token.type === "code_inline") {
        const node = document.createElement("code"); node.textContent = token.content; parent.append(node); continue;
      }
      if (["softbreak", "hardbreak"].includes(token.type)) { parent.append(document.createElement("br")); continue; }
      if (token.type === "hr") { parent.append(document.createElement("hr")); continue; }
      text(parent, token.content || "");
    }
  };
  try { walk(parser.parse(source, {}), target); }
  catch (_) {
    target.replaceChildren();
    text(target, "已达到 Markdown 显示上限 · 完整内容显示在下方");
    const pre = document.createElement("pre"); pre.textContent = source; target.append(pre);
  }
}
