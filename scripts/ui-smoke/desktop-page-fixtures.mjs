import assert from "node:assert/strict";

// Shared deterministic observations for mounted page resize/theme checks only.
const now = Date.now();
const longPath = '/fixture/团队项目/repository-with-a-long-readable-directory-name/'.repeat(3);
const projects = ['alpha', 'beta'].map(name => ({ id: `agent:fixture-runner:${name}`, name: `${name} — Desktop layout verification 团队项目`, path: `${longPath}${name}`, client_id: 'fixture-runner', connected: true, sessions: { active_sessions: 2, running_sessions: 1, latest_updated_at: Math.floor(now / 1000) } }));
const sessions = Array.from({ length: 4 }, (_, index) => ({ session_id: `fixture-session-${index}`, project_id: projects[index % 2].id, title: `Review Desktop preferences and verify long project metadata — 检查布局 ${index}`, updated_at: Math.floor(now / 1000) - index * 100, lifecycle: index === 2 ? 'closed' : 'active', running_call: index === 0, running_jobs: index === 1 ? 1 : 0, running_jobs_complete: true, overview: { attention: { open_todos: index === 0 ? 2 : 0, open_questions: index === 1 ? 1 : 0, open_risks: 0 }, reported_progress: { text: 'Checking the renderer with long but representative project metadata and multiple lines of context. 正在检查项目界面。', reported_at: Math.floor(now / 1000) }, validation: { state: 'unavailable', unresolved_failure_count: 0 } } }));
const windows = Array.from({ length: 10 }, (_, index) => ({ client_window_key: `fixture-window-${index}`, source: 'chatgpt', last_project: projects[index % 2].id, last_seen_at_ms: now - index * 30000, last_meaningful_activity_at_ms: now - index * 30000, active_count: index === 0 ? 1 : 0, linked_session_count: 1 }));
const details = Object.fromEntries(windows.map((row, index) => [row.client_window_key, { ...row, linked_sessions: [{ project: row.last_project, workflow_session_id: sessions[index % 4].session_id, title: sessions[index % 4].title }], active_requests: [], activity: [{ meaningful: true, tool_name: 'read_files', project: row.last_project, status: 'succeeded', started_at_ms: now - index * 30000 - 1000, ended_at_ms: now - index * 30000, request_observed_at_ms: now - index * 30000 - 1000, response_handed_at_ms: now - index * 30000, service_ms: 1000 }], sessions_truncated: false, activity_truncated: false }]));

