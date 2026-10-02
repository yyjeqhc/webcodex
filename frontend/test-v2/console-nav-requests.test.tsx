import { act, render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { App } from "../src/runtime-v2/App.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import { RUNTIME_CREDENTIAL_SESSION_KEY } from "../src/runtime_storage.js";
import { runtimeOverview } from "./fixtures.js";

it.each(["work", "projects"])("%s has one Project inventory owner while navigation polls only aggregates", async view => {
  vi.useFakeTimers();
  window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY,"test-token");
  window.localStorage.setItem("webcodex.runtime.v2.view.v1",view);
  const fixture = runtimeOverview();
  const requests: Array<{path:string;body:Record<string,unknown>}> = [];
  let visibility: DocumentVisibilityState = "visible";
  const spy = vi.spyOn(document,"visibilityState","get").mockImplementation(()=>visibility);
  const json = (value: unknown) => new Response(JSON.stringify(value),{headers:{"Content-Type":"application/json"}});
  vi.stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
    const path=String(input).split("/").at(-1)!; const body=JSON.parse(String(init?.body || "{}"));
    requests.push({path,body});
    if(path==="overview") return json({...fixture,detail_level:"primary",projects_included:false,visible_project_families:1,projects:[],recent_sessions:{sessions:[],returned:0,candidate_count:0,truncated:false,scan_truncated:true}});
    if(path==="projects") return json({projects:fixture.projects,total:fixture.projects.length,truncated:false});
    if(path==="windows") return json({windows:[],total:0,returned:0,truncated:false,visibility:{scope:"principal"}});
    if(path==="workflow-sessions") return json({sessions:[],total:0,returned:0,truncated:false});
    throw new Error(`Unexpected request ${path}`);
  }));
  let unmount: (()=>void)|undefined;
  try {
    ({unmount}=render(<UiProvider><App /></UiProvider>));
    await act(async()=>{});
    // Flush React effects between timer ticks, as separate browser turns do.
    // A single 25-second act batches revision updates into one artificial turn.
    for (let second=0; second<25; second++) {
      await act(async()=>{await vi.advanceTimersByTimeAsync(1_000);});
    }
    const nav=requests.filter(r=>r.path==="overview");
    expect(nav).toHaveLength(3); expect(nav.every(r=>r.body.include_projects===false && r.body.include_sessions===false)).toBe(true);
    expect(requests.filter(r=>r.path==="projects")).toHaveLength(1);
    const inventory=requests.filter(r=>r.path==="windows"&&r.body.projection==="inventory");
    const live=requests.filter(r=>r.path==="windows"&&r.body.projection==="liveness");
    expect(inventory).toHaveLength(1); expect(inventory[0].body.limit).toBe(50);
    expect(live).toHaveLength(view==="work"?8:5);
    if(view==="projects") expect([...inventory,...live].every(r=>Array.isArray(r.body.projects)&&r.body.projects.includes(fixture.projects[0].id))).toBe(true);
    const before=requests.length;
    await act(async()=>{visibility="hidden";document.dispatchEvent(new Event("visibilitychange"));});
    await act(async()=>{await vi.advanceTimersByTimeAsync(60_000);});
    expect(requests).toHaveLength(before);
    console.info(`CONSOLE_REQUEST_COUNTS view=${view} elapsed_ms=25000 overview=3 full_project_inventories=1 window_inventories=1 liveness=${live.length} hidden_60000ms=0`);
  } finally {unmount?.();spy.mockRestore();vi.useRealTimers();vi.unstubAllGlobals();}
});
