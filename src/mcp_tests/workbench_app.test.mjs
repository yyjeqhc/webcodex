import test from "node:test";
import assert from "node:assert/strict";
import { app, flush, toolResult } from "./app_test_support.mjs";
import { project, session_id, baseState } from "./work_result_app_fixture.mjs";

const projectUri = "webcodex-resource://project/YWdlbnQ6ZGVtbw/cm9vdA";
const fileUri = "webcodex-resource://file/YWdlbnQ6ZGVtbw/cm9vdA/c3JjL2EucnM";
const secondProject = "agent:other:project";
const projectItem = { type:"resource_link", uri:projectUri, name:project, title:"Demo Project", _meta:{kind:"project",project} };
const fileItem = { type:"resource_link", uri:fileUri, name:"src/a.rs", description:"Rust source", _meta:{kind:"file",project,path:"src/a.rs"} };
const page = (items = [], next_offset = null) => ({items,offset:0,limit:50,next_offset});
const contentOnly = output => ({ content:[{type:"text",text:JSON.stringify({success:true,output})}] });
const refs = view => view.sent.filter(request=>request.method === "ui/update-model-context");
const readRequests = view => view.calls("read_webcodex_resource");
const search = (view, kind) => view.calls("search_webcodex_resources").filter(request=>request.params.arguments.kind===kind);
async function initialize(view, modalities = {}, context = undefined, extension = false) {
  await view.reply(view.sent[0], {protocolVersion:"2026-01-26",hostCapabilities:{updateModelContext:modalities,...(extension ? {experimental:{"openai/modelContext":{}}} : {})},...(context===undefined ? {} : {hostContext:{"openai/modelContext":context}})});
}
async function launch(modalities = {}) {
  const view = app("mcp_workbench_app.html");
  view.toolInput({});
  view.toolResult({projects:page([projectItem])});
  await initialize(view,modalities);
  return view;
}
async function selectProject(view, value = project) {
  view.nodes.projectSelect.value=value;
  view.nodes.projectSelect.onchange();
  await flush();
}
async function files(view, items = [fileItem]) {
  await selectProject(view);
  view.nodes.tabFiles.onclick();
  await flush();
  await view.reply(search(view,"file").at(-1),toolResult(page(items)));
}
const rowActions = view => view.nodes.rows.children[0].children[1].children;

for (const order of ["input-result-init","result-init-input","init-input-result"]) test(`empty launcher needs an explicit Project even for one result: ${order}`,async()=>{
  const view=app("mcp_workbench_app.html");
  const input=()=>view.toolInput({});
  const result=()=>view.toolResult({projects:page([projectItem])});
  for (const step of order.split("-")) { if(step==="input") input(); else if(step==="result") result(); else await initialize(view); }
  await flush();
  assert.equal(view.nodes.projectSelect.value,"");
  assert.equal(view.nodes.projectSelect.children.length,2);
  assert.equal(view.nodes.selectionState.textContent,"Choose a Project to begin.");
  assert.equal(view.calls("work_result_state").length,0);
  assert.equal(view.calls("list_sessions").length,0);
  assert.equal(readRequests(view).length,0);
  assert.equal(refs(view).length,0);
});

test("empty launch recovers missing structuredContent with bounded JSON text",async()=>{
  const view=app("mcp_workbench_app.html");
  view.toolInput({});await initialize(view);
  const request=search(view,"project")[0];
  await view.reply(request,contentOnly(page([projectItem])));
  assert.equal(view.nodes.projectSelect.children[1].textContent,"Demo Project");
  assert.equal(view.nodes.projectSelect.value,"");
  await selectProject(view);
  await view.reply(view.calls("work_result_state")[0],contentOnly({work_result:baseState}));
  assert.equal(view.nodes.overview.children.length,4);
});

test("standard initial text launcher fallback uses supplied project resources",async()=>{
  const view=app("mcp_workbench_app.html");
  view.notification("ui/notifications/tool-result",contentOnly({projects:page([projectItem])}));
  await initialize(view);
  assert.equal(view.nodes.projectSelect.children.length,2);
  assert.equal(search(view,"project").length,0);
});

