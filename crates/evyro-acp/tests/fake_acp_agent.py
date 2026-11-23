#!/usr/bin/env python3
"""A stand-in ACP agent for Evyro's tests. It speaks ACP v1 over stdio and does
what a real coding agent does in a turn, without a model:

- streams a plan, message chunks and tool calls (session/update);
- calls the Evyro MCP server it was given, to read the active task;
- reads a file through the client (fs/read_text_file);
- if the prompt says "write", asks permission and, when allowed, writes
  notes.txt through the client (fs/write_text_file);
- tries to read .env and reports whether the client refused;
- if the prompt says "slow", waits for session/cancel and stops as cancelled.

Every message from the client is checked against the parts of the ACP v1
schema this agent relies on; a violation ends the process with exit code 3.
"""
import json
import os
import subprocess
import sys

LOG = os.environ.get("FAKE_ACP_LOG")
state = {"next": 1000, "cancelled": False, "session": None, "cwd": None, "mcp": []}


def log(msg):
    if LOG:
        with open(LOG, "a") as f:
            f.write(msg + "\n")


def die(why):
    log("PROTOCOL ERROR: " + why)
    sys.stderr.write("fake-acp: protocol error: " + why + "\n")
    sys.exit(3)


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    if os.environ.get("FAKE_ACP_RAW"):
        with open(os.environ["FAKE_ACP_RAW"], "a") as f:
            f.write(line if line.endswith("\n") else line + "\n")
    msg = json.loads(line)
    if msg.get("jsonrpc") != "2.0":
        die("missing jsonrpc 2.0")
    return msg


def handle_notification(msg):
    if msg.get("method") == "session/cancel":
        if msg["params"].get("sessionId") != state["session"]:
            die("cancel for unknown session")
        state["cancelled"] = True
        log("cancel received")


def call(method, params):
    """Call the client and wait for the answer, handling notifications meanwhile."""
    state["next"] += 1
    rid = state["next"]
    send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
    while True:
        msg = read()
        if "method" in msg and "id" not in msg:
            handle_notification(msg)
            continue
        if msg.get("id") == rid:
            return msg
        die("unexpected message while waiting for %s: %s" % (method, msg))


def update(u):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": state["session"], "update": u}})


def say(text):
    update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})


def ask_evyro_mcp():
    """Start the Evyro MCP server given in session/new and read the active task."""
    for s in state["mcp"]:
        if s["name"] != "evyro":
            continue
        env = dict(os.environ)
        for e in s.get("env", []):
            env[e["name"]] = e["value"]
        p = subprocess.Popen([s["command"]] + s.get("args", []), stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, env=env, cwd=state["cwd"], text=True)
        msgs = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "fake-acp", "version": "1"}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "get_active_task", "arguments": {}}},
        ]
        out, _ = p.communicate("".join(json.dumps(m) + "\n" for m in msgs), timeout=30)
        for line in out.splitlines():
            r = json.loads(line)
            if r.get("id") == 2:
                res = r["result"]["structuredContent"]["result"]
                task = (res or {}).get("task") or {}
                return task.get("title") or "(none)"
    return None


