export type SelectionMotionOptions = { itemSelector?: string; selectedSelector?: string };
type Box = { x: number; y: number; width: number; height: number };
type Controller = { update(options?: { animate?: boolean }): void; destroy(): void };

const duration = 260;
const svgNamespace = "http://www.w3.org/2000/svg";

// Copy only presentation into inert spans/SVG. Real controls remain the sole
// owners of focus, events, accessible names and IDs.
function copyVisual(node: Node, doc: Document): Node | null {
  if (node.nodeType === 3) return doc.createTextNode(node.textContent ?? "");
  if (node.nodeType !== 1) return null;
  const source = node as Element;
  if (/^(input|select|textarea|script|style|iframe|object)$/i.test(source.localName)) return null;
  const svg = source.namespaceURI === svgNamespace;
  const target = svg ? doc.createElementNS(svgNamespace, source.localName) : doc.createElement("span");
  for (const { name, value } of Array.from(source.attributes)) {
    if (name === "class" || name === "style" || (svg && !/^(id|tabindex|role|aria-.+|on.+|href|xlink:href)$/i.test(name))) {
      target.setAttribute(name, value);
    }
  }
  if ((!svg || source.localName === "svg") && doc.defaultView) {
    // Mirrors use inert spans rather than duplicate controls, so selectors such
    // as `button > span` no longer apply. Preserve descendant label/badge layout.
    const computed = doc.defaultView.getComputedStyle(source);
    const styles = (target as HTMLElement).style;
    for (const property of ["display", "box-sizing", "font", "line-height", "letter-spacing", "text-transform", "text-align", "white-space", "vertical-align", "width", "height", "min-width", "max-width", "min-height", "max-height", "padding", "margin", "border-width", "border-style", "border-radius", "align-items", "align-self", "justify-content", "flex-direction", "flex-wrap", "flex-grow", "flex-shrink", "flex-basis", "gap", "order", "overflow", "overflow-wrap", "word-break"]) {
      styles.setProperty(property, computed.getPropertyValue(property));
    }
    styles.borderColor = "transparent";
  }
  for (const child of Array.from(source.childNodes)) {
    const visual = copyVisual(child, doc);
    if (visual) target.appendChild(visual);
  }
  return target;
}

function sameBox(a: Box, b: Box): boolean {
  return Math.abs(a.x - b.x) < .25 && Math.abs(a.y - b.y) < .25
    && Math.abs(a.width - b.width) < .25 && Math.abs(a.height - b.height) < .25;
}