test("explicit launch scope works in either notification order and reads only overview",async()=>{
  for (const resultFirst of [false,true]) {
    const view=app("mcp_workbench_app.html");
    if(resultFirst)view.toolResult({project,session_id});
    view.toolInput({project,session_id});
    if(!resultFirst)view.toolResult({project,session_id});
    await initialize(view);
    assert.equal(view.nodes.projectSelect.value,project);
    assert.equal(view.nodes.sessionSelect.value,session_id);
    assert.deepEqual({...view.calls("work_result_state")[0].params.arguments},{project,session_id});
    assert.equal(readRequests(view).length,0);
    assert.equal(search(view,"file").length,0);
  }
});

test("named launch maps canonical Project IDs to a visible option when discovery includes short refs",async()=>{
  const view=app("mcp_workbench_app.html");
  view.toolInput({project:"~p1"});
  view.toolResult({project,projects:page([{...projectItem,_meta:{...projectItem._meta,project_ref:"~p1"}}])});
  await initialize(view);
  assert.equal(view.nodes.projectSelect.value,project);
  assert.ok(view.nodes.projectSelect.children.some(option=>option.value===view.nodes.projectSelect.value));
  assert.equal(view.calls("work_result_state")[0].params.arguments.project,project);
});

test("Session list never chooses a candidate and artifacts require explicit Session",async()=>{
  const view=await launch();await selectProject(view);
  await view.reply(view.calls("list_sessions")[0],toolResult({sessions:[{session_id,title:"A Session"}],next_offset:null}));
  assert.equal(view.nodes.sessionSelect.value,"");
  view.nodes.tabArtifacts.onclick();await flush();
  assert.equal(search(view,"artifact").length,0);
  assert.match(view.nodes.selectionState.textContent,/Choose a Workflow Session/);
  view.nodes.sessionSelect.value=session_id;view.nodes.sessionSelect.onchange();await flush();
  assert.deepEqual({...search(view,"artifact")[0].params.arguments},{kind:"artifact",project,session_id,offset:0,limit:50});
});

test("Project switch immediately clears rows, preview and Session, fencing late replies",async()=>{
  const view=await launch();await files(view);
  rowActions(view)[0].onclick();await flush();
  const oldRead=readRequests(view)[0];
  const oldOverview=view.calls("work_result_state")[0];
  await selectProject(view,secondProject);
  assert.equal(view.nodes.rows.children.length,0);
  assert.equal(view.nodes.preview.hidden,true);
  assert.equal(view.nodes.previewContent.textContent,"");
  assert.equal(view.nodes.sessionSelect.value,"");
  await view.reply(oldRead,toolResult({uri:fileUri,kind:"file",data:"OLD FILE"}));
  await view.reply(oldOverview,toolResult({work_result:baseState}));
  assert.equal(view.nodes.previewContent.textContent,"");
  assert.equal(view.nodes.overview.children.length,0);
  assert.ok(view.sent.some(request=>request.method==="notifications/cancelled" && request.params.requestId===oldRead.id));
});

test("Session switch clears artifact rows and ignores old artifact search",async()=>{
  const view=await launch();await selectProject(view);view.nodes.tabArtifacts.onclick();
  view.nodes.sessionSelect.value=session_id;view.nodes.sessionSelect.onchange();await flush();
  const old=search(view,"artifact")[0];
  const newSession="wc_sess_"+"2".repeat(32);
  view.nodes.sessionSelect.value=newSession;view.nodes.sessionSelect.onchange();await flush();
  assert.equal(view.nodes.rows.children.length,0);
  await view.reply(old,toolResult(page([{...fileItem,_meta:{...fileItem._meta,kind:"artifact"}}])));
  assert.equal(view.nodes.rows.children.length,0);
  assert.equal(search(view,"artifact").at(-1).params.arguments.session_id,newSession);
});

