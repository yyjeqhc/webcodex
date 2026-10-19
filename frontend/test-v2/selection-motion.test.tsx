import { useRef } from "react";
import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSelectionMotion } from "../src/ui/selectionMotion.js";
import { useFlowingSelection } from "../src/ui/useFlowingSelection.js";

type Rect = { x: number; y: number; width: number; height: number };
const rect = ({ x, y, width, height }: Rect): DOMRect => ({ x, y, left: x, top: y, width, height, right: x + width, bottom: y + height, toJSON() {} });
const cleanups: (() => void)[] = [];
afterEach(() => { cleanups.splice(0).forEach(cleanup => cleanup()); document.body.replaceChildren(); });

function fixture({ reduced = false, forced = false, fieldset = false } = {}) {
  const media = new Map<string, { matches: boolean; addEventListener: ReturnType<typeof vi.fn>; removeEventListener: ReturnType<typeof vi.fn> }>();
  vi.spyOn(window, "matchMedia").mockImplementation(query => {
    const result = { matches: query.includes("reduced") ? reduced : forced, addEventListener: vi.fn(), removeEventListener: vi.fn() };
    media.set(query, result);
    return result as unknown as MediaQueryList;
  });
  const group = document.createElement(fieldset ? "fieldset" : "div");
  group.style.position = "static";
  group.innerHTML = '<button id="first" aria-selected="true"><svg id="icon" viewBox="0 0 24 24"><path id="path" d="M0 0L1 1" stroke="currentColor"/></svg><span id="label">One</span></button><button id="second" aria-selected="false">Variable width</button><button id="third" aria-selected="false">Third</button>';
  document.body.append(group);
  const items = Array.from(group.querySelectorAll("button"));
  const boxes: Rect[] = [{ x: 4, y: 3, width: 64, height: 30 }, { x: 80, y: 3, width: 130, height: 30 }, { x: 4, y: 50, width: 84, height: 36 }];
  let visual: Rect | null = null;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function(this: HTMLElement) {
    if (this === group) return rect({ x: 200, y: 100, width: 230, height: 100 });
    if (this.hasAttribute("data-selection-motion-origin")) return rect({x:200,y:fieldset?120:100,width:0,height:0});
    const index = items.indexOf(this as HTMLButtonElement);
    if (index >= 0) return rect({ ...boxes[index], x: 200 + boxes[index].x, y: (fieldset?120:100) + boxes[index].y });
    if (this.hasAttribute("data-selection-motion-plate")) {
      const [x, y] = this.style.transform.match(/-?\d+(?:\.\d+)?/g)?.map(Number) ?? [0, 0];
      const current = visual ?? { x, y, width: Number.parseFloat(this.style.width), height: Number.parseFloat(this.style.height) };
      return rect({ ...current, x: current.x + 200, y: current.y + (fieldset?120:100) });
    }
    return rect({ x: 0, y: 0, width: 0, height: 0 });
  });
  const cancel = vi.fn();
  const animate = vi.fn(() => ({ cancel }) as unknown as Animation);
  Object.defineProperty(HTMLElement.prototype, "animate", { configurable: true, value: animate });
  const controller = createSelectionMotion(group);
  cleanups.push(controller.destroy);
  const select = (index: number) => { items.forEach((item, i) => item.setAttribute("aria-selected", String(i === index))); controller.update(); };
  return { group, items, boxes, animate, cancel, controller, select, media, setVisual: (value: Rect) => { visual = value; } };
}

