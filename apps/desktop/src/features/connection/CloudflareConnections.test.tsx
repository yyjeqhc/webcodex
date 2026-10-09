import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import { connectionFixture } from "../../test/connections-fixtures";
import type { CloudflareConnectionStatus, TunnelProfileRequest } from "../../models/connections-tools";
import { ConnectionEditor } from "./ConnectionEditor";
import { CloudflareConnectionCard } from "./CloudflareConnectionCard";
const api=vi.hoisted(()=>({saveTunnelProfile:vi.fn(),cloudflareConnection:vi.fn()}));
const clipboard=vi.hoisted(()=>({writeText:vi.fn()}));
vi.mock("../../lib/desktop-api",()=>({desktopApi:api}));
vi.mock("@tauri-apps/plugin-clipboard-manager",()=>clipboard);
function wrap(content:React.ReactNode) { return <DesktopMantineProvider><LocaleProvider>{content}</LocaleProvider></DesktopMantineProvider>; }
const status:CloudflareConnectionStatus={profile_id:"cf",server_instance_id:"server-selected",process_generation:7,lifecycle:"running",public_origin:"https://current.example",oauth_configured:false,observed_authorization:false,configured_revision:4,applied_revision:4,local_target:"http://127.0.0.1:8900",reason_code:null};
beforeEach(()=>{vi.resetAllMocks();localStorage.setItem("webcodex.desktop.locale","en-US");api.saveTunnelProfile.mockResolvedValue({});api.cloudflareConnection.mockResolvedValue(status);clipboard.writeText.mockResolvedValue(undefined);});

describe("Cloudflare connection configuration",()=>{
  it("saves a Named origin and write-only token through the existing profile operation",async()=>{
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={null} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    fireEvent.change(screen.getByLabelText("Connection provider"),{target:{value:"cloudflare_named"}});
    fireEvent.change(screen.getByLabelText("Name"),{target:{value:"Named"}});
    fireEvent.change(screen.getByLabelText("Tunnel ID"),{target:{value:"named-id"}});
    fireEvent.change(screen.getByLabelText("Fixed HTTPS origin"),{target:{value:"https://mcp.example/"}});
    fireEvent.change(screen.getByLabelText("Tunnel token"),{target:{value:"private-named-token"}});
    fireEvent.click(screen.getByRole("button",{name:"Save & Apply"}));
    await waitFor(()=>expect(saved?.provider).toEqual({kind:"cloudflare_named",public_origin:"https://mcp.example",tunnel_id:"named-id"}));
    expect(saved).toMatchObject({cloudflare_token:"private-named-token",api_key:null,host_mode:"embedded",expected_revision:null});
    expect(screen.getByLabelText("Tunnel token")).toHaveValue("");
    expect(api.saveTunnelProfile.mock.calls[0][0].cloudflare_token).toBeNull();
  });
  it("saves Quick profiles without requesting Tunnel or API credentials",async()=>{
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={null} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    fireEvent.change(screen.getByLabelText("Connection provider"),{target:{value:"cloudflare_quick"}});
    expect(screen.queryByLabelText("API Key")).toBeNull();expect(screen.queryByLabelText("Tunnel ID")).toBeNull();expect(screen.queryByLabelText("Tunnel token")).toBeNull();
    expect(screen.getByText(/addresses change on reconnect/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Name"),{target:{value:"Quick"}});
    fireEvent.click(screen.getByRole("button",{name:"Save & Apply"}));
    await waitFor(()=>expect(saved?.provider).toEqual({kind:"cloudflare_quick"}));
    expect(saved).toMatchObject({cloudflare_token:null,api_key:null,tunnel_id:""});
  });
  it("routes legacy setup through Runtime repair and never creates a parallel Cloudflare store",()=>{
    const onSetup=vi.fn(),onClose=vi.fn();
    render(wrap(<ConnectionEditor profile={null} persistentLocal={false} onState={vi.fn()} onClose={onClose} onSetup={onSetup} />));
    fireEvent.change(screen.getByLabelText("Connection provider"),{target:{value:"cloudflare_quick"}});
    expect(screen.getByRole("button",{name:"Save & Apply"})).toBeDisabled();
    fireEvent.click(screen.getByRole("button",{name:"Open Runtime setup"}));
    expect(onSetup).toHaveBeenCalledOnce();expect(onClose).toHaveBeenCalledOnce();expect(api.saveTunnelProfile).not.toHaveBeenCalled();
  });
  it("retains the exact saved Named token and revision with an empty replacement field",async()=>{
    const profile=connectionFixture({id:"cf",revision:4,provider:{kind:"cloudflare_named",public_origin:"https://current.example",tunnel_id:"named"},tunnel_id:"named",source:"environment",host_mode:"embedded"});
    let saved:TunnelProfileRequest|undefined;
    api.saveTunnelProfile.mockImplementation(async request=>{saved=structuredClone(request);return {};});
    render(wrap(<ConnectionEditor profile={profile} persistentLocal onState={vi.fn()} onClose={vi.fn()} />));
    expect(screen.getByLabelText("Tunnel token")).toHaveValue("");expect(screen.getByLabelText("Connection provider")).toBeDisabled();expect(screen.getByLabelText("Tunnel ID")).toBeDisabled();
    fireEvent.click(screen.getByRole("button",{name:"Save & Apply"}));
    await waitFor(()=>expect(saved).toMatchObject({id:"cf",expected_revision:4,cloudflare_token:null}));
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
    mount();
    fireEvent.click(await screen.findByRole("button",{name:"Reconnect"}));
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"start",profile_id:"cf",server_instance_id:"server-selected",expected_revision:4}));
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
    await waitFor(()=>expect(api.cloudflareConnection).toHaveBeenCalledWith({action:"start",profile_id:"cf",server_instance_id:"server-selected",expected_revision:4}));
  });
  it("retains address and fenced Stop/Reconnect controls while forwarding is disconnected",async()=>{
    api.cloudflareConnection.mockResolvedValue({...status,lifecycle:"disconnected",reason_code:"network_disconnected",oauth_configured:true,observed_authorization:true});
    mount();
    await screen.findByText("Forwarding disconnected");
    expect(screen.getByText("https://current.example/mcp")).toBeInTheDocument();
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
    await screen.findByText("Stopped");
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

});
