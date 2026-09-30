#!/usr/bin/env python3
"""Opt-in coding/interactive-pipe acceptance on disposable real MCP/WS services.

Build dogfood Server/Runner first. No production profile, paid API, PTY, external
model, or global client config is used. Input retransmissions are deliberate
same-id receipt reconciliation, never automatic business-operation retries.
"""
from __future__ import annotations
import argparse
import base64
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from urllib.request import Request
from urllib.error import HTTPError
from e2e_session_continuity_ws import ROOT, Smoke, check, stop

SCOPES = ["runtime:read", "project:read", "project:write", "job:run", "session:collaborate"]

class InteractiveSmoke(Smoke):
    def post(self, path, body, token):
        if path != "/mcp":
            return super().post(path, body, token)
        # Modern stateless metadata is per-request, not inherited from an old
        # initialize response. _wc belongs to this admitted protocol surface.
        params = body.setdefault("params", {})
        params["_meta"] = {"io.modelcontextprotocol/protocolVersion":"2026-07-28",
                           "io.modelcontextprotocol/clientCapabilities":{}}
        headers = {"Authorization":"Bearer "+token, "Content-Type":"application/json",
                   "Accept":"application/json, text/event-stream", "MCP-Protocol-Version":"2026-07-28",
                   "MCP-Method":body["method"]}
        if "name" in params:
            headers["MCP-Name"] = params["name"]
        request = Request(self.url+path,json.dumps(body).encode(),method="POST",headers=headers)
        try:
            with self.opener.open(request, timeout=40) as response:
                raw=response.read(2*1024*1024)
        except HTTPError as error:
            raw=error.read(2*1024*1024)
        return json.loads(raw)

    def initialize(self, token):
        self.rpc_id += 1
        response=self.post("/mcp",{"jsonrpc":"2.0","id":self.rpc_id,"method":"tools/list","params":{}},token)
        names={tool.get("name") for tool in response.get("result",{}).get("tools",[])}
        check({"run_process","read_files","edit_project_files","observe_jobs","wait_for_job_readiness","call_runtime_tool"} <= names,
              "fresh stateless tool inventory is missing a core callable")
        check("job_write_input" not in names,"input should remain a gateway capability, not another Direct tool")

    def raw_tool(self, name, args, token, *, direct=False):
        self.rpc_id += 1
        args=dict(args); invocation=args.pop("_wc",None)
        params = {"name": name, "arguments": args} if direct else {
            "name": "call_runtime_tool", "arguments": {"tool": name, "arguments": args}}
        if invocation is not None:
            params["arguments"]["_wc"]=invocation
        reply = self.post("/mcp", {"jsonrpc":"2.0", "id":self.rpc_id,
            "method":"tools/call", "params":params}, token)
        result = reply.get("result", {})
        check(isinstance(result.get("structuredContent"), dict), f"{name}: no structured result; fields={list(reply)}; protocol_error={str(reply.get('error',''))[:500]}")
        return result

    def call(self, name, args, token=None, *, direct=False, success=True):
        result = self.raw_tool(name, args, token or self.owner, direct=direct)["structuredContent"]
        check(result.get("success") is success,
              f"{name}: success mismatch; kind={result.get('output',{}).get('error_kind')}; message={str(result.get('error',''))[:200]}")
        return result.get("output", {})

    def observe(self, job):
        return self.call("observe_jobs", {"items":[{"job_id":job}], "tail_lines":80}, direct=True)["items"][0]

    def ready_prompt(self, job, marker="ready>"):
        # Fixture-only dependency wait under one absolute deadline; never extend
        # it on progress or restart an execution. Production clients use the same
        # retained Job observation and don't use this helper for heartbeat loops.
        deadline = time.monotonic() + 20
        item = self.observe(job)
        while marker not in item.get("stdout_tail", ""):
            check(not item.get("terminal"), f"interactive child ended before prompt: {item.get('status')} {item.get('error')}")
            remaining = deadline-time.monotonic()
            check(remaining > 0, "prompt observation deadline")
            item = self.call("observe_jobs", {"items":[{"job_id":job,"after_observation_token":item["observation_token"]}],
                "wait_secs":max(1,min(5,int(remaining))),"tail_lines":80}, direct=True)["items"][0]
        return item

    def terminal(self, job):
        result = self.call("wait_for_job_readiness", {"job_ids":[job],"mode":"all","wait_secs":30}, direct=True)
        check(not result.get("pending_job_ids"), "fixture Job did not terminate by its deadline")
        item = self.observe(job)
        check(item.get("terminal") is True,"terminal readiness must identify original Job")
        return item

    def start_pipe(self, program, timeout=30):
        output = self.call("run_process", {"project":self.project_ref,"session_id":self.session_ref,
            "interactive":True,"executable":sys.executable,"args":["-u","-c",program],
            "timeout_secs":timeout}, direct=True)
        check(output.get("execution_state")=="pending" and output.get("interactive") is True,
              "interactive initiation was flattened into ordinary command completion")
        job = output.get("job_id") or output.get("continuation",{}).get("arguments",{}).get("items",[{}])[0].get("job_id")
        check(isinstance(job,str),"public Job identity missing")
        return job

    def write(self, job, key, data="", close=False, *, success=True, project=None, token=None):
        return self.call("job_write_input", {"project":project or self.project_ref,"job_id":job,
            "input_id":key,"data":data,"close":close}, token, success=success)

    def prepare(self):
        self.env["WEBCODEX_MCP_HOST_PROFILE"]="host_code_mode"
        self.start_server()
        for username in ["input-owner","input-foreign"]:
            check(self.post("/api/users/create",{"username":username},self.bootstrap).get("success"),"create fixture user")
        self.owner=self.pat("input-owner",SCOPES)
        self.foreign=self.pat("input-foreign",SCOPES)
        agent=self.post("/api/agent-tokens/create",{"username":"input-owner","client_id":"input-runner","name":"input-fixture"},self.bootstrap)
        check(isinstance(agent.get("token"),str),"fixture Runner enrollment")
        self.project_dir=self.root/"project";self.project_dir.mkdir()
        other=self.root/"other";other.mkdir();registry=self.root/"projects";registry.mkdir()
        for name,path in [("demo",self.project_dir),("other",other)]:
            (registry/f"{name}.toml").write_text(f'id = "{name}"\npath = {json.dumps(str(path))}\n')
        subprocess.run(["git","init","-q", "-b","main",str(self.project_dir)],env=self.env,stdin=subprocess.DEVNULL,check=True)
        (self.project_dir/"AGENTS.md").write_text("Temporary smoke fixture only. Do not access any other directory.\n")
        (self.project_dir/"one.txt").write_text("before one\n");(self.project_dir/"two.txt").write_text("before two\n")
        self.runner_config=self.root/"runner.toml"
        self.runner_config.write_text('\n'.join([f'server_url = "{self.url}"',f'token = {json.dumps(agent["token"])}',
            'client_id = "input-runner"','owner = "input-owner"','transport = "websocket"',
            f'project_registry_dir = {json.dumps(str(registry))}', '[policy]', 'allow_raw_shell = true',
            f'allowed_roots = [{json.dumps(str(self.project_dir))}, {json.dumps(str(other))}]'])+'\n')
        self.runner_config.chmod(0o600)
        self.runner=self.launch("webcodex-runner",["--config",str(self.runner_config)],self.env)
        self.project="agent:input-runner:demo"
        self.wait_ready(self.project,self.owner);self.initialize(self.owner)
        started=self.call("work_on_project",{"project":self.project,"instruction":"Verify disposable read/edit and interactive pipe lifecycle",
            "_wc":{"context":["project.instructions"]}},direct=True)
        self.project_ref=started["project_ref"];self.session_ref=started["session_ref"]
        check(self.project_ref.startswith("~p") and self.session_ref.startswith("~s"),"short refs not issued")

    def workflow(self):
        args={"project":self.project_ref,"items":[{"path":"one.txt"},{"path":"two.txt"}],"_wc":{"record":self.session_ref}}
        before=self.call("read_files",args,direct=True)
        revisions=[item["output"]["read_revision"] for item in before["items"]]
        changed=self.call("edit_project_files",{"project":self.project_ref,"_wc":{"record":self.session_ref},"changes":[
            {"kind":"edit","path":path,"expected_read_revision":float(rev),"edits":[{"kind":"replace_exact","old_text":"before","new_text":"after"}]}
            for path,rev in zip(["one.txt","two.txt"],revisions)]},direct=True)
        check(changed.get("changed") is True,"batched edit effect lost")
        after=self.call("read_files",args) # same full contract through gateway
        check(all("after" in item["output"]["text"] for item in after["items"]),"fresh read mismatch")
        self.call("edit_project_files",{"project":self.project_ref,"changes":[{"kind":"edit","path":"one.txt",
            "expected_read_revision":revisions[0],"edits":[{"kind":"replace_exact","old_text":"after","new_text":"stale"}]}]},direct=True,success=False)
        check((self.project_dir/"one.txt").read_text()=="after one\n","stale edit wrote data")
        self.call("search_and_read",{"project":self.project_ref,"query":{"pattern":"after","path":"one.txt","pattern_mode":"literal"}},direct=True)
        # A generated 1x1 PNG is only a local binary fixture, never transferred as
        # model-authored Base64. Verify MCP image delivery bytes in the client.
        def chunk(kind,data):return struct.pack(">I",len(data))+kind+data+struct.pack(">I",zlib.crc32(kind+data)&0xffffffff)
        png=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",1,1,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(b"\0\xff\0\0"))+chunk(b"IEND",b"")
        (self.project_dir/"pixel.png").write_bytes(png)
        image=self.raw_tool("project_artifact",{"project":self.project_ref,"path":"pixel.png","action":"image"},self.owner,direct=True)
        images=[block for block in image.get("content",[]) if block.get("type")=="image"]
        check(len(images)==1 and images[0].get("mimeType")=="image/png" and base64.b64decode(images[0]["data"])==png,"MCP native image lost or rewritten")
        print("PASS: short Project/Session refs, batch reads/edits, numeric fence, stale rejection, compound search and native image")

    def interactions(self):
        program="import sys; from pathlib import Path; print('ready>', end='', flush=True)\nfor line in sys.stdin:\n with open('received.txt','a',encoding='utf-8') as f: f.write(line)\n print('echo:'+line,end='',flush=True)\nprint('eof',flush=True)"
        job=self.start_pipe(program,90);self.ready_prompt(job)
        first=self.write(job,"one","你好\n");check(first["state"]=="written","small write not acknowledged")
        check(self.write(job,"one","你好\n")==first,"same input receipt changed unexpectedly")
        conflict=self.write(job,"one","different\n",success=False)
        check(conflict.get("error_kind")=="job_input_conflict","changed id did not conflict")
        self.write(job,"wrong-project","forbidden",project="agent:input-runner:other",success=False)
        self.write(job,"foreign","forbidden",project=self.project,token=self.foreign,success=False)
        self.call("job_write_input",{"project":self.project_ref,"job_id":job,"input_id":"not-a-poll"},success=False)
        # Server restart must recover the same native process and receipt, not
        # replay its start or the input. The Runner is left continuously alive.
        pid=self.runner.pid;stop(self.server);self.start_server();self.wait_ready(self.project,self.owner);self.initialize(self.owner)
        check(self.runner.pid==pid and self.runner.poll() is None,"Runner changed in Server-only restart")
        deadline=time.monotonic()+20
        while True:
            item=self.observe(job)
            if item.get("status")!="recovering":break
            check(time.monotonic()<deadline,"original Job failed to reconcile")
            time.sleep(.1)
        check(self.write(job,"one","你好\n")==first,"same live Runner lost the input receipt")
        self.write(job,"two","second\n")
        closed=self.write(job,"eof",close=True);check(closed["state"]=="closed" and closed["stdin_closed"],"EOF not confirmed")
        terminal=self.terminal(job);check(terminal.get("exit_code")==0,"interactive child failed")
        check(self.write(job,"eof",close=True)==closed,"terminal receipt not retained")
        self.write(job,"after-close","again",success=False)
        check((self.project_dir/"received.txt").read_text()=="你好\nsecond\n","duplicate input, wrong target, or lost bytes")
        print("PASS: exact input/EOF, conflicting replay, Project/principal checks and Server restart preserve one process")

        blocked=self.start_pipe("import time; print('ready>',end='',flush=True); time.sleep(90)",90);self.ready_prompt(blocked)
        write=self.write(blocked,"blocked","x"*65536)
        check(write["state"] in ["pending","written"],"bounded write receipt missing")
        self.call("stop_job",{"project":self.project_ref,"job_id":blocked,"session_id":self.session_ref,"confirm":True})
        self.terminal(blocked)
        self.write(blocked,"new-after-stop","data",success=False)
        print("PASS: a non-reading process cannot block request completion; original Job stop terminates it")

        timed=self.start_pipe("import sys; print('ready>',end='',flush=True); sys.stdin.read()",2)
        timed_result=self.terminal(timed)
        check(timed_result.get("status")=="timeout",f"execution deadline result: {timed_result.get('status')} / {timed_result.get('command_execution_state')}")
        old=self.start_pipe("import sys; print('ready>',end='',flush=True); sys.stdin.read()",90);self.ready_prompt(old)
        stop(self.runner)
        self.runner=self.launch("webcodex-runner",["--config",str(self.runner_config)],self.env)
        self.wait_ready(self.project,self.owner)
        self.write(old,"after-replacement","do-not-retarget",success=False)
        print("PASS: execution timeout and Runner replacement never reinterpret an old input target")

    def run(self):
        self.prepare();self.workflow();self.interactions()
        print("PASS: all services and files were disposable; no paid endpoints, configuration edits or business retries")


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--bin-dir",type=Path,default=ROOT/"target/dogfood")
    args=parser.parse_args()
    for binary in ["webcodex-server","webcodex-runner"]:check((args.bin_dir/binary).is_file(),"build dogfood binaries first")
    with tempfile.TemporaryDirectory(prefix="webcodex-job-input-") as directory:
        smoke=InteractiveSmoke(args.bin_dir.resolve(),Path(directory).resolve())
        try:smoke.run()
        finally:smoke.close()

if __name__=="__main__":main()