/** A single persistent plate and a clipped, stationary presentation of labels. */
export function createSelectionMotion(group: HTMLElement, options: SelectionMotionOptions = {}): Controller {
  const doc = group?.ownerDocument;
  const view = doc?.defaultView;
  // Embedded protocol tests and non-DOM hosts do not provide a browser renderer.
  if (!view || !group.querySelectorAll || !group.matches || !view.getComputedStyle) {
    return { update() {}, destroy() {} };
  }
  const itemSelector = options.itemSelector ?? '[role="tab"], button';
  const selectedSelector = options.selectedSelector ?? '[aria-selected="true"], [aria-pressed="true"], .active';
  const originalPosition = group.style.position;
  let ownsPosition = view.getComputedStyle(group).position === "static";
  if (ownsPosition) group.style.position = "relative";
  const plate = doc.createElement("span");
  const foreground = doc.createElement("span");
  const origin = doc.createElement("span");
  origin.dataset.selectionMotionOrigin = "";
  plate.dataset.selectionMotionPlate = "";
  foreground.dataset.selectionMotionForeground = "";
  for (const element of [plate, foreground, origin]) {
    element.dataset.selectionMotionOverlay = "";
    element.setAttribute("aria-hidden", "true");
    element.setAttribute("inert", "");
  }
  group.dataset.selectionMotion = "";
  group.append(plate, foreground, origin);
  let items: HTMLElement[] = [];
  let target: Box | null = null;
  let radius = "0px";
  let animations: Animation[] = [];
  let destroyed = false;
  let rebuilding = false;
  let pendingFrame = 0;
  const reduced = view.matchMedia?.("(prefers-reduced-motion: reduce)");
  const forced = view.matchMedia?.("(forced-colors: active)");
  const observer = new view.MutationObserver(records => {
    const relevant = records.filter(record => {
      const element = record.target.nodeType === 1 ? record.target as Element : record.target.parentElement;
      // React temporarily rewrites controlled radio names/types. Mantine also
      // moves its disabled original indicator. Neither changes our label layout.
      if (element?.localName === "input" && ["name", "type", "value"].includes(record.attributeName ?? "")) return false;
      if (element !== group && !element?.closest(itemSelector)) return false;
      return !element?.closest("[data-selection-motion-overlay]")
        && !(element === group && record.attributeName === "data-selection-motion")
        && record.attributeName !== "data-selection-motion-item"
        && record.attributeName !== "data-selection-motion-selected";
    });
    if (!relevant.length) return;
    // Roving tabindex follows selection without changing label geometry.
    const selectionOnly = relevant.every(record => record.type === "attributes"
      && ["aria-selected", "aria-pressed", "tabindex", "class", "data-active", "checked", "name", "value"].includes(record.attributeName ?? ""));
    update({ animate: selectionOnly });
  });
  const observe = () => observer.observe(group, { subtree: true, childList: true, characterData: true, attributes: true });
  const resize = typeof view.ResizeObserver === "function" ? new view.ResizeObserver(() => scheduleSnap()) : null;
  resize?.observe(group);
  const theme = new view.MutationObserver(() => scheduleSnap());

  function localBox(element: HTMLElement): Box {
    const bounds = element.getBoundingClientRect();
    // Native fieldsets place absolute children below their legend. Measure the
    // actual containing-block origin rather than assuming the group's border.
    const zero = origin.getBoundingClientRect();
    return { x: bounds.left - zero.left, y: bounds.top - zero.top, width: bounds.width, height: bounds.height };
  }

  function frame(box: Box): Keyframe {
    return { transform: `translate(${box.x}px, ${box.y}px)`, width: `${box.width}px`, height: `${box.height}px` };
  }

  function clip(box: Box): string {
    return `inset(${box.y}px calc(100% - ${box.x + box.width}px) calc(100% - ${box.y + box.height}px) ${box.x}px round ${radius})`;
  }

  function cancelAnimations() {
    for (const animation of animations) animation.cancel();
    animations = [];
  }

  function rebuildVisuals(nextItems: HTMLElement[]) {
    for (const item of items) if (!nextItems.includes(item)) {
      item.removeAttribute("data-selection-motion-item");
      item.removeAttribute("data-selection-motion-selected");
      resize?.unobserve(item);
    }
    for (const item of nextItems) if (!items.includes(item)) resize?.observe(item);
    items = nextItems;
    foreground.replaceChildren();
    for (const item of items) {
      item.dataset.selectionMotionItem = "";
      item.toggleAttribute("data-selection-motion-selected", item.matches(selectedSelector));
      const box = localBox(item);
      const computed = view!.getComputedStyle(item);
      const mirror = doc.createElement("span");
      mirror.dataset.selectionMotionLabel = "";
      Object.assign(mirror.style, {
        left: `${box.x}px`, top: `${box.y}px`, width: `${box.width}px`, height: `${box.height}px`,
        font: computed.font, letterSpacing: computed.letterSpacing, textTransform: computed.textTransform,
        lineHeight: computed.lineHeight, textAlign: computed.textAlign, whiteSpace: computed.whiteSpace,
        justifyContent: /flex|grid/.test(computed.display) ? computed.justifyContent
          : /left|start/.test(computed.textAlign) ? "flex-start" : /right|end/.test(computed.textAlign) ? "flex-end" : "center",
        alignItems: /flex|grid/.test(computed.display) ? computed.alignItems : "center",
        flexDirection: computed.flexDirection, gap: computed.gap, padding: computed.padding,
        borderWidth: computed.borderWidth, borderStyle: "solid", borderColor: "transparent",
      });
      for (const child of Array.from(item.childNodes)) {
        const visual = copyVisual(child, doc);
        if (visual) mirror.appendChild(visual);
      }
      foreground.appendChild(mirror);
    }
  }

  function update({ animate = true }: { animate?: boolean } = {}) {
    if (destroyed || rebuilding) return;
    rebuilding = true;
    observer.disconnect();
    // Responsive Desktop category groups switch between sticky and static.
    // Reveal the stylesheet position before establishing a containing block;
    // never pin a formerly-static group to relative across the sticky breakpoint.
    if (ownsPosition) { group.style.position = originalPosition; ownsPosition = false; }
    if (view!.getComputedStyle(group).position === "static") { group.style.position = "relative"; ownsPosition = true; }
    const nextItems = Array.from(group.querySelectorAll<HTMLElement>(itemSelector))
      .filter(item => !item.closest("[data-selection-motion-overlay]"));
    const selected = nextItems.find(item => item.matches(selectedSelector));
    const next = selected ? localBox(selected) : null;
    const previous = target && !plate.hidden ? localBox(plate) : null;
    const unchanged = !!next && !!target && sameBox(next, target);
    // Preserve an in-flight animation when a duplicate React/observer update
    // sees the same selection. A layout/theme update explicitly snaps instead.
    if (unchanged && animate) {
      rebuilding = false;
      observe();
      return;
    }
    cancelAnimations();
    rebuildVisuals(nextItems);
    target = next;
    plate.hidden = foreground.hidden = !next || next.width <= 0 || next.height <= 0;
    if (next && selected) {
      Object.assign(plate.style, frame(next));
      radius = view!.getComputedStyle(selected).borderRadius || "0px";
      // Native radio labels often put the visible surface on their child span.
      const surface = selected.lastElementChild;
      if (surface instanceof view!.HTMLElement && (!radius || /^0(?:px)?$/.test(radius))) {
        const surfaceBox = localBox(surface);
        if (sameBox(surfaceBox, next)) radius = view!.getComputedStyle(surface).borderRadius || radius;
      }
      plate.style.borderRadius = radius;
      foreground.style.clipPath = clip(next);
      const canAnimate = animate && previous && !sameBox(previous, next) && !reduced?.matches
        && !forced?.matches && typeof plate.animate === "function";
      if (canAnimate && previous) {
        const dx = next.x - previous.x;
        const dy = next.y - previous.y;
        const distance = Math.hypot(dx, dy);
        const stretchX = distance ? .03 * Math.abs(dx) / distance : 0;
        const stretchY = distance ? .03 * Math.abs(dy) / distance : 0;
        const settle = { x: next.x - next.width * stretchX / 2, y: next.y - next.height * stretchY / 2,
          width: next.width * (1 + stretchX), height: next.height * (1 + stretchY) };
        const timing: KeyframeAnimationOptions = { duration, easing: "cubic-bezier(.2,.75,.25,1)", fill: "none" };
        animations = [
          plate.animate([frame(previous), { ...frame(settle), offset: .72 }, frame(next)], timing),
          foreground.animate([{ clipPath: clip(previous) }, { clipPath: clip(settle), offset: .72 }, { clipPath: clip(next) }], timing),
        ];
      }
    }
    rebuilding = false;
    observe();
  }

  function scheduleSnap() {
    if (destroyed || pendingFrame) return;
    pendingFrame = view!.requestAnimationFrame(() => {
      pendingFrame = 0;
      update({ animate: false });
    });
  }

  function viewportResize() {
    // Inline Hosts resize iframe height when a panel opens. Preserve motion if
    // the selected control has not reflowed; real responsive changes still snap.
    const selected = items.find(item => item.matches(selectedSelector));
    if (!selected || !target || !sameBox(localBox(selected), target)) scheduleSnap();
  }

  theme.observe(doc.documentElement, { attributes: true, attributeFilter: ["class", "style", "lang", "data-mantine-color-scheme", "data-theme", "data-resolved-theme", "data-display-mode"] });
  if (doc.body !== group) theme.observe(doc.body, { attributes: true, attributeFilter: ["class", "style", "lang", "data-theme"] });
  reduced?.addEventListener("change", scheduleSnap);
  forced?.addEventListener("change", scheduleSnap);
  view.addEventListener("resize", viewportResize);
  doc.fonts?.addEventListener("loadingdone", scheduleSnap);
  update({ animate: false });
  return {
    update,
    destroy() {
      if (destroyed) return;
      destroyed = true;
      observer.disconnect();
      resize?.disconnect();
      theme.disconnect();
      cancelAnimations();
      view.cancelAnimationFrame(pendingFrame);
      reduced?.removeEventListener("change", scheduleSnap);
      forced?.removeEventListener("change", scheduleSnap);
      view.removeEventListener("resize", viewportResize);
      doc.fonts?.removeEventListener("loadingdone", scheduleSnap);
      for (const item of items) {
        item.removeAttribute("data-selection-motion-item");
        item.removeAttribute("data-selection-motion-selected");
      }
      plate.remove();
      foreground.remove();
      origin.remove();
      group.removeAttribute("data-selection-motion");
      if (ownsPosition) group.style.position = originalPosition;
    },
  };
}