test("area/search fences late replies, supports paging and never eagerly reads",async()=>{
  const view=await launch();await selectProject(view);view.nodes.tabFiles.onclick();await flush();
  const old=search(view,"file")[0];
  view.nodes.query.value="a";view.nodes.searchForm.onsubmit({preventDefault(){}});await flush();
  const current=search(view,"file")[1];
  await view.reply(old,toolResult(page([fileItem])));
  assert.equal(view.nodes.rows.children.length,0);
  await view.reply(current,toolResult({...page([fileItem],50),list_truncated:true}));
  assert.equal(view.nodes.more.hidden,false);
  assert.match(view.nodes.coverage.textContent,/Partial/);
  view.nodes.more.onclick();await flush();
  assert.equal(search(view,"file").at(-1).params.arguments.offset,50);
  assert.equal(search(view,"file").at(-1).params.arguments.query,"a");
  assert.equal(readRequests(view).length,0);
  view.nodes.tabGoals.onclick();await flush();
  assert.equal(search(view,"goal").length,1);
});

test("preview deduplicates only in-flight reads and observes latest bounded content on every later action",async()=>{
  const view=await launch();await files(view);
  rowActions(view)[0].onclick();await flush();
  rowActions(view)[0].onclick();await flush();
  assert.equal(readRequests(view).length,1);
  await view.reply(readRequests(view)[0],toolResult({uri:fileUri,kind:"file",version:"v1",data:"<script>"+"汉".repeat(30000),truncated:true}));
  assert.match(view.nodes.previewContent.textContent,/^<script>/);
  assert.ok(new TextEncoder().encode(view.nodes.previewContent.textContent).length<=24576);
  assert.match(view.nodes.previewContent.textContent,/truncated/);
  assert.match(view.nodes.previewMeta.textContent,/v1.*Partial/);
  rowActions(view)[0].onclick();await flush();
  assert.equal(readRequests(view).length,2);
  await view.reply(readRequests(view)[1],toolResult({uri:fileUri,kind:"file",version:"v2",data:"Current content"}));
  assert.equal(view.nodes.previewContent.textContent,"Current content");
  assert.match(view.nodes.previewMeta.textContent,/v2/);
});

test("resourceLink capability attaches references only after explicit action without reading or messaging",async()=>{
  const view=await launch({resourceLink:{}});await files(view);
  const add=rowActions(view)[1];add.onclick();await flush();
  assert.equal(refs(view).length,1);
  assert.equal(readRequests(view).length,0);
  assert.deepEqual(JSON.parse(JSON.stringify(refs(view)[0].params)),{content:[fileItem]});
  await view.reply(refs(view)[0],{});
  assert.equal(view.nodes.references.children.length,1);
  add.onclick();await flush();assert.equal(refs(view).length,1);
  await selectProject(view,secondProject);
  assert.equal(view.nodes.references.children.length,1);
  assert.equal(view.sent.filter(request=>request.method==="ui/message").length,0);
  assert.equal(view.calls("run_shell").length,0);
});

for (const modality of ["resource","text","none"]) test(`${modality} fallback materializes bounded content instead of relying on a URI`,async()=>{
  const view=await launch(modality==="none" ? {} : {[modality]:{}});await files(view);
  rowActions(view)[1].onclick();await flush();
  assert.equal(readRequests(view).length,1);
  assert.equal(refs(view).length,0);
  await view.reply(readRequests(view)[0],toolResult({uri:fileUri,kind:"file",data:{content:"actual file text " + "汉".repeat(20000)}}));
  if(modality==="none") {
    assert.equal(refs(view).length,0);
    assert.equal(view.nodes.copyLabel.hidden,false);
    assert.match(view.nodes.referenceText.value,/actual file text/);
    assert.ok(new TextEncoder().encode(view.nodes.referenceText.value).length<5000);
  } else {
    assert.equal(refs(view).length,1);
    const block=refs(view)[0].params.content[0];
    assert.equal(block.type,modality);
    const text=modality==="resource" ? block.resource.text : block.text;
    assert.match(text,/actual file text/);assert.match(text,/Preview truncated/);
    await view.reply(refs(view)[0],{});
  }
  assert.equal(view.sent.filter(request=>request.method==="ui/message").length,0);
});