def prompt_turn(msg):
    p = msg["params"]
    if p.get("sessionId") != state["session"]:
        die("prompt for unknown session")
    blocks = p.get("prompt")
    if not isinstance(blocks, list) or not blocks or blocks[0].get("type") != "text":
        die("prompt must be a list of content blocks starting with text")
    text = blocks[0]["text"]
    log("prompt: " + text.splitlines()[0][:120] if text else "prompt: (empty)")
    state["cancelled"] = False

    update({"sessionUpdate": "plan", "entries": [
        {"content": "Read the task", "priority": "high", "status": "in_progress"},
        {"content": "Make the change", "priority": "medium", "status": "pending"},
    ]})
    title = ask_evyro_mcp()
    if title is not None:
        say("Evyro says the task is: %s\n" % title)

    if "slow" in text:
        say("Working slowly...\n")
        while not state["cancelled"]:
            m = read()
            if "method" in m and "id" not in m:
                handle_notification(m)
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"stopReason": "cancelled"}})
        return

    update({"sessionUpdate": "tool_call", "toolCallId": "call_read", "title": "Read README.md", "kind": "read", "status": "pending",
            "locations": [{"path": os.path.join(state["cwd"], "README.md")}]})
    r = call("fs/read_text_file", {"sessionId": state["session"], "path": os.path.join(state["cwd"], "README.md"), "line": 1, "limit": 1})
    first = (r.get("result") or {}).get("content", "").strip() if "result" in r else "(error)"
    update({"sessionUpdate": "tool_call_update", "toolCallId": "call_read", "status": "completed", "title": "Read README.md"})
    say("README starts with: %s\n" % first)

    if "write" in text:
        update({"sessionUpdate": "tool_call", "toolCallId": "call_write", "title": "Write notes.txt", "kind": "edit", "status": "pending",
                "locations": [{"path": os.path.join(state["cwd"], "notes.txt")}]})
        r = call("session/request_permission", {
            "sessionId": state["session"],
            "toolCall": {"toolCallId": "call_write", "title": "Write notes.txt", "kind": "edit", "status": "pending"},
            "options": [
                {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                {"optionId": "always", "name": "Always allow", "kind": "allow_always"},
                {"optionId": "no", "name": "Reject", "kind": "reject_once"},
            ],
        })
        outcome = (r.get("result") or {}).get("outcome") or {}
        if outcome.get("outcome") not in ("selected", "cancelled"):
            die("bad permission outcome: %s" % r)
        if outcome.get("outcome") == "selected" and outcome.get("optionId") in ("yes", "always"):
            w = call("fs/write_text_file", {"sessionId": state["session"], "path": os.path.join(state["cwd"], "notes.txt"), "content": "written by the fake agent\n"})
            ok = "error" not in w
            update({"sessionUpdate": "tool_call_update", "toolCallId": "call_write", "status": "completed" if ok else "failed", "title": "Write notes.txt"})
            say("Wrote notes.txt.\n" if ok else "Could not write notes.txt.\n")
        else:
            update({"sessionUpdate": "tool_call_update", "toolCallId": "call_write", "status": "failed"})
            say("Permission denied, notes.txt not written.\n")

    r = call("fs/read_text_file", {"sessionId": state["session"], "path": os.path.join(state["cwd"], ".env")})
    say("Reading .env was %s.\n" % ("refused" if "error" in r else "ALLOWED"))
    r = call("fs/read_text_file", {"sessionId": state["session"], "path": os.path.join(state["cwd"], "..", "outside.txt")})
    say("Reading outside the project was %s.\n" % ("refused" if "error" in r else "ALLOWED"))

    update({"sessionUpdate": "plan", "entries": [
        {"content": "Read the task", "priority": "high", "status": "completed"},
        {"content": "Make the change", "priority": "medium", "status": "completed"},
    ]})
    say("Done: %s" % text.splitlines()[-1][:80])
    send({"jsonrpc": "2.0", "id": msg["id"], "result": {"stopReason": "end_turn"}})


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--version":
        print("fake-acp 1.0")
        return
    while True:
        msg = read()
        m = msg.get("method")
        if m is None:
            die("unexpected response %s" % msg)
        if "id" not in msg:
            handle_notification(msg)
            continue
        p = msg.get("params") or {}
        if m == "initialize":
            if p.get("protocolVersion") != 1:
                die("protocolVersion must be 1")
            fs = (p.get("clientCapabilities") or {}).get("fs") or {}
            if not isinstance(fs.get("readTextFile"), bool) or not isinstance(fs.get("writeTextFile"), bool):
                die("clientCapabilities.fs must say readTextFile/writeTextFile")
            log("initialize from %s" % (p.get("clientInfo") or {}).get("name"))
            send({"jsonrpc": "2.0", "id": msg["id"], "result": {
                "protocolVersion": 1,
                "agentCapabilities": {"loadSession": False, "promptCapabilities": {"image": False, "audio": False, "embeddedContext": False}},
                "agentInfo": {"name": "fake-acp", "title": "Fake ACP agent", "version": "1.0"},
                "authMethods": [],
            }})
        elif m == "session/new":
            if not os.path.isabs(p.get("cwd", "")):
                die("cwd must be absolute")
            servers = p.get("mcpServers")
            if not isinstance(servers, list):
                die("mcpServers must be a list")
            for s in servers:
                if not os.path.isabs(s.get("command", "")) or not isinstance(s.get("args"), list) or not isinstance(s.get("env"), list):
                    die("stdio MCP server needs an absolute command, args and env lists")
                for e in s["env"]:
                    if set(e) != {"name", "value"}:
                        die("env entries are {name, value}")
            state.update(session="sess_fake_1", cwd=p["cwd"], mcp=servers)
            log("session/new in %s with mcp %s" % (p["cwd"], [s["name"] for s in servers]))
            send({"jsonrpc": "2.0", "id": msg["id"], "result": {"sessionId": state["session"]}})
        elif m == "session/prompt":
            prompt_turn(msg)
        else:
            send({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32601, "message": "method not found"}})


if __name__ == "__main__":
    main()
