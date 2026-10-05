import { describe, expect, it } from "vitest";
import { runnerServerAddress } from "./runner-server-address";

describe("Runner onboarding address", () => {
  it.each(["http://localhost:8080", "http://LOCALHOST.", "http://runner.localhost", "http://127.3.2.1", "http://2130706433", "http://[::1]", "http://[::ffff:127.0.0.1]", "http://0.0.0.0", "http://[::]"])("explains local-only address %s", value => {
    expect(runnerServerAddress(value).issue).toBe("loopback");
  });
  it.each(["https://api.openai.com", "https://platform.openai.com", "https://chatgpt.com", "https://tunnel.openai.com"])("does not offer OpenAI %s as a Runner Server", value => {
    expect(runnerServerAddress(value).issue).toBe("openai");
  });
  it.each(["", "tunnel_example", "ftp://server.example", "https://user:secret@server.example", "https://server.example/mcp", "https://server.example?key=x", "https://server.example#fragment"])("rejects invalid origin %s", value => {
    expect(runnerServerAddress(value)).toEqual({ origin: null, issue: "invalid" });
  });
  it("canonicalizes direct Server URLs without inferring remote reachability", () => {
    expect(runnerServerAddress(" https://SERVER.example:443/ ")).toEqual({ origin: "https://server.example", issue: null });
    expect(runnerServerAddress("http://192.168.1.5:8080").issue).toBeNull();
    expect(runnerServerAddress("https://openai.com.example").issue).toBeNull();
  });
});