test("context failure does not claim selection; remove replaces complete App content",async()=>{
  const view=await launch({resourceLink:{}});await files(view);
  rowActions(view)[1].onclick();await flush();await view.reject(refs(view)[0]);
  assert.equal(view.nodes.references.children.length,0);
  assert.match(view.nodes.referenceStatus.textContent,/Unavailable/);
  rowActions(view)[1].onclick();await flush();await view.reply(refs(view)[1],{});
  view.nodes.references.children[0].children[1].onclick();await flush();
  assert.equal(refs(view)[2].params.content.length,0);
  await view.reply(refs(view)[2],{});assert.equal(view.nodes.references.children.length,0);
});

const anotherFile={...fileItem,uri:"webcodex-resource://file/YWdlbnQ6ZGVtbw/cm9vdA/c3JjL2IucnM",name:"src/b.rs",_meta:{...fileItem._meta,path:"src/b.rs"}};
for (const modality of ["text","resource"]) test(`${modality} rapid reference selections preserve both resources across Host acknowledgements`,async()=>{
  const view=await launch({[modality]:{}});await files(view,[fileItem,anotherFile]);
  rowActions(view)[1].onclick();
  view.nodes.rows.children[1].children[1].children[1].onclick();await flush();
  assert.equal(readRequests(view).length,1);
  await view.reply(readRequests(view)[0],toolResult({uri:fileUri,kind:"file",data:{text:"First reference"}}));
  assert.equal(refs(view).length,1);
  await view.reply(refs(view)[0],{});
  assert.equal(readRequests(view).length,2);
  await view.reply(readRequests(view)[1],toolResult({uri:anotherFile.uri,kind:"file",data:{text:"Second reference"}}));
  assert.equal(refs(view).length,2);
  assert.equal(refs(view)[1].params.content.length,2);
  assert.match(JSON.stringify(refs(view)[1].params.content),/First reference/);
  assert.match(JSON.stringify(refs(view)[1].params.content),/Second reference/);
  await view.reply(refs(view)[1],{});
  assert.equal(view.nodes.references.children.length,2);
  assert.equal(view.sent.filter(request=>request.method==="ui/message").length,0);
});

test("resourceLink rapid selections serialize complete context updates without losing the second choice",async()=>{
  const view=await launch({resourceLink:{}});await files(view,[fileItem,anotherFile]);
  rowActions(view)[1].onclick();
  view.nodes.rows.children[1].children[1].children[1].onclick();await flush();
  assert.equal(refs(view).length,1);
  await view.reply(refs(view)[0],{});
  assert.equal(refs(view).length,2);
  assert.deepEqual(JSON.parse(JSON.stringify(refs(view)[1].params.content)),[fileItem,anotherFile]);
  await view.reply(refs(view)[1],{});
  assert.equal(view.nodes.references.children.length,2);
});

test("canonical Host removal invalidates queued reference selections as well as an in-flight update",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});view.toolResult({projects:page([projectItem])});
  await initialize(view,{resourceLink:{}},null,true);await files(view,[fileItem,anotherFile]);
  rowActions(view)[1].onclick();
  view.nodes.rows.children[1].children[1].children[1].onclick();await flush();
  view.notification("ui/notifications/host-context-changed",{"openai/modelContext":{updateId:"removed",content:[]}});await flush();
  await view.reply(refs(view)[0],{_meta:{"openai/modelContext":{updateId:"previous"}}});
  assert.equal(refs(view).length,1);
  assert.equal(view.nodes.references.children.length,0);
});

test("OpenAI canonical context restores remount references and follows host removal without messages",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});view.toolResult({projects:page([projectItem])});
  const stripped={...fileItem};delete stripped._meta;
  await initialize(view,{resourceLink:{}},{updateId:"restored",content:[stripped]},true);
  assert.equal(view.nodes.references.children.length,1);
  view.notification("ui/notifications/host-context-changed",{"openai/modelContext":{updateId:"removed",content:[]}});await flush();
  assert.equal(view.nodes.references.children.length,0);
  view.notification("ui/notifications/host-context-changed",{"openai/modelContext":null});await flush();
  assert.equal(refs(view).length,0);
  assert.equal(view.sent.filter(request=>request.method==="ui/message").length,0);
});

