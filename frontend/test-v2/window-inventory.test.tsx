import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useWindowInventory } from "../src/runtime-v2/state/useWindowInventory.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";

const row = (id: number, active = 0) => ({ client_window_key: id.toString(16).padStart(64,"0"), source:"openai-session", last_seen_at_ms:id+1, first_seen_at_ms:1, active_count:active, linked_session_count:3, recorder_gap_count:1 });

it("refreshes an explicitly opened prefix beyond one server page without dropping older rows", async () => {
  vi.useFakeTimers();
  try {
    const calls: Array<{offset:number;limit:number}> = [];
    const client = { post: vi.fn(async (_path:string, body:{offset:number;limit:number})=>{
      calls.push(body);
      const length=Math.min(body.limit,2100-body.offset);
      const next=body.offset+length;
      return {ok:true,status:200,data:{windows:Array.from({length},(_,i)=>row(body.offset+i)),total:2100,truncated:next<2100,...(next<2100?{next_offset:next}:{})}};
    }) } as unknown as RuntimeV2Client;
    const unauthorized=vi.fn();
    const {result,unmount}=renderHook(()=>useWindowInventory(client,true,unauthorized));
    await act(async()=>{});
    for(let page=0;page<40;page++) await act(async()=>result.current.loadMore());
    expect(result.current.windows).toHaveLength(2050);
    const before=calls.length;
    await act(async()=>result.current.refresh());
    expect(calls.slice(before)).toMatchObject([{offset:0,limit:2000},{offset:2000,limit:50}]);
    expect(result.current.windows).toHaveLength(2050);
    expect(result.current.windows.at(-1)?.client_window_key).toBe(row(2049).client_window_key);
    unmount();
  } finally {vi.useRealTimers();}
});

describe("bounded Window inventory request contract", () => {
  it.each([3_000, 5_000])("keeps historical rows stable across %ims liveness and scopes every page", async refreshMs => {
    vi.useFakeTimers();
    try {
      const calls: Array<Record<string, unknown>> = [];
      const client = { post: vi.fn(async (_path: string, body: Record<string, unknown>) => {
        calls.push(body);
        const live = body.projection === "liveness";
        const offset = Number(body.offset || 0);
        return { ok:true,status:200,data:{windows:live?[row(49,1)]:Array.from({length:50},(_,i)=>row(offset+i)),total:live?1:100,truncated:!live&&offset===0,...(!live&&offset===0?{next_offset:50}:{}),visibility:{scope:"global"}} };
      }) } as unknown as RuntimeV2Client;
      const unauthorized = vi.fn();
      const projects = ["agent:r:repo", "agent:r:worktree"];
      const {result,unmount}=renderHook(()=>useWindowInventory(client,true,unauthorized,{projects,refreshMs}));
      await act(async()=>{});
      expect(calls).toHaveLength(1); expect(calls[0]).toMatchObject({projection:"inventory",limit:50,offset:0,projects});
      const history=result.current.historicalWindows;
      await act(async()=>{await vi.advanceTimersByTimeAsync(refreshMs*3);});
      expect(calls.filter(c=>c.projection==="inventory")).toHaveLength(1);
      expect(calls.filter(c=>c.projection==="liveness")).toHaveLength(3);
      expect(calls.every(c=>JSON.stringify(c.projects)===JSON.stringify(projects))).toBe(true);
      expect(result.current.historicalWindows).toBe(history);
      expect(result.current.windows[49]).toMatchObject({active_count:1,first_seen_at_ms:1,linked_session_count:3});
      await act(async()=>result.current.loadMore());
      expect(calls.at(-1)).toMatchObject({projection:"inventory",offset:50,limit:50});
      expect(result.current.windows).toHaveLength(100); expect(result.current.truncated).toBe(false);
      console.info(`WINDOW_REQUEST_COUNTS cadence_ms=${refreshMs} inventory=1 liveness=3 initial_rows=50 next_page_rows=50`);
      unmount();
    } finally {vi.useRealTimers();}
  });
  it("does not query global history while Project selection is disabled; discards late prior-scope results", async()=>{
    let resolveOld!: (value: unknown)=>void;
    const old=new Promise(resolve=>{resolveOld=resolve;});
    const post=vi.fn(async(_path:string,body:{projects?:string[]})=>body.projects?.[0]==="agent:r:old"?old:{ok:true,status:200,data:{windows:[row(2)],total:1,truncated:false}});
    const client={post} as unknown as RuntimeV2Client; const unauthorized=vi.fn();
    const {result,rerender}=renderHook(({enabled,project})=>useWindowInventory(client,enabled,unauthorized,{projects:[project]}),{initialProps:{enabled:false,project:"agent:r:old"}});
    expect(post).not.toHaveBeenCalled();
    rerender({enabled:true,project:"agent:r:old"}); await act(async()=>{});
    rerender({enabled:true,project:"agent:r:new"}); await act(async()=>{});
    await act(async()=>resolveOld({ok:true,status:200,data:{windows:[row(1)],total:1}}));
    expect(result.current.windows.map(r=>r.client_window_key)).toEqual([row(2).client_window_key]);
  });
});
