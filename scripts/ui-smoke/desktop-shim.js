// Harness-only Tauri boundary, no native API calls or config reads.
window.__fixtureReady=(async()=>{
const state=await (await fetch('/__fixture/desktop-state'+location.search)).json();
let settings={paths:{instruction_files:['/fixture/instructions.md'],skill_roots:['/fixture/skills']},file_access:{configured_roots:[],effective_roots:['/fixture/home'],using_default_roots:true,allow_cwd_anywhere:false},plugin_ids:['fixture-plugin'],target:{config_path:'/fixture/runner.toml',client_id:'fixture-runner',server_url:'http://127.0.0.1:1'},can_restart:true};
let permissions={supported:true,foreground:true,desktop_accessibility:false,desktop_screen_recording:false};
const build={schema_version:1,binary:'webcodex-desktop',version:'0.4.3',git_commit:'fixture-revision',git_dirty:false,built_at:null,target:'fixture',architecture:'x64',desktop_runtime_contract:{min_generation:1,max_generation:1}};
const runtime={source:{kind:'bundled'},selection_revision:1,desktop_contract:build.desktop_runtime_contract,selected:{candidate_id:'fixture-runtime',source:{kind:'bundled'},selection_revision:1,checked_at_ms:1,directory:'/fixture/bin',binaries:[],compatibility:'compatible',build_alignment:'exact',advisories:[],error_code:null,fingerprint:'fixture'},candidate:null,previous_source:null,last_switch:null,unavailable_code:null,active_jobs:0,can_switch:true,switch_unavailable_reason:null};
const diagnostic={schema_version:1,observed_at_ms:Date.now(),trace:{mode:'off',effective_mode:'off',revision:'fixture-trace',available:true,restart_required:false,can_restart:true,error_code:null},configuration:{reason_code:null,backup_available:false,primary_fingerprint:null},resources:['runtime_console','app_data'],can_copy_console_credential:false,credential_copy_fence:null,report:{schema_version:1,desktop:build,last_webcodex_call:null},markdown:'Fixture report'};
let instructions={path:'/fixture/desktop/instructions/AGENTS.md',exists:true,content:'Use the project conventions.\nVerify changes before finishing.',revision:'fixture-instructions'};
const update={state:'up_to_date',latest:null,update_available:false,show_banner:false,cached:true,last_check_at_ms:1,manual_error:null,automatic_download:true,download:{phase:'idle',version:null,platform:'linux-x64',downloaded_bytes:0,total_bytes:null,error_kind:null,installation:'source_build',can_install:false,pending_install:false,legacy_release:false,cancelled:false}};
let authorized=false;
let providers=[];
state.coding_agents={revision:0,profiles:[],global_settings:null,restart_required:false,config_error:false,max_enabled:8};
const authorization=()=>({target:structuredClone(settings.target),can_authorize:true,coding_agents:authorized,ssh_resources:authorized});
const sshInventory=()=>({runner:'fixture-runner',available:authorized,observation_id:authorized?'fixture-observation':null,resources:[],error_kind:authorized?null:'insufficient_scope'});

window.__fixtureCalls=[];let callbackId=1;const callbacks=new Map();
window.__TAURI_INTERNALS__={transformCallback(fn){const id=callbackId++;callbacks.set(id,fn);return id},unregisterCallback(id){callbacks.delete(id)},convertFileSrc(){throw Error('Unexpected file access')},async invoke(cmd,args={}){
 window.__fixtureCalls.push({cmd,args:structuredClone(args)});
 switch(cmd){
 case 'workspace_query':{
  const request=args.request||{};
  const projectRows=state.saved_projects.map(project=>({id:project.runtime_project_id,name:project.path.split('/').pop(),path:project.path,connected:true,sessions:{active_sessions:1,running_sessions:0,latest_updated_at:Math.floor(Date.now()/1000)}}));
  if(request.kind==='overview')return {client_id:'fixture-runner',connected:true,status:'online',coding_agent_providers:structuredClone(providers),visible_project_count:projectRows.length,projects:projectRows,projects_truncated:false,recent_sessions:{sessions:[],truncated:false,scan_truncated:false}};
  if(request.kind==='windows')return {windows:[]};
  if(request.kind==='projects')return {projects:projectRows,total:projectRows.length,truncated:false};
  if(request.kind==='extensions')return {project:request.project,runner:'fixture-runner',can_reload_plugins:true,instructions:{files:[],scan_complete:true,truncated:false},skills:{available:true,catalog:{skills:[],truncated:false}},plugins:{available:true,catalog:{plugins:[],truncated:false}}};
  if(request.kind==='project_git')return {branch:'codex/fixture-ui',clean:true,git_available:true,non_git_project:false,files:[],files_total:0,files_truncated:false};
  if(request.kind==='sessions')return {sessions:[],truncated:false};
  throw Error('Unimplemented workspace fixture '+request.kind);
 }
 case 'plugin:event|listen':return callbackId++;
 case 'plugin:event|unlisten':return null;
 case 'get_desktop_state':case 'refresh_runtime_status':case 'observe_chatgpt_activity':case 'resume_saved_runtime':return structuredClone(state);
 case 'runner_capability_authorization':return authorization();
 case 'authorize_runner_capabilities':if(args.request.confirmed!==true)throw Error('Explicit fixture authorization required');authorized=true;return authorization();
 case 'ssh_resource_list':return sshInventory();
 case 'save_coding_agent':{
  const request=args.request;
  if(request.expected_revision!==state.coding_agents.revision)throw Error('Stale fixture revision');
  state.coding_agents.profiles=state.coding_agents.profiles.filter(profile=>profile.provider_id!==request.previous_id);
  state.coding_agents.profiles.push(structuredClone(request.profile));
  state.coding_agents.revision++;state.coding_agents.restart_required=true;
  return structuredClone(state);
 }
 case 'remove_coding_agent':state.coding_agents.profiles=state.coding_agents.profiles.filter(profile=>profile.provider_id!==args.request.provider_id);state.coding_agents.revision++;state.coding_agents.restart_required=true;return structuredClone(state);
 case 'restart_owned_runner':providers=state.coding_agents.profiles.filter(profile=>profile.enabled).map(({provider_id,name})=>({provider_id,name}));state.coding_agents.restart_required=false;return structuredClone(state);
 case 'get_bounded_activity':return [];
 case 'get_launch_at_login':return false;
 case 'set_launch_at_login':return args.request.enabled;
 case 'get_runner_settings':return structuredClone(settings);
 case 'get_desktop_build_info':return structuredClone(build);
 case 'get_runtime_settings':case 'recheck_runtime':return structuredClone(runtime);
 case 'get_diagnostics':return structuredClone(diagnostic);
 case 'check_for_updates':return structuredClone(update);
 case 'get_update_download_state':return structuredClone(update.download);
 case 'managed_instructions_read':return structuredClone(instructions);
 case 'managed_instructions_save':if(args.request.expected_revision!==instructions.revision)throw Error('Stale fixture instructions');instructions={...instructions,content:args.request.content,revision:instructions.revision+'-saved'};return structuredClone(instructions);
 case 'update_tunnel_proxy':state.tunnel_proxy={...state.tunnel_proxy,mode:args.request.mode,custom_url:args.request.customUrl};return structuredClone(state);
 case 'add_runner_plugin':settings.plugin_ids.push(args.request.provider.id);return structuredClone(state);
 case 'update_runner_settings':settings.paths=structuredClone(args.request.paths);return structuredClone(state);
 case 'get_computer_permissions':return {...permissions};
 case 'request_computer_permission':if(args.action==='accessibility')permissions.desktop_accessibility=true;if(args.action==='screen_recording')permissions.desktop_screen_recording=true;return {...permissions};
 case 'update_tunnel_config':if(args.request.action==='save'){state.openai_tunnel_config.saved_tunnel_id=args.request.tunnelId;state.openai_tunnel_config.effective_tunnel_id=args.request.tunnelId;}return structuredClone(state);
 case 'save_tunnel_profile':{
  const request=args.request;
  state.connections={profiles:[{id:'fixture-connection',name:request.name,tunnel_id:request.tunnel_id,credential_present:Boolean(request.api_key),enabled:false,autostart:request.autostart,revision:1,source:'file',lifecycle:'stopped',pid:null,health:'unknown',last_error:null,ready:false,runtime_directory:null,local_mcp_url:null,logs:[]}],running:0,needs_attention:0,config_error:false};
  return structuredClone(state);
 }
 case 'activate_local_project':{let project=state.saved_projects.find(p=>p.path===args.request.projectPath);if(!project){project={path:args.request.projectPath,allowed_root:args.request.projectPath,is_git_repository:true,runtime_project_id:'agent:fixture-runner:gamma'};state.saved_projects.push(project);}state.project=project;return structuredClone(state);}
 case 'plugin:dialog|open':return '/fixture/gamma';
 case 'inspect_project':return {path:args.request.projectPath,allowed_root:args.request.projectPath,is_git_repository:true,runtime_project_id:'agent:fixture-runner:gamma'};
 case 'configure_local_setup':state.project={path:args.request.projectPath,allowed_root:args.request.projectPath,is_git_repository:true,runtime_project_id:'agent:fixture-runner:gamma'};state.saved_projects.push(state.project);return structuredClone(state);
 case 'plugin:clipboard-manager|write_text':return null;
 default:throw Error('Unimplemented Tauri fixture '+cmd);
 }
}};
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
localStorage.setItem('webcodex.desktop.locale','en-US');
if(!new URLSearchParams(location.search).has('permissions'))localStorage.setItem('desktop-permissions-explained','1');else localStorage.removeItem('desktop-permissions-explained');
window.dispatchEvent(new Event('fixture-ready'));
})();
