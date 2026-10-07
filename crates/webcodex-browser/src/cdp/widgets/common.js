// Shared bounded DOM mechanics for typed Browser widgets. Never model-supplied code/selectors.
const target = this;
const doc = target.ownerDocument;
const view = doc.defaultView;
const deadline = performance.now() + 3500;
const MAX_NODES = 4096;
const MAX_OPTIONS = 256;
let changed = false;
let effectStarted = false;
let opened = false;
const norm = value => String(value ?? "").normalize("NFKC").replace(/\s+/g, " ").trim();
const attr = (node, name) => node?.getAttribute?.(name) ?? "";
const role = node => attr(node, "role").toLowerCase();
const fail = kind => { throw { widgetKind: kind }; };
const tick = () => new Promise(resolve => view.setTimeout(resolve, 0));
const connected = () => {
    if (!target.isConnected || target.ownerDocument !== doc) fail("stale_element");
    if (target instanceof view.HTMLInputElement && !["text", "search"].includes(target.type)) fail("widget_not_supported");
    if (performance.now() >= deadline) fail("widget_timeout");
};
const visible = node => {
    if (!node?.isConnected || !Array.from(node.getClientRects()).some(rect => rect.width > 0 && rect.height > 0)) return false;
    const style = view.getComputedStyle(node);
    return style.display !== "none" && style.visibility !== "hidden" && style.visibility !== "collapse"
        && !node.closest('[hidden],[aria-hidden="true"],[inert]');
};
const disabled = node => !!node.disabled || attr(node, "aria-disabled") === "true"
    || /(?:^|[-_\s])disabled(?:$|[-_\s])/.test(attr(node, "class"))
    || !!node.closest('[inert],[aria-disabled="true"],[disabled]');
const walk = (root, predicate, limit = MAX_OPTIONS) => {
    const found = [];
    const walker = doc.createTreeWalker(root, view.NodeFilter.SHOW_ELEMENT);
    let count = 0;
    for (let node = root; node; node = walker.nextNode()) {
        if (++count > MAX_NODES || performance.now() >= deadline) fail("widget_bounds_exceeded");
        if (predicate(node)) {
            if (found.length === limit) fail("widget_bounds_exceeded");
            found.push(node);
        }
    }
    return found;
};
const labels = node => {
    const text = node instanceof view.HTMLInputElement ? node.value : node.textContent;
    return [attr(node, "data-value"), attr(node, "value"), attr(node, "aria-label"),
        attr(node, "title"), attr(node, "datetime"), text].filter(value => value && value.length <= 4096).map(norm);
};
let host = target;
for (let ancestor = target.parentElement, depth = 0; ancestor && depth < 3; ancestor = ancestor.parentElement, depth++) {
    if (["combobox", "listbox"].includes(role(ancestor)) || attr(ancestor, "aria-haspopup")
        || /\b(?:ant-picker|ant-select|ant-cascader|el-date-editor|el-select|el-cascader|react-datepicker|flatpickr)\b/.test(attr(ancestor, "class"))) {
        host = ancestor;
        break;
    }
}
const inputControls = () => walk(host, node => node instanceof view.HTMLInputElement, 8);
const reading = () => {
    connected();
    const values = [attr(target, "aria-valuetext"), attr(target, "data-value"),
        attr(host, "aria-valuetext"), attr(host, "data-value")];
    if (target instanceof view.HTMLInputElement) values.push(target.value);
    if (target instanceof view.HTMLButtonElement || role(target) === "combobox" && target.tagName !== "INPUT") {
        values.push(target.textContent);
    }
    if (host !== target) {
        values.push(...inputControls().map(node => node.value));
        // A single control's rendered label can differ from its backing code.
        values.push(host.textContent);
    }
    return values.filter(value => value && value.length <= 4096).map(norm);
};
const fieldFingerprint = () => JSON.stringify([
    target instanceof view.HTMLInputElement ? target.value : norm(target.textContent),
    attr(target, "aria-valuetext"), attr(target, "data-value"),
    ...inputControls().map(node => node.value),
]);
let initialField = null;
const beginEffect = () => {
    if (initialField === null) initialField = fieldFingerprint();
    effectStarted = true;
};
const nativeSet = (node, value) => {
    connected();
    if (!node.isConnected || node.ownerDocument !== doc) fail("stale_element");
    if (disabled(node)) fail("control_disabled");
    if (node instanceof view.HTMLInputElement && (node.readOnly || attr(node, "aria-readonly") === "true")) fail("widget_not_supported");
    const prototype = node instanceof view.HTMLSelectElement
        ? view.HTMLSelectElement.prototype : view.HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
    if (!setter) fail("widget_not_supported");
    beginEffect();
    setter.call(node, value);
    node.dispatchEvent(new view.Event("input", { bubbles: true }));
    node.dispatchEvent(new view.Event("change", { bubbles: true }));
};
const activate = node => {
    connected();
    if (!node.isConnected) fail("stale_element");
    if (disabled(node)) fail("option_disabled");
    beginEffect();
    // Framework selects often open on mousedown. No coordinate lookup or pointer dispatch.
    node.dispatchEvent(new view.MouseEvent("mousedown", { bubbles: true, cancelable: true, button: 0 }));
    connected();
    if (!node.isConnected) return; // mousedown may itself commit and remove an option.
    node.dispatchEvent(new view.MouseEvent("mouseup", { bubbles: true, cancelable: true, button: 0 }));
    connected();
    if (!node.isConnected) return;
    const formDefault = (node instanceof view.HTMLButtonElement || node instanceof view.HTMLInputElement)
        && ["submit", "reset", "image"].includes(node.type);
    if (formDefault) {
        // Cancel before dispatch: an earlier capture listener may stop later listeners.
        // Component handlers still run, but native form submission/reset cannot run.
        const click = new view.MouseEvent("click", { bubbles: true, cancelable: true, button: 0, view });
        click.preventDefault();
        node.dispatchEvent(click);
    } else node.click();
};
const selected = node => attr(node, "aria-selected") === "true" || attr(node, "aria-checked") === "true"
    || node.selected === true || node.checked === true;
