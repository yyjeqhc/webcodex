import { test } from "vitest";
import assert from "node:assert/strict";
import { AdminMutationController, AdminMutationError } from "../src/admin_mutation_controller.js";

function deferred() { let resolve, reject; const promise = new Promise((a,b)=>{resolve=a;reject=b}); return {promise,resolve,reject}; }
function harness() {
  const calls=[]; const keys=[]; const pending=[]; const errors=[]; const outcomes=[]; const refreshes=[];
  const controller=new AdminMutationController({
    request(kind,token,body,signal){ const d=deferred(); calls.push({kind,token,body,signal,d}); return d.promise; },
    keyFactory(){ const key=`key-${keys.length+1}`; keys.push(key); return key; },
    refresh(){ refreshes.push(true); return Promise.resolve(); }, outcome:m=>outcomes.push(m), error:c=>errors.push(c), pending:(t,v)=>pending.push([t,v]), lock:()=>{}
  });
  return {controller,calls,keys,pending,errors,outcomes,refreshes};
}

test("same target is single-flight and duplicate click reuses one key", async()=>{ const h=harness(); h.controller.beginSession("A"); const c=h.controller.start("disable","agent:oe:p",{project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}); const duplicate=h.controller.start("disable","agent:oe:p",{}); assert.equal(duplicate,null); const run=h.controller.submit(c); h.controller.submit(c); assert.equal(h.calls.length,1); assert.equal(h.keys.length,1); assert.equal(h.calls[0].body.idempotency_key,"key-1"); h.calls[0].d.resolve({}); await run; assert.equal(h.refreshes.length,1); });

test("token switch aborts and stale success cannot update", async()=>{ const h=harness(); h.controller.beginSession("A"); const c=h.controller.start("enable","agent:oe:p",{project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}); const run=h.controller.submit(c); h.controller.beginSession("B"); assert.equal(h.calls[0].signal.aborted,true); h.calls[0].d.resolve({}); await run; assert.equal(h.outcomes.length,0); assert.equal(h.refreshes.length,0); });

test("indeterminate and network errors preserve retry context and key", async()=>{ const h=harness(); h.controller.beginSession("A"); const c=h.controller.start("unregister","agent:oe:p",{project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}); const first=h.controller.submit(c); h.calls[0].d.reject(new AdminMutationError(503,"operation_indeterminate")); await first; assert.deepEqual(h.errors,["operation_indeterminate"]); const retry=h.controller.retry("agent:oe:p"); assert.equal(h.calls[1].body.idempotency_key,"key-1"); h.calls[1].d.reject(new Error("network")); await retry; assert.deepEqual(h.errors,["operation_indeterminate","network_error"]); });

test("revision conflict refreshes and requires a new operation key", async()=>{ const h=harness(); h.controller.beginSession("A"); const c=h.controller.start("disable","agent:oe:p",{project:"agent:oe:p",expected_revision:"sha256:old",confirm:true}); const run=h.controller.submit(c); h.calls[0].d.reject(new AdminMutationError(409,"revision_conflict")); await run; assert.equal(h.refreshes.length,1); const next=h.controller.start("disable","agent:oe:p",{project:"agent:oe:p",expected_revision:"sha256:new",confirm:true}); assert.ok(next); assert.equal(h.keys.length,2); });

test("runner unregistration converges without an idempotency key and reports its own label", async()=>{ const h=harness(); h.controller.beginSession("A"); const c=h.controller.start("unregister_runner","stale-runner",{client_id:"stale-runner",confirm:true}); assert.ok(c); const run=h.controller.submit(c); assert.equal(h.calls.length,1); assert.deepEqual(h.calls[0].body,{client_id:"stale-runner",confirm:true}); assert.equal("idempotency_key" in h.calls[0].body,false); h.calls[0].d.resolve({outcome:"unregistered"}); await run; assert.deepEqual(h.outcomes,["Unregister runner completed."]); assert.equal(h.refreshes.length,1); assert.equal(h.controller.has("stale-runner"),false); });