test("host canonical removal during an update wins over a stale acknowledgement",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});view.toolResult({projects:page([projectItem])});
  await initialize(view,{resourceLink:{}},null,true);await files(view);
  rowActions(view)[1].onclick();await flush();
  view.notification("ui/notifications/host-context-changed",{"openai/modelContext":{updateId:"removed",content:[]}});await flush();
  await view.reply(refs(view)[0],{_meta:{"openai/modelContext":{updateId:"previous"}}});
  assert.equal(view.nodes.references.children.length,0);
});

test("host brand never implies adapter support and forged messages cannot alter state",async()=>{
  const view=await launch({resourceLink:{}});await files(view);
  view.notification("ui/notifications/host-context-changed",{"openai/modelContext":{updateId:"fake",content:[fileItem]}});await flush();
  assert.equal(view.nodes.references.children.length,0);
  view.toolInput({project:secondProject},{});await flush();
  assert.equal(view.nodes.projectSelect.value,project);
});

test("overview refresh pauses while hidden, resumes on visibility, and teardown cancels pending work",async()=>{
  const view=await launch({resourceLink:{}});await selectProject(view);
  await view.reply(view.calls("work_result_state")[0],toolResult({work_result:baseState}));
  await view.reply(view.calls("list_sessions")[0],toolResult({sessions:[]}));
  assert.equal(view.timers.size,1);
  await view.visibility(true);assert.equal(view.timers.size,0);
  await view.fireTimers(10000);assert.equal(view.calls("work_result_state").length,1);
  await view.visibility(false);assert.equal(view.calls("work_result_state").length,2);
  const pending=view.calls("work_result_state")[1];
  const old=view.nodes.overview.children;
  await view.teardown();assert.equal(view.timers.size,0);
  await view.reply(pending,toolResult({work_result:{...baseState,workspace:{...baseState.workspace,clean:true}}}));
  assert.equal(view.nodes.overview.children,old);
  assert.equal(view.nodes.refresh.disabled,true);
  assert.ok(view.sent.some(request=>request.method==="notifications/cancelled" && request.params.requestId===pending.id));
});

for(const failure of ["error","timeout"]) test(`initial handshake ${failure} blocks tools and reports failure`,async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({project});
  await view.initialize(failure);
  assert.equal(view.calls("work_result_state").length,0);
  assert.match(view.nodes.status.textContent,/Unavailable/);
  assert.equal(view.nodes.projectSelect.disabled,true);
});

test("malformed, oversized and unsuccessful tool results surface protocol errors",async()=>{
  for(const result of [
    toolResult({items:[{...fileItem,uri:"file:///etc/passwd"}]}),
    {content:[{type:"text",text:" ".repeat(262145)+JSON.stringify({success:true,output:page([fileItem])})}]},
    {structuredContent:{success:false,error:"Permission required"}},
  ]) {
    const view=await launch();await selectProject(view);view.nodes.tabFiles.onclick();await flush();
    await view.reply(search(view,"file")[0],result);
    assert.equal(view.nodes.rows.children.length,0);
    assert.match(view.nodes.status.textContent,/Unavailable/);
  }
});

test("Project chooser supports narrowed search and paging without selecting a result",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});
  view.toolResult({projects:page([projectItem],100)});await initialize(view);
  assert.equal(view.nodes.projectsMore.hidden,false);
  view.nodes.projectsMore.onclick();await flush();
  assert.equal(search(view,"project")[0].params.arguments.offset,100);
  await view.reply(search(view,"project")[0],toolResult(page([{...projectItem,uri:"webcodex-resource://project/b3RoZXI/cm9vdA",name:secondProject,_meta:{kind:"project",project:secondProject}}])));
  assert.equal(view.nodes.projectSelect.children.length,3);
  assert.equal(view.nodes.projectSelect.value,"");
  view.nodes.projectQuery.value="汉".repeat(250);
  view.nodes.projectSearchForm.onsubmit({preventDefault(){}});await flush();
  assert.equal(search(view,"project").at(-1).params.arguments.query.length,200);
  assert.equal(search(view,"project").at(-1).params.arguments.offset,0);
});

