import { selectOption } from "../../test/select-option";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import { connectionFixture } from "../../test/connections-fixtures";
import type { CloudflareConnectionStatus, TunnelProfileRequest } from "../../models/connections-tools";
import { ConnectionEditor } from "./ConnectionEditor";
import { CloudflareConnectionCard } from "./CloudflareConnectionCard";
const api=vi.hoisted(()=>({saveTunnelProfile:vi.fn(),cloudflareConnection:vi.fn(),openDiagnosticResource:vi.fn()}));
const clipboard=vi.hoisted(()=>({writeText:vi.fn()}));
vi.mock("../../lib/desktop-api",()=>({desktopApi:api}));
vi.mock("@tauri-apps/plugin-clipboard-manager",()=>clipboard);
function wrap(content:React.ReactNode) { return <DesktopMantineProvider><LocaleProvider>{content}</LocaleProvider></DesktopMantineProvider>; }
const status:CloudflareConnectionStatus={profile_id:"cf",configuration_id:"cf-version",server_instance_id:"server-selected",process_generation:7,lifecycle:"running",public_origin:"https://current.example",oauth_configured:false,observed_authorization:false,configured_revision:4,applied_revision:4,local_target:"http://127.0.0.1:8900",reason_code:null};
beforeEach(()=>{vi.resetAllMocks();localStorage.setItem("webcodex.desktop.locale","en-US");api.saveTunnelProfile.mockResolvedValue({});api.cloudflareConnection.mockResolvedValue(status);clipboard.writeText.mockResolvedValue(undefined);});