const optionClass = node => /(?:^|[-_\s])(?:option|menuitem)(?:$|[-_\s])/.test(attr(node, "class").toLowerCase());
const isPopup = node => ["listbox", "menu", "tree", "dialog", "grid"].includes(role(node))
    || node.tagName === "UL" || node.tagName === "DIALOG" || node.hasAttribute("popover")
    || /(?:^|[-_\s])(?:dropdown|popup|popover)(?:$|[-_\s])/.test(attr(node, "class").toLowerCase());
const linkedNodes = node => {
    const ids = [attr(node, "aria-controls"), attr(node, "aria-owns")].join(" ").split(/\s+/).filter(Boolean);
    if (ids.length > 16) fail("widget_bounds_exceeded");
    return ids.map(id => doc.getElementById(id)).filter(Boolean);
};
const linkedRoots = (node, allowMirror = true) => linkedNodes(node).flatMap(linked => {
    if (visible(linked)) return [linked];
    // Only an explicitly zero-size ARIA mirror can borrow its nearby visible popup.
    // A hidden/inert child list is a closed scope, never evidence for its parent.
    if (!allowMirror || !["listbox", "menu", "tree"].includes(role(linked))
        || linked.closest('[hidden],[aria-hidden="true"],[inert]')) return [];
    const rects = Array.from(linked.getClientRects());
    const style = view.getComputedStyle(linked);
    if (!rects.length || rects.some(rect => rect.width !== 0 || rect.height !== 0)
        || style.display === "none" || ["hidden", "collapse"].includes(style.visibility)) return [];
    for (let parent = linked.parentElement, depth = 0; parent && parent !== doc.body && depth < 3; parent = parent.parentElement, depth++) {
        const parentStyle = view.getComputedStyle(parent);
        if (parentStyle.display === "none" || ["hidden", "collapse"].includes(parentStyle.visibility)) return [];
        if (isPopup(parent) && visible(parent)) return [parent];
    }
    return [];
});
const outerRoots = roots => [...new Set(roots)].filter(node => !roots.some(other => other !== node && other.contains(node)));
let beforePopups = new Set();
const popupRoots = (extra = []) => {
    const childLinked = extra.flatMap(node => linkedRoots(node, false));
    if (childLinked.length) return outerRoots(childLinked);
    const linked = [...linkedRoots(target), ...linkedRoots(host)];
    if (["listbox", "tree", "menu"].includes(role(target)) && visible(target)) linked.push(target);
    if (linked.length) return outerRoots(linked);
    return outerRoots(walk(doc.documentElement, node => isPopup(node) && visible(node), 32)
        .filter(node => !beforePopups.has(node)));
};
const observe = async (probe, timeout = 650) => {
    const until = Math.min(deadline, performance.now() + timeout);
    for (;;) {
        connected();
        const result = probe();
        if (result) return result;
        if (performance.now() >= until) return null;
        // Attribute/tree mutations wake immediately; the short timer also catches property-only value updates.
        await new Promise(resolve => {
            let timer;
            const observer = new view.MutationObserver(() => { observer.disconnect(); view.clearTimeout(timer); resolve(); });
            observer.observe(doc.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
            timer = view.setTimeout(() => { observer.disconnect(); resolve(); }, Math.min(30, until - performance.now()));
        });
    }
};
const open = async () => {
    connected();
    if (initialField === null) initialField = fieldFingerprint();
    if (disabled(target) || disabled(host)) fail("control_disabled");
    if (["listbox", "tree", "menu"].includes(role(target))) return;
    if (attr(target, "aria-expanded") === "true" || attr(host, "aria-expanded") === "true") {
        // Already-open, unlinked popups still need a unique scoped surface.
        const current = outerRoots(walk(doc.documentElement, node => isPopup(node) && visible(node), 32));
        if (!linkedRoots(target).length && !linkedRoots(host).length) {
            const declared = [attr(target, "aria-controls"), attr(target, "aria-owns"), attr(host, "aria-controls"), attr(host, "aria-owns")].some(Boolean);
            if (declared) beforePopups = new Set(current);
            else if (current.length !== 1) fail("option_ambiguous");
        }
    } else {
        // A linked popup may appear asynchronously, or be a zero-size accessibility
        // mirror. Record existing surfaces before opening so fallback never selects
        // an unrelated already-visible list while waiting for the real popup.
        beforePopups = new Set(walk(doc.documentElement, node => isPopup(node) && visible(node), 32));
        opened = true;
        beginEffect();
        target.focus?.({ preventScroll: true });
        activate(target);
        await tick();
    }
};
const optionsIn = (roots, dates = false) => {
    const result = [];
    for (const root of roots) {
        const candidates = walk(root, node => {
            if (!visible(node)) return false;
            const semantic = ["option", "menuitem", "menuitemradio", "treeitem", "radio"].includes(role(node));
            if (semantic) return true;
            if (optionClass(node)) {
                for (let ancestor = node.parentElement; ancestor && root.contains(ancestor); ancestor = ancestor.parentElement) {
                    if (optionClass(ancestor) || ["option", "menuitem", "treeitem"].includes(role(ancestor))) return false;
                    if (ancestor === root) break;
                }
                return true;
            }
            if (dates && (node.tagName === "BUTTON" || role(node) === "gridcell" || node.tagName === "TD")) {
                return !node.querySelector('button,[role="gridcell"]');
            }
            return node.tagName === "LI" && !node.querySelector('[role="option"],[role="menuitem"],[role="treeitem"]');
        });
        for (const node of candidates) if (!result.includes(node)) result.push(node);
        if (result.length > MAX_OPTIONS) fail("widget_bounds_exceeded");
    }
    return result;
};
const unique = candidates => {
    if (candidates.length > 1) fail("option_ambiguous");
    if (candidates.length === 1 && disabled(candidates[0])) fail("option_disabled");
    return candidates[0] || null;
};
const cleanup = async () => {
    if (!opened || changed || !target.isConnected) return;
    const active = doc.activeElement || target;
    active.dispatchEvent(new view.KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    active.dispatchEvent(new view.KeyboardEvent("keyup", { key: "Escape", code: "Escape", bubbles: true }));
    await tick();
};
const rejected = async error => {
    await cleanup();
    if (initialField !== null) {
        if (!target.isConnected || target.ownerDocument !== doc) changed = true;
        else {
            try { if (initialField !== fieldFingerprint()) changed = true; }
            catch { changed = true; } // Lost observation is not proof that an effect did not occur.
        }
    }
    return { ok: false, kind: error?.widgetKind || "widget_not_supported", mutated: effectStarted || changed };
};
