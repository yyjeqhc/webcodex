import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { ReadinessBanner } from "./ReadinessBanner";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";

const api=vi.hoisted(()=>({resumeSavedRuntime:vi.fn(),getState:vi.fn()}));
vi.mock("../../lib/desktop-api",()=>({desktopApi:api}));
const failure={code:"mcp_provider_executable_unavailable",message:"MCP provider unavailable",next_action:"Edit or disable",details:{provider_kind:"mcp",provider_id:"missing",provider_name:"Browser tools"}};
const stopped={readiness:{server:"stopped",runner:"stopped",project:"none",exposure:"disabled",runtime_ready:false},runtime_error:failure} as DesktopState;
beforeEach(()=>{vi.resetAllMocks();localStorage.clear();localStorage.setItem("webcodex.desktop.locale","en-US");});
function show(state:DesktopState,onState=vi.fn(),onProviders=vi.fn()) {
  render(<LocaleProvider><ReadinessBanner state={state} onState={onState} onDiagnostics={vi.fn()} onRuntime={vi.fn()} onConnection={vi.fn()} onProviders={onProviders}/></LocaleProvider>);
  return {onState,onProviders};
}

it("shows the precise failed optional provider on Home with recovery navigation",()=>{
  const {onProviders}=show(stopped);
  expect(screen.getByRole("alert")).toHaveTextContent("MCP · Browser tools");
  expect(screen.getByText(/Your configuration is saved/)).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"Edit or disable provider"}));
  expect(onProviders).toHaveBeenCalledWith("mcp");
  expect(screen.getByRole("button",{name:"Start Runtime"})).toBeEnabled();
  expect(screen.queryByRole("button",{name:"Select Runtime folder…"})).not.toBeInTheDocument();
});

it("points operator-owned providers to their Runner configuration",()=>{
  const configPath = "C:\\isolated-fixture\\runner.toml";
  const {onProviders}=show({...stopped,runtime_error:{...failure,details:{...failure.details,provider_config_path:configPath}}});
  expect(screen.getByRole("alert")).toHaveTextContent(configPath);
  expect(screen.getByText(/Edit its program path or remove its entry there/)).toBeVisible();
  expect(screen.queryByRole("button",{name:"Edit or disable provider"})).not.toBeInTheDocument();
  expect(onProviders).not.toHaveBeenCalled();
});

it("refreshes terminal native state after a failed start instead of keeping Starting",async()=>{
  const initial={...stopped,runtime_error:null};
  api.resumeSavedRuntime.mockRejectedValue(failure); api.getState.mockResolvedValue(stopped);
  const {onState}=show(initial);
  fireEvent.click(screen.getByRole("button",{name:"Start Runtime"}));
  await waitFor(()=>expect(onState).toHaveBeenCalledWith(stopped));
  expect(screen.getByRole("alert")).toHaveTextContent("Browser tools");
  expect(screen.queryByRole("heading",{name:"Runtime is starting"})).not.toBeInTheDocument();
});

it("keeps the concrete error when the follow-up state observation also fails",async()=>{
  api.resumeSavedRuntime.mockRejectedValue(failure); api.getState.mockRejectedValue(new Error("observation failed"));
  const {onState}=show({...stopped,runtime_error:null});
  fireEvent.click(screen.getByRole("button",{name:"Start Runtime"}));
  await screen.findByText("MCP · Browser tools");
  expect(onState).not.toHaveBeenCalled();
});