export async function installDesktopPageFixtures(page, locale, mode = "ready") {
    await page.route('**/__fixture/desktop-state*', async route => {
      const response = await route.fetch(); const state = await response.json();
      if (mode === 'onboarding' || mode === 'completion') { state.topology = null; state.project = null; state.saved_projects = []; state.readiness.runtime_ready = false; }
      if (mode === 'operation') state.current_operation = {id:'fixture-operation',kind:'configure_environment',phase:'running',cancellable:true};
      state.workspace_runner = { client_id: 'fixture-runner', config_path: `${longPath}runner.toml`, server_url: 'http://127.0.0.1:1' };
      await route.fulfill({ response, json: state });
    });
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch(); let source = await response.text();
      const marker = "if(request.kind==='overview')return";
      assert(source.includes(marker), 'fixture workspace marker');
      const override = `if(request.kind==='overview')return ${JSON.stringify({ client_id: 'fixture-runner', connected: true, status: 'online', runners: [{ client_id: 'fixture-runner', connected: true, status: 'online' }], projects_available: true, visible_projects: projects.length, projects, projects_truncated: false, recent_sessions: { sessions, truncated: false, scan_truncated: false } })};if(request.kind==='projects')return ${JSON.stringify({ projects, total: projects.length, truncated: false })};if(request.kind==='windows')return ${JSON.stringify({ windows })};if(request.kind==='window')return ${JSON.stringify(details)}[request.client_window_key];`;
      source = source.replace(marker, override + marker);
      const inventoryEntry = (id, kind = 'file') => ({ id, component: id.split('.')[0], purpose: id.replaceAll('.', '_'), source: 'configured', configured_path: longPath + id, canonical_path: longPath + id, directory_to_open: longPath, status: 'present', kind, category: 'mixed_configuration', log_source: null });
      const inventory = { schema_version: 1, observed_at_ms: now, environment_id: null, local_server: true, local_runner: true, service_scope: 'user', revision: 'fixture-inventory-1', roots: [inventoryEntry('environment.root', 'directory'), inventoryEntry('desktop.root', 'directory')], entries: ['server.configuration', 'runner.configuration', 'desktop.locale', 'tunnel.profiles'].map(id => inventoryEntry(id)), issues: [], builds: [], identities: [] };
      source = source.replace("case 'get_runner_settings':", `case 'get_path_inventory':return ${JSON.stringify(inventory)};case 'get_runner_settings':`);
      source = source.replace("case 'get_launch_at_login':return false;", "case 'desktop_shell_restore_only':return false;case 'read_desktop_navigation':return null;case 'set_desktop_locale':case 'write_desktop_navigation':return null;case 'get_local_update_status':return {environment_id:null,selection_revision:1,installed_observed:false,installed_checked_at_ms:null,observation_error:false,confirmation:null,running:[],view:{schema_version:1,download:update.download,installed:[],candidate:null,candidate_components:[],upgrade:null,blockers:['unsupported_installation'],restart_required:false}};case 'get_launch_at_login':return false;");
      source = source.replace("localStorage.setItem('webcodex.desktop.locale','en-US');", `if(!localStorage.getItem('webcodex.desktop.locale'))localStorage.setItem('webcodex.desktop.locale',${JSON.stringify(locale)});`);
      source = source.replace("effective_roots:['/fixture/home']", `effective_roots:[${JSON.stringify(longPath)}]`);
      if (mode === 'update') {
        const candidate = {version:'0.6.0',target:{platform:'linux-x64',format:'deb'},source_sha:'a'.repeat(40),manifest_sha256:'b'.repeat(64),installer_sha256:'c'.repeat(64)};
        const confirmation = {services:[{component:'server',scope:'user'},{component:'runner',scope:'user'}],service_inventory_complete:true,candidate,target:{environment_id:'fixture-environment',manifest_sha256:'b'.repeat(64),operation_id:null},selection_revision:1};
        const download = {phase:'ready_to_install',version:'0.6.0',platform:'linux-x64',downloaded_bytes:1,total_bytes:1,error_kind:null,installation:'managed',can_install:true,pending_install:false,legacy_release:false,cancelled:false};
        const update = {state:'available',latest:{version:'0.6.0',runtime_version:'0.6.0',release_url:'https://example.invalid/fixture',compatibility:'desktop_required'},update_available:true,show_banner:true,cached:false,last_check_at_ms:1,manual_error:null,automatic_download:false,download};
        const local = {environment_id:'fixture-environment',selection_revision:1,installed_observed:false,installed_checked_at_ms:null,observation_error:false,confirmation,running:[],view:{schema_version:1,download,installed:[],candidate,candidate_components:[],upgrade:null,blockers:[],restart_required:false}};
        source = source.replace(/^const update=.*;$/m,'const update='+JSON.stringify(update)+';');
        source = source.replace(/case 'get_local_update_status':return .*?;case 'get_launch_at_login'/,"case 'get_local_update_status':return "+JSON.stringify(local)+";case 'get_launch_at_login'");
      }
      if (mode === 'completion') {
        const next = {topology:{experience:'full',server:{kind:'local'},runner:{kind:'local'},exposure:{kind:'none'},enrollment:{kind:'managed_pairing'}},persistent_environment:'fixture-environment',environment_setup:{environment_id:'fixture-environment',mode:'create',runner:true,configured:true,project_path:null,service_scope:'user',server_url:'http://127.0.0.1:1'},readiness:{server:'ready',runner:'ready',exposure:'disabled',project:'not_configured',runtime_ready:true,ready_for_chatgpt:false},saved_projects:[],project:null};
        source = source.replace("case 'get_runner_settings':", "case 'configure_environment':Object.assign(state,"+JSON.stringify(next)+");return structuredClone(state);case 'get_runner_settings':");
      }
      await route.fulfill({ response, body: source });
    });
}