test("Goals are owner-scoped and their search omits Project selectors",async()=>{
  const view=await launch();await selectProject(view);view.nodes.tabGoals.onclick();await flush();
  assert.deepEqual({...search(view,"goal")[0].params.arguments},{kind:"goal",offset:0,limit:50});
});

test("Project refs retain canonical identity after chooser search changes",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});
  const ref="p_ref_test";
  view.toolResult({projects:page([{...projectItem,_meta:{...projectItem._meta,project_ref:ref}}])});
  await initialize(view);await selectProject(view,ref);
  view.nodes.projectQuery.value="other";view.nodes.projectSearchForm.onsubmit({preventDefault(){}});await flush();
  await view.reply(search(view,"project")[0],toolResult(page([])));
  await view.reply(view.calls("work_result_state")[0],toolResult({work_result:baseState}));
  assert.equal(view.nodes.overview.children.length,4);
  assert.equal(view.nodes.projectReference.hidden,false);
});

test("malformed overview without an authoritative Project cannot be rendered",async()=>{
  const view=await launch();await selectProject(view,secondProject);
  const {project:_,...state}=baseState;
  await view.reply(view.calls("work_result_state")[0],toolResult({work_result:state}));
  assert.equal(view.nodes.overview.children.length,0);
  assert.match(view.nodes.status.textContent,/selection mismatch/);
});

test("canonical file text is previewed directly and a late fallback read never attaches after scope switch",async()=>{
  const view=await launch({text:{}});await files(view);
  rowActions(view)[0].onclick();await flush();
  await view.reply(readRequests(view)[0],toolResult({uri:fileUri,kind:"file",data:{text:"Readable source",read_revision:"rev"}}));
  assert.equal(view.nodes.previewContent.textContent,"Readable source");
  view.nodes.tabFiles.onclick();await flush();await view.reply(search(view,"file").at(-1),toolResult(page([{...fileItem,uri:"webcodex-resource://file/YWdlbnQ6ZGVtbw/cm9vdA/b3RoZXI"}])));
  rowActions(view)[1].onclick();await flush();const pending=readRequests(view).at(-1);
  await selectProject(view,secondProject);
  await view.reply(pending,toolResult({uri:pending.params.arguments.uri,kind:"file",data:"Previous Project"}));
  assert.equal(refs(view).length,0);
});

test("standard text and embedded references restore from canonical context on remount",async()=>{
  for(const block of [
    {type:"text",text:`WebCodex reference\nURI: ${fileUri}\nFile\ncontent`},
    {type:"resource",resource:{uri:fileUri,mimeType:"text/plain",text:"content"}},
  ]) {
    const view=app("mcp_workbench_app.html");view.toolInput({});view.toolResult({projects:page([projectItem])});
    await initialize(view,{text:{},resource:{}},{updateId:"remount",content:[block]},true);
    assert.equal(view.nodes.references.children.length,1);
  }
});

test("late canonical launcher scope resolves short selectors without overriding a user choice",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({project:"p_short",session_id:"s_short"});
  await initialize(view);
  assert.equal(view.nodes.projectSelect.value,"p_short");
  view.toolResult({project,session_id});await flush();
  assert.equal(view.nodes.projectSelect.value,project);
  assert.equal(view.nodes.sessionSelect.value,session_id);
  await selectProject(view,secondProject);
  view.toolResult({project,session_id});await flush();
  assert.equal(view.nodes.projectSelect.value,secondProject);
});

test("empty result before a named input does not swallow the explicit launcher scope",async()=>{
  const view=app("mcp_workbench_app.html");await initialize(view);
  view.toolResult({});view.toolInput({project});await flush();
  assert.equal(view.nodes.projectSelect.value,project);
  assert.equal(view.calls("work_result_state").length,1);
});

