import test from "node:test";
import assert from "node:assert/strict";
import { AdminMutationController, AdminMutationError } from "../dist/admin_mutation_controller.js";
import { AdminMutationDialogCoordinator } from "../dist/admin_mutation_view.js";

function deferred() { let resolve, reject; const promise = new Promise((a,b)=>{resolve=a;reject=b}); return {promise,resolve,reject}; }
function harness() {
  const calls=[]; const keys=[]; const refreshes=[]; const errors=[]; let open=true; let cleared=0; let focused=0; let closed=0;
  const mutation=new AdminMutationController({
    request(kind,token,body,signal){ const task=deferred(); calls.push({kind,token,body,signal,task}); return task.promise; },
    keyFactory(){ const key=`key-${keys.length+1}`; keys.push(key); return key; },
    refresh(){ refreshes.push(true); return Promise.resolve(); }, outcome(){}, error(code){ errors.push(code); }, pending(){}, lock(){},
  });
  mutation.beginSession("token");
  const view=new AdminMutationDialogCoordinator(mutation, { close(){closed++;open=false;}, isOpen(){return open;}, clearSensitive(){cleared++;}, restoreFocus(){focused++;} });
  return {mutation,view,calls,keys,refreshes,errors,state:()=>({open,cleared,focused,closed}),setOpen(v){open=v}};
}

test("register failure retries through production coordinator with same key", async()=>{
  const h=harness(); const body={client_id:"oe",project_id:"p",name:"P",description:null,path:"/tmp/p",allow_patch:true}; h.view.open("register:1");
  const first=h.view.submit("register",body); h.calls[0].task.reject(new Error("network")); await first;
  const retry=h.view.submit("register",body); assert.equal(h.calls.length,2); assert.equal(h.calls[0].body.idempotency_key,"key-1"); assert.equal(h.calls[1].body.idempotency_key,"key-1"); h.calls[1].task.resolve({}); await retry;
});

test("changed create form cancels old context and creates a new key", async()=>{
  const h=harness(); const a={client_id:"oe",project_id:"p",name:"P",description:null,path:"/tmp/p",allow_patch:true,git_init:false,template:null,adopt_existing_empty:false}; h.view.open("create:1");
  const first=h.view.submit("create",a); h.calls[0].task.reject(new Error("network")); await first;
  const b={...a,name:"Changed"}; const second=h.view.submit("create",b); assert.equal(h.calls[1].body.idempotency_key,"key-2"); h.calls[1].task.resolve({}); await second;
});

test("escape cancels idle context, clears sensitive fields, and restores focus",()=>{
  const h=harness(); const body={project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}; const context=h.mutation.start("disable","agent:oe:p",body); h.view.open("agent:oe:p",context,body); let prevented=0; h.view.handleCancel({preventDefault(){prevented++;}}); assert.equal(prevented,1); assert.equal(h.mutation.has("agent:oe:p"),false); assert.deepEqual(h.state(),{open:false,cleared:1,focused:1,closed:1});
});

test("escape while pending keeps dialog and mutation open",async()=>{
  const h=harness(); const body={project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}; const context=h.mutation.start("disable","agent:oe:p",body); h.view.open("agent:oe:p",context,body); const run=h.mutation.submit(context); h.view.handleCancel({preventDefault(){}}); assert.equal(h.state().open,true); assert.equal(h.mutation.has("agent:oe:p"),true); h.calls[0].task.resolve({}); await run;
});

test("close fallback is idempotent and removes zombie context",()=>{
  const h=harness(); const body={project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}; const context=h.mutation.start("enable","agent:oe:p",body); h.view.open("agent:oe:p",context,body); h.setOpen(false); h.view.handleClose(); h.view.handleClose(); assert.equal(h.mutation.has("agent:oe:p"),false); assert.equal(h.view.currentTarget(),"");
});

test("active jobs conflict keeps context while revision conflict invalidates it",async()=>{
  const h=harness(); const body={project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}; let context=h.mutation.start("unregister","agent:oe:p",body); h.view.open("agent:oe:p",context,body); let run=h.mutation.submit(context); h.calls[0].task.reject(new AdminMutationError(409,"active_jobs_conflict")); await run; assert.equal(h.mutation.has("agent:oe:p"),true); run=h.view.submit("unregister",body); assert.equal(h.calls[1].body.idempotency_key,"key-1"); h.calls[1].task.reject(new AdminMutationError(409,"revision_conflict")); await run; assert.equal(h.mutation.has("agent:oe:p"),false); assert.equal(h.refreshes.length,2);
});


test("dashboard unauthorized orchestration aborts mutation and closes dialog", async()=>{
  const h=harness(); const dashboard=[]; let locked=0;
  const refresh=new (await import("../dist/admin_controller.js")).AdminRefreshController({
    request(token,signal){const task=deferred();dashboard.push({token,signal,task});return task.promise;}, render(){},showAuthenticated(){},showLocked(){},setStatus(){},showError(){},clearError(){},
    onUnauthorized(){ h.view.closeForSessionEnd(); h.mutation.lock(); refresh.lock(); locked++; },
  });
  const body={project:"agent:oe:p",expected_revision:"sha256:x",confirm:true}; const context=h.mutation.start("disable","agent:oe:p",body); h.view.open("agent:oe:p",context,body); const mutationRun=h.mutation.submit(context);
  const dashboardRun=refresh.beginSession("token"); dashboard[0].task.reject(new (await import("../dist/admin_controller.js")).AdminHttpError(401,"unauthorized")); await dashboardRun;
  assert.equal(locked,1); assert.equal(h.calls[0].signal.aborted,true); assert.equal(h.mutation.has("agent:oe:p"),false); assert.equal(h.state().open,false);
  h.calls[0].task.resolve({}); await mutationRun;
});