describe("flowing selection motion", () => {
  it("keeps one plate, original controls and unique IDs while mirroring only inert visuals", () => {
    const { group, items, controller, animate } = fixture();
    const plate = group.querySelector("[data-selection-motion-plate]");
    expect(animate).not.toHaveBeenCalled();
    expect(group.style.position).toBe("relative");
    expect(group.querySelectorAll("button")).toHaveLength(3);
    const overlay = group.querySelector("[data-selection-motion-foreground]")!;
    expect(overlay.getAttribute("aria-hidden")).toBe("true");
    expect(overlay.hasAttribute("inert")).toBe(true);
    expect(overlay.querySelectorAll("button,input,[id],[role],[tabindex]")).toHaveLength(0);
    expect(overlay.querySelector("svg path")?.getAttribute("stroke")).toBe("currentColor");
    expect(group.querySelectorAll("#label")).toHaveLength(1);
    expect(overlay.textContent).toBe("OneVariable widthThird");
    items[1].click();
    controller.update({ animate: false });
    expect(group.querySelector("[data-selection-motion-plate]")).toBe(plate);
    controller.destroy();
    expect(group.querySelectorAll("[data-selection-motion-overlay]")).toHaveLength(0);
    expect(items[0].id).toBe("first");
    expect(group.style.position).toBe("static");
    expect(group.hasAttribute("data-selection-motion")).toBe(false);
  });

  it("animates both axes and variable widths for 260ms with bounded stretch, then retargets from the current visual box", () => {
    const { animate, select, setVisual, cancel } = fixture();
    select(1);
    expect(animate).toHaveBeenCalledTimes(2);
    const [frames, timing] = animate.mock.calls[0] as unknown as [Keyframe[], KeyframeAnimationOptions];
    expect(timing.duration).toBe(260);
    expect(frames[0]).toMatchObject({ transform: "translate(4px, 3px)", width: "64px" });
    expect(Number.parseFloat(frames[1].width as string)).toBeCloseTo(130 * 1.03);
    expect(frames[2]).toMatchObject({ transform: "translate(80px, 3px)", width: "130px" });
    setVisual({ x: 40, y: 3, width: 100, height: 30 });
    select(2);
    expect(cancel).toHaveBeenCalledTimes(2);
    const retarget = animate.mock.calls[2][0] as unknown as Keyframe[];
    expect(retarget[0]).toMatchObject({ transform: "translate(40px, 3px)", width: "100px" });
    expect(retarget[2]).toMatchObject({ transform: "translate(4px, 50px)", width: "84px", height: "36px" });
    expect(Number.parseFloat(retarget[1].width as string)).toBeLessThanOrEqual(84 * 1.03);
    expect(Number.parseFloat(retarget[1].height as string)).toBeLessThanOrEqual(36 * 1.03);
  });

  it.each([{ reduced: true }, { forced: true }])("snaps when the platform requests %o", preferences => {
    const { select, animate } = fixture(preferences);
    select(2);
    expect(animate).not.toHaveBeenCalled();
  });

  it("snaps layout updates, preserves a sticky group, and releases media listeners", () => {
    const { group, select, controller, animate, cancel, media } = fixture();
    select(1);
    controller.update({ animate: false });
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(animate).toHaveBeenCalledTimes(2);
    controller.destroy();
    group.style.position = "sticky";
    const sticky = createSelectionMotion(group);
    expect(group.style.position).toBe("sticky");
    sticky.destroy();
    expect(group.style.position).toBe("sticky");
    for (const query of media.values()) expect(query.removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
  });

  it("establishes a containing block across sticky-to-static responsive changes", () => {
    const { group, controller } = fixture();
    controller.destroy();group.style.position="";
    const sheet=document.createElement("style");sheet.textContent=".responsive-tabs { position:sticky; }";document.head.append(sheet);group.className="responsive-tabs";
    const responsive=createSelectionMotion(group);
    expect(group.style.position).toBe("");
    sheet.textContent=".responsive-tabs { position:static; }";
    responsive.update({animate:false});expect(group.style.position).toBe("relative");
    sheet.textContent=".responsive-tabs { position:sticky; }";
    responsive.update({animate:false});expect(group.style.position).toBe("");
    expect(getComputedStyle(group).position).toBe("sticky");
    responsive.destroy();sheet.remove();
  });

  it("measures native fieldset children from the origin below the legend", () => {
    const {group,select,animate}=fixture({fieldset:true});
    expect(group.querySelector<HTMLElement>("[data-selection-motion-plate]")!.style.transform).toBe("translate(4px, 3px)");
    select(1);
    const frames=animate.mock.calls[0][0] as Keyframe[];
    expect(frames[2].transform).toBe("translate(80px, 3px)");
  });

  it("does not cancel motion for React radio rewrites or an unused library indicator", async () => {
    const {group,items,select,cancel,controller}=fixture();
    const input=document.createElement("input");input.type="radio";items[0].append(input);
    const libraryIndicator=document.createElement("span");group.append(libraryIndicator);
    controller.update({animate:false});select(1);
    await act(async()=>{input.name="";input.type="radio";input.name="mode";libraryIndicator.style.transform="translateX(80px)";libraryIndicator.setAttribute("data-initialized","true");});
    expect(cancel).not.toHaveBeenCalled();
  });

  it("preserves descendant badge geometry after removing control ancestor semantics", () => {
    const {group,items,controller}=fixture();group.className="badge-group";
    const sheet=document.createElement("style");sheet.textContent=".badge-group button .counter { min-width:20px; padding:2px 6px; display:inline-flex; }";document.head.append(sheet);
    const badge=document.createElement("span");badge.className="counter";badge.textContent="2";items[0].append(badge);controller.update({animate:false});
    const mirror=group.querySelector<HTMLElement>("[data-selection-motion-foreground] .counter")!;
    expect(mirror.style.minWidth).toBe("20px");expect(mirror.style.padding).toBe("2px 6px");expect(mirror.style.display).toBe("inline-flex");
    sheet.remove();
  });

  it("responds to selection mutations and snaps translated labels", async () => {
    const { items, group, animate } = fixture();
    await act(async () => {
      items[0].setAttribute("aria-selected", "false");
      items[1].setAttribute("aria-selected", "true");
    });
    expect(animate).toHaveBeenCalledTimes(2);
    await act(async () => { items[1].textContent = "Translated label"; });
    expect(group.querySelector("[data-selection-motion-foreground]")!.textContent).toContain("Translated label");
    expect(animate).toHaveBeenCalledTimes(2);
  });

  it("animates a keyboard tab selection while its roving tabindex moves", async () => {
    const { items, animate } = fixture();
    await act(async () => {
      items[0].setAttribute("aria-selected", "false");
      items[0].setAttribute("tabindex", "-1");
      items[1].setAttribute("aria-selected", "true");
      items[1].setAttribute("tabindex", "0");
      items[1].focus();
    });
    expect(document.activeElement).toBe(items[1]);
    expect(animate).toHaveBeenCalledTimes(2);
  });

  it("keeps inline Host height changes from cancelling motion, but snaps a reflow", async () => {
    const { select, boxes, animate, cancel } = fixture();
    select(1);
    await act(async () => {
      window.dispatchEvent(new Event("resize"));
      await new Promise(requestAnimationFrame);
    });
    expect(cancel).not.toHaveBeenCalled();
    boxes[1].width = 95;
    await act(async () => {
      window.dispatchEvent(new Event("resize"));
      await new Promise(requestAnimationFrame);
    });
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(animate).toHaveBeenCalledTimes(2);
  });

  it("supports label selectors without duplicating radio controls", () => {
    const { group, controller } = fixture();
    controller.destroy();
    group.innerHTML = '<label data-active><input name="mode" type="radio" checked/><span>Radio</span></label>';
    const radios = createSelectionMotion(group, { itemSelector: "label", selectedSelector: "[data-active]" });
    expect(group.querySelectorAll("input")).toHaveLength(1);
    expect(group.querySelector("[data-selection-motion-foreground]")!.textContent).toBe("Radio");
    radios.destroy();
  });

  it("attaches when an asynchronous view supplies a previously empty ref", () => {
    function AsyncView({ ready }: { ready: boolean }) {
      const ref = useRef<HTMLDivElement>(null);
      useFlowingSelection(ref, "same");
      return ready ? <div ref={ref}><button aria-selected="true">Ready</button></div> : null;
    }
    const result = render(<AsyncView ready={false} />);
    result.rerender(<AsyncView ready />);
    expect(result.container.querySelector("[data-selection-motion-plate]")).toBeTruthy();
    result.unmount();
  });

  it("gracefully ignores non-browser protocol harnesses", () => {
    const controller = createSelectionMotion({} as HTMLElement);
    expect(() => { controller.update(); controller.destroy(); }).not.toThrow();
  });
});