test("Your Goals remains browsable and referenceable when Project discovery is unavailable",async()=>{
  const view=app("mcp_workbench_app.html");view.toolInput({});
  view.toolResult({projects:{...page([]),total:0,list_truncated:true,incomplete:"project_discovery_unavailable"}});
  await initialize(view,{resourceLink:{}});
  assert.match(view.nodes.selectionState.textContent,/Project discovery unavailable.*Goals/);
  assert.match(view.nodes.status.textContent,/Project discovery unavailable/);
  assert.equal(view.nodes.projectSelect.value,"");
  assert.equal(view.calls("work_result_state").length,0);
  view.nodes.tabGoals.onclick();await flush();
  assert.equal(view.nodes.browser.hidden,false);
  assert.match(view.nodes.selectionState.textContent,/Your Goals/);
  assert.deepEqual({...search(view,"goal")[0].params.arguments},{kind:"goal",offset:0,limit:50});
  const goalItem={type:"resource_link",uri:"webcodex-resource://goal/d2NfZ29hbF9hYmM",name:"Independent Goal",_meta:{kind:"goal",goal_id:"wc_goal_abc"}};
  await view.reply(search(view,"goal")[0],toolResult(page([goalItem])));
  assert.equal(view.nodes.rows.children.length,1);
  rowActions(view)[1].onclick();await flush();
  await view.reply(refs(view)[0],{});
  assert.equal(view.nodes.references.children.length,1);
  view.nodes.query.value="Independent";view.nodes.searchForm.onsubmit({preventDefault(){}});await flush();
  assert.equal(search(view,"goal").at(-1).params.arguments.query,"Independent");
  assert.equal("project" in search(view,"goal").at(-1).params.arguments,false);
  view.nodes.tabFiles.onclick();await flush();
  assert.equal(view.nodes.browser.hidden,true);
  assert.equal(search(view,"file").length,0);
});

test("no-Project Goals can refresh and resume after visibility changes",async()=>{
  const view=await launch();view.nodes.tabGoals.onclick();await flush();
  await view.reply(search(view,"goal")[0],toolResult(page([])));
  view.nodes.refresh.onclick();await flush();
  assert.equal(search(view,"goal").length,2);
  await view.visibility(true);await view.visibility(false);
  assert.equal(search(view,"goal").length,3);
  assert.equal(view.calls("list_sessions").length,0);
  assert.equal(view.calls("work_result_state").length,0);
});

test("overview uses canonical active_requests/events activity and leaves unknown workspace state unavailable",async()=>{
  const view=await launch();await selectProject(view);
  const activeActivity={available:true,active:true,truncated:false,active_requests:[{label:"Reading current files",started_at_ms:100}],events:[{label:"Finished earlier check",ended_at_ms:90}]};
  await view.reply(view.calls("work_result_state")[0],toolResult({work_result:{...baseState,workspace:{git_available:true},activity:activeActivity}}));
  assert.equal(view.nodes.overview.children[0].children[1].textContent,"Unavailable");
  assert.equal(view.nodes.overview.children[2].children[1].textContent,"Reading current files");
  view.nodes.refresh.onclick();await flush();
  await view.reply(view.calls("work_result_state").at(-1),toolResult({work_result:{...baseState,activity:{available:true,active:false,truncated:true,active_requests:[],events:[{label:"Newest review",ended_at_ms:300},{label:"Older edit",ended_at_ms:200}]}}}));
  assert.equal(view.nodes.overview.children[2].children[1].textContent,"Newest review");
});

test("text reference added after an earlier preview re-reads current resource content",async()=>{
  const view=await launch({text:{}});await files(view);
  rowActions(view)[0].onclick();await flush();
  await view.reply(readRequests(view)[0],toolResult({uri:fileUri,kind:"file",version:"old",data:{text:"Previous content"}}));
  rowActions(view)[1].onclick();await flush();
  assert.equal(readRequests(view).length,2);
  await view.reply(readRequests(view)[1],toolResult({uri:fileUri,kind:"file",version:"new",data:{text:"Latest reference content"}}));
  assert.match(refs(view)[0].params.content[0].text,/Latest reference content/);
  assert.doesNotMatch(refs(view)[0].params.content[0].text,/Previous content/);
  await view.reply(refs(view)[0],{});
});