describe("Cloudflare connection configuration",()=>{
  it("saves a Named origin and write-only token through the existing profile operation",async()=>{
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={null} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    selectOption(screen.getByRole("combobox", { name: "Connection provider" }), "cloudflare_named");
    fireEvent.change(screen.getByLabelText("Name"),{target:{value:"Named"}});
    fireEvent.change(screen.getByLabelText("Tunnel ID"),{target:{value:"named-id"}});
    fireEvent.change(screen.getByLabelText("Fixed HTTPS origin"),{target:{value:"https://mcp.example/"}});
    fireEvent.change(screen.getByLabelText("Tunnel token"),{target:{value:"private-named-token"}});
    fireEvent.click(screen.getByRole("button",{name:"Save"}));
    await waitFor(()=>expect(saved?.provider).toEqual({kind:"cloudflare_named",public_origin:"https://mcp.example",tunnel_id:"named-id"}));
    expect(saved).toMatchObject({cloudflare_token:"private-named-token",api_key:null,host_mode:"embedded",expected_revision:null});
    expect(screen.getByLabelText("Tunnel token")).toHaveValue("");
    expect(api.saveTunnelProfile.mock.calls[0][0].cloudflare_token).toBeNull();
  });
  it("saves Quick profiles without requesting Tunnel or API credentials",async()=>{
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={null} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    selectOption(screen.getByRole("combobox", { name: "Connection provider" }), "cloudflare_quick");
    expect(screen.queryByLabelText("API key")).toBeNull();expect(screen.queryByLabelText("Tunnel ID")).toBeNull();expect(screen.queryByLabelText("Tunnel token")).toBeNull();
    expect(screen.getByText(/addresses change on reconnect/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Name"),{target:{value:"Quick"}});
    fireEvent.click(screen.getByRole("button",{name:"Save"}));
    await waitFor(()=>expect(saved?.provider).toEqual({kind:"cloudflare_quick"}));
    expect(saved).toMatchObject({cloudflare_token:null,api_key:null,tunnel_id:"",oauth_dynamic_registration_enabled:false});
  });
  it("routes legacy setup through Connect Gemini and never creates a parallel Cloudflare store",()=>{
    const onSetup=vi.fn(),onClose=vi.fn();
    render(wrap(<ConnectionEditor profile={null} persistentLocal={false} onState={vi.fn()} onClose={onClose} onSetup={onSetup} />));
    selectOption(screen.getByRole("combobox", { name: "Connection provider" }), "cloudflare_quick");
    expect(screen.getByRole("button",{name:"Save"})).toBeDisabled();
    fireEvent.click(screen.getByRole("button",{name:"Connect Gemini"}));
    expect(onSetup).toHaveBeenCalledOnce();expect(onClose).toHaveBeenCalledOnce();expect(api.saveTunnelProfile).not.toHaveBeenCalled();
  });
  it("retains the exact saved Named token and revision with an empty replacement field",async()=>{
    const profile=connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_named",public_origin:"https://current.example",tunnel_id:"named"},tunnel_id:"named",source:"environment",host_mode:"embedded"});
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={profile} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    expect(screen.getByLabelText("Tunnel token")).toHaveValue("");expect(screen.getByRole("combobox", { name: "Connection provider" })).toBeDisabled();expect(screen.getByLabelText("Tunnel ID")).toBeDisabled();
    fireEvent.click(screen.getByRole("button",{name:"Save"}));
    await waitFor(()=>expect(saved).toMatchObject({id:"cf",expected_revision:4,cloudflare_token:null}));
  });
  it("omits an untouched DCR policy on edit and explains explicit revocation", async () => {
    const profile = connectionFixture({ provider: {kind:"cloudflare_quick"}, oauth_dynamic_registration_enabled: true });
    render(wrap(<ConnectionEditor profile={profile} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    const policy = screen.getByRole("checkbox", {name: "Allow automatic OAuth client registration"});
    expect(policy).toBeChecked();
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(policy).toBeEnabled());
    expect(api.saveTunnelProfile.mock.calls[0][0]).not.toHaveProperty("oauth_dynamic_registration_enabled");
    fireEvent.click(policy);
    expect(screen.getByText(/revokes dynamically registered clients/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(api.saveTunnelProfile.mock.lastCall?.[0]).toMatchObject({oauth_dynamic_registration_enabled:false,expected_revision:profile.revision}));
  });
});

describe("Cloudflare private native controls and OAuth handoff",()=>{
  it("routes an incorrect Runner owner to explicit Runtime repair",async()=>{
    const repair=vi.fn();
    api.cloudflareConnection.mockResolvedValue({...status,lifecycle:"error",public_origin:null,reason_code:"cloudflare_owner_repair_required"});
    render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"}})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onRepair={repair} />));
    expect(await screen.findByRole("alert")).toHaveTextContent("Repair the Environment user and Runner owner");
    fireEvent.click(screen.getByRole("button",{name:"Open Runtime setup"}));
    expect(repair).toHaveBeenCalledOnce();
    expect(api.cloudflareConnection.mock.calls.every(([request])=>request.action==="status")).toBe(true);
  });
  it("offers an explicit Server restart when the Server has not loaded its first Cloudflare connection",async()=>{
    const restart=vi.fn();
    api.cloudflareConnection.mockRejectedValue({code:"cloudflare_ingress_not_applied",message:"Cloudflare is not loaded",next_action:"Restart the Server"});
    render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"},host_mode:"standalone",autostart:true})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onRestartServer={restart} />));
    expect(await screen.findByText("Restart WebCodex Server to apply saved connections")).toBeInTheDocument();
    expect(screen.getByText(/Active Server and Runner work may be interrupted/)).toBeInTheDocument();
    expect(screen.getByRole("button",{name:"Start"})).toBeDisabled();
    expect(restart).not.toHaveBeenCalled();
    expect(api.cloudflareConnection.mock.calls.every(([request])=>request.action==="status")).toBe(true);
    fireEvent.click(screen.getByRole("button",{name:"Restart Server"}));
    expect(restart).toHaveBeenCalledOnce();
  });
  it("does not infer a Server restart from an ordinary control failure",async()=>{
    api.cloudflareConnection.mockRejectedValue(new Error("private-response-body"));
    render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"}})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onRestartServer={vi.fn()} />));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"Restart Server"})).toBeNull();
    expect(screen.queryByText("private-response-body")).toBeNull();
    expect(screen.queryByText("Restart WebCodex Server to apply saved connections")).toBeNull();
  });
  it("preserves structured Cloudflare failure diagnostics without leaking private error text or offering unsafe retry", async () => {
    api.cloudflareConnection.mockRejectedValue({
      code: "cloudflare_authentication_failed",
      message: "Bearer wc_pat_private_canary",
      next_action: "Do not display this private instruction",
      diagnostic: {
        component: "auth",
        stage: "cloudflare_status",
        reason_code: "http_forbidden",
        recovery_kind: "user_action",
        safe_action: "repair_user_credential",
        retry_allowed: false,
        http_status: 403,
        resource_id: "cf-profile-1",
      },
    });
    render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"}})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} />));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveAttribute("data-error-component", "auth");
    expect(alert).toHaveTextContent("HTTP 403");
    expect(alert).toHaveTextContent("cloudflare_status");
    expect(alert).not.toHaveTextContent("wc_pat_private_canary");
    expect(alert).not.toHaveTextContent("private instruction");
    expect(screen.queryByRole("button", {name: "Retry"})).toBeNull();
  });
  function mount() {return render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"},credential_present:false,host_mode:"embedded"})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} />));}
  it("copies the current MCP origin and keeps observed authorization separate from process readiness",async()=>{
    mount();
    fireEvent.click(await screen.findByRole("button",{name:"Copy MCP address"}));
    await waitFor(()=>expect(clipboard.writeText).toHaveBeenCalledWith("https://current.example/mcp"));
    expect(screen.getByText("OAuth authorization not yet observed")).toBeInTheDocument();
    expect(screen.getByText("OAuth not configured")).toBeInTheDocument();
    expect(screen.getByText("http://127.0.0.1:8900")).toBeInTheDocument();
    expect(screen.getByText(/addresses change on reconnect/)).toBeInTheDocument();
  });
  it("shows the first client secret only after explicit callback handoff and clears it on close",async()=>{
    api.cloudflareConnection.mockImplementation(async request=>request.action==="configure_oauth"?{client_id:"oauth-client",client_secret:"one-time-secret",already_configured:false}:status);
    mount();
    fireEvent.click(await screen.findByRole("button",{name:"Configure OAuth"}));
    expect(screen.queryByLabelText("Client secret")).toBeNull();
    expect(screen.getByLabelText("OAuth scopes")).toHaveValue("runtime:read runner:manage session:collaborate project:read project:write job:run");
    expect(screen.getByText("mcp:local")).toBeInTheDocument();
    expect(screen.getByText("ssh:local")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("OAuth callback URL"),{target:{value:"https://client.example/callback"}});
    fireEvent.click(screen.getByRole("button",{name:"Create client credentials"}));
    expect(await screen.findByLabelText("Client secret")).toHaveValue("one-time-secret");
    expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"configure_oauth",profile_id:"cf",server_instance_id:"server-selected",process_generation:7,redirect_uri:"https://client.example/callback",scopes:["runtime:read","runner:manage","session:collaborate","project:read","project:write","job:run"],replace:false});
    fireEvent.click(screen.getAllByRole("button",{name:"Close"})[0]);
    await waitFor(()=>expect(screen.queryByLabelText("Client secret")).toBeNull());
    fireEvent.click(screen.getByRole("button",{name:"Configure OAuth"}));
    expect(screen.queryByDisplayValue("one-time-secret")).toBeNull();
  });
  it("reconnects Quick with the observed Server fence and saved revision",async()=>{
    api.cloudflareConnection.mockImplementation(async request => request.action === "stop" ? {...status,lifecycle:"stopped",public_origin:null} : status);
    mount();
    fireEvent.click(await screen.findByRole("button",{name:"Reconnect"}));
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"start",profile_id:"cf",server_instance_id:"server-selected",expected_revision:4,expected_generation:7,expected_configuration_id:"cf-version"}));
    expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"stop",profile_id:"cf",server_instance_id:"server-selected",process_generation:7});
  });
  it("clears the old Quick address and authorization observation before reconnect completes",async()=>{
    let completeStop!: (value:CloudflareConnectionStatus)=>void;
    api.cloudflareConnection.mockImplementation(request=>request.action==="stop"?new Promise<CloudflareConnectionStatus>(resolve=>{completeStop=resolve;}):Promise.resolve(request.action==="start"?{...status,lifecycle:"starting",public_origin:null,observed_authorization:false}:{...status,oauth_configured:true,observed_authorization:true}));
    mount();
    await screen.findByText("OAuth authorization observed");
    expect(screen.getByText("OAuth configured")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button",{name:"Reconnect"}));
    expect(screen.queryByRole("button",{name:"Copy MCP address"})).toBeNull();
    expect(screen.queryByText("https://current.example/mcp")).toBeNull();
    expect(screen.queryByText("OAuth authorization observed")).toBeNull();
    completeStop({...status,lifecycle:"stopped",public_origin:null});
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"start",profile_id:"cf",server_instance_id:"server-selected",expected_revision:4,expected_generation:7,expected_configuration_id:"cf-version"}));
  });
  it("withdraws the address and retains fenced Stop/Reconnect controls while forwarding is disconnected",async()=>{
    api.cloudflareConnection.mockResolvedValue({...status,lifecycle:"disconnected",reason_code:"network_disconnected",oauth_configured:true,observed_authorization:true});
    mount();
    await screen.findByText("Forwarding disconnected");
    expect(screen.queryByText("https://current.example/mcp")).toBeNull();
    expect(screen.getByText("OAuth authorization observed")).toBeInTheDocument();
    expect(screen.getByRole("button",{name:"Reconnect"})).toBeEnabled();
    fireEvent.click(screen.getByRole("button",{name:"Stop"}));
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"stop",profile_id:"cf",server_instance_id:"server-selected",process_generation:7}));
  });
  it("requires explicit replacement for already configured OAuth and never reads the old secret",async()=>{
    api.cloudflareConnection.mockImplementation(async request=>request.action==="configure_oauth"?{client_id:"replacement",client_secret:"replacement-secret",already_configured:false}:{...status,oauth_configured:true});
    mount();fireEvent.click(await screen.findByRole("button",{name:"Configure OAuth"}));
    expect(screen.queryByRole("button",{name:"Create client credentials"})).toBeNull();
    expect(screen.getByText(/secret cannot be read again/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("OAuth callback URL"),{target:{value:"https://client.example/new"}});
    fireEvent.click(screen.getByRole("button",{name:"Replace credentials and reauthorize"}));
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith(expect.objectContaining({action:"configure_oauth",replace:true,server_instance_id:"server-selected",process_generation:7})));
  });
  it("does not retry a rejected action against a replacement Server",async()=>{
    api.cloudflareConnection.mockImplementation(async request=> {
      if(request.action === "start") throw new Error("stale instance");
      return {...status,lifecycle:"stopped"};
    });
    mount();
    fireEvent.click(await screen.findByRole("button",{name:"Start"}));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    const starts=api.cloudflareConnection.mock.calls.filter(([request])=>request.action==="start");
    expect(starts).toHaveLength(1);
    expect(starts[0][0]).toMatchObject({server_instance_id:"server-selected",expected_revision:4});
    expect(screen.getByRole("button",{name:"Start"})).toBeDisabled();
  });

  it("requires the displayed profile to match the current desired revision before starting",async()=>{
    api.cloudflareConnection.mockResolvedValue({...status,lifecycle:"stopped",configured_revision:5});
    mount();
    await screen.findByText("Connection unavailable");
    expect(screen.getByRole("button",{name:"Start"})).toBeDisabled();
    expect(api.cloudflareConnection.mock.calls.every(([request])=>request.action==="status")).toBe(true);
  });

  it("explains why an unselected standalone Cloudflare service cannot start",async()=>{
    api.cloudflareConnection.mockResolvedValue({...status,lifecycle:"stopped"});
    render(wrap(<CloudflareConnectionCard profile={connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_quick"},credential_present:false,host_mode:"standalone",autostart:false})} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} />));
    await screen.findByText("Stopped");
    expect(screen.getByRole("button",{name:"Start"})).toBeDisabled();
    expect(screen.getByText(/select this standalone Cloudflare service/)).toBeInTheDocument();
  });

  const gemini = connectionFixture({ id:"cf", revision:4, provider:{kind:"cloudflare_quick"}, host_mode:"embedded", oauth_dynamic_registration_enabled:true });
  function card(profile = gemini) {
    return wrap(<CloudflareConnectionCard profile={profile} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} />);
  }
  it.each([
    {public_origin:"http://current.example"}, {public_origin:"https://user@current.example"}, {public_origin:"https://current.example/mcp?private"},
    {lifecycle:"starting"}, {lifecycle:"disconnected"}, {registration_available:false}, {configured_revision:5},
  ])("withholds Gemini handoff for an unusable observation %o", async change => {
    api.cloudflareConnection.mockResolvedValue({...status, registration_available:true, ...change});
    await act(async () => { render(card()); });
    expect(screen.queryByRole("button", {name:"Copy MCP address"})).toBeNull();
    expect(screen.queryByRole("button", {name:"Open Gemini"})).toBeNull();
    expect(screen.queryByText("MCP endpoint ready")).toBeNull();
  });
  it("explains where the Gemini URL comes from and reveals it only after starting and verifying Cloudflare", async () => {
    api.cloudflareConnection.mockResolvedValueOnce({...status, lifecycle: "stopped", public_origin: null, registration_available: false})
      .mockResolvedValue({...status, lifecycle: "running", registration_available: true});
    render(card());
    expect(await screen.findByText("No verified public MCP address yet")).toBeInTheDocument();
    expect(await screen.findByText(/Connect Gemini to prepare a public MCP address/)).toBeInTheDocument();
    expect(screen.queryByText("https://current.example/mcp")).toBeNull();
    expect(screen.queryByRole("button", {name: "Copy MCP address"})).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Start public connection"}));
    await waitFor(() => expect(screen.getByText("https://current.example/mcp")).toBeInTheDocument());
    expect(screen.getByText("Gemini MCP address (copy the full URL)")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", {name: "Copy MCP address"}));
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledWith("https://current.example/mcp"));
    expect(api.cloudflareConnection).toHaveBeenCalledWith({
      action: "start", profile_id: "cf", server_instance_id: "server-selected", expected_revision: 4, expected_generation: 7, expected_configuration_id: "cf-version",
    });
  });
  it("withholds the observed address until public discovery verifies it", async () => {
    api.cloudflareConnection.mockResolvedValue({...status, registration_available: false, reason_code: "cloudflare_discovery_pending"});
    render(card());
    expect(await screen.findByText("No verified public MCP address yet")).toBeInTheDocument();
    expect(screen.queryByText("https://current.example/mcp")).toBeNull();
    expect(screen.queryByRole("button", {name: "Copy MCP address"})).toBeNull();
    expect(screen.queryByRole("button", {name: "Open Gemini"})).toBeNull();
    expect(clipboard.writeText).not.toHaveBeenCalled();
  });
  it("rechecks the generation at clipboard handoff", async () => {
    api.cloudflareConnection.mockResolvedValueOnce({...status, registration_available:true})
      .mockResolvedValue({...status, process_generation:8, public_origin:null, registration_available:false});
    render(card());
    fireEvent.click(await screen.findByRole("button", {name:"Copy MCP address"}));
    await waitFor(() => expect(screen.queryByText("https://current.example/mcp")).toBeNull());
    expect(clipboard.writeText).not.toHaveBeenCalled();
  });
  it("provides a concrete Runtime owner-repair reason instead of implying that a public URL exists", async () => {
    const repair = vi.fn();
    api.cloudflareConnection.mockResolvedValue({...status, lifecycle: "error", public_origin: null, reason_code: "cloudflare_owner_repair_required"});
    render(wrap(<CloudflareConnectionCard profile={gemini} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onRepair={repair} onEnableRegistration={vi.fn()} />));
    expect(await screen.findByText("No verified public MCP address yet")).toBeInTheDocument();
    expect(screen.getByText(/Repair the Environment user and Runner owner/)).toBeInTheDocument();
    expect(screen.getByText("cloudflare_owner_repair_required")).toBeInTheDocument();
    expect(screen.queryByRole("button", {name:"Copy MCP address"})).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Open Runtime setup"}));
    expect(repair).toHaveBeenCalledOnce();
  });
  it("keeps legacy manual OAuth status behind a separate advanced disclosure in Gemini mode", async () => {
    api.cloudflareConnection.mockResolvedValue({...status, registration_available: true});
    render(wrap(<CloudflareConnectionCard profile={gemini} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onEnableRegistration={vi.fn()} />));
    expect(await screen.findByText("Gemini MCP address (copy the full URL)")).toBeInTheDocument();
    const manual = screen.getByText("Manual OAuth client (advanced; not needed for Gemini)");
    expect(manual.closest("details")).not.toHaveAttribute("open");
    expect(screen.getByText("OAuth not configured")).not.toBeVisible();
    expect(screen.getByRole("button", {name:"Copy MCP address"})).toBeEnabled();
  });

  it("keeps the manual client address in advanced controls until app registration is enabled", async () => {
    const profile = {...gemini, oauth_dynamic_registration_enabled:false};
    render(wrap(<CloudflareConnectionCard profile={profile} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onEnableRegistration={vi.fn()} />));
    expect(await screen.findByText("App registration is off")).toBeInTheDocument();
    const address = screen.getByText("https://current.example/mcp");
    expect(address.closest("details")).not.toBeNull();
    expect(address.closest("details")).not.toHaveAttribute("open");
    expect(screen.queryByRole("button", {name:"Open Gemini"})).toBeNull();
  });

  it("offers apply for an older standalone Server materialization while retaining the generation-fenced Stop action", async () => {
    api.cloudflareConnection.mockResolvedValue({...status, configured_revision:3, reason_code:"cloudflare_restart_server_required"});
    const apply = vi.fn();
    render(wrap(<CloudflareConnectionCard profile={{...gemini, host_mode:"standalone", autostart:true}} canStart busy={false} onEdit={vi.fn()} onDelete={vi.fn()} onRestartServer={apply} />));
    fireEvent.click(await screen.findByRole("button", {name:"Restart Server"}));
    expect(apply).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", {name:"Stop"})).toBeEnabled();
    expect(screen.queryByRole("button", {name:"Open Gemini"})).toBeNull();
  });

  it("restores a Named connection and its independent observations on reopening without launching anything", async () => {
    const named = {...gemini, provider:{kind:"cloudflare_named" as const, public_origin:"https://current.example", tunnel_id:"named"}};
    api.cloudflareConnection.mockResolvedValue({...status, registration_available:true, observed_authorization:true, observed_tool_call:true});
    const view = render(card(named));
    expect(await screen.findByText("Successful MCP tool call observed on this run")).toBeInTheDocument();
    view.unmount();
    api.cloudflareConnection.mockResolvedValue({...status, server_instance_id:"new-server", registration_available:true, observed_authorization:true});
    render(card(named));
    expect(await screen.findByText("OAuth authorization observed")).toBeInTheDocument();
    expect(screen.getByText("https://current.example/mcp")).toBeInTheDocument();
    expect(screen.queryByText("Successful MCP tool call observed on this run")).toBeNull();
    expect(api.cloudflareConnection.mock.calls.every(([request]) => request.action === "status")).toBe(true);
  });
  it("keeps a copy failure recoverable without losing the verified URL or surfacing native error text", async () => {
    api.cloudflareConnection.mockResolvedValue({...status, registration_available:true});
    clipboard.writeText.mockRejectedValue(new Error("private diagnostic"));
    render(card());
    fireEvent.click(await screen.findByRole("button", {name:"Copy MCP address"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("copy it manually");
    expect(screen.getByText("https://current.example/mcp")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("private diagnostic");
    expect(JSON.stringify(localStorage)).not.toMatch(/current\.example|private diagnostic/);
  });
  it("discards a delayed status for a replaced profile and never resumes a reconnect after unmount", async () => {
    let finish!: (value:CloudflareConnectionStatus) => void;
    api.cloudflareConnection.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const view = render(card());
    api.cloudflareConnection.mockResolvedValue({...status, profile_id:"second", public_origin:"https://second.example", registration_available:true});
    view.rerender(card({...gemini, id:"second"}));
    await screen.findByText("https://second.example/mcp");
    await act(async () => finish({...status, registration_available:true}));
    expect(screen.queryByText("https://current.example/mcp")).toBeNull();
    api.cloudflareConnection.mockImplementationOnce(() => new Promise(resolve => {finish = resolve;}));
    fireEvent.click(screen.getByRole("button", {name:"Reconnect"}));
    view.unmount();
    await act(async () => finish({...status, profile_id:"second", lifecycle:"stopped"}));
    expect(api.cloudflareConnection.mock.calls.some(([request]) => request.action === "start")).toBe(false);
  });

});
