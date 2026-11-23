// Desktop UI smoke test.
//
// Loads the real ui/ in headless Chromium with a stand-in for the Tauri
// backend, clicks through every screen and the main dialogs, and fails on any
// uncaught script error. Lists of targets, agents, handoffs, sessions and
// decisions come from the real `evyro` CLI (--json) run against a scratch
// project, so their shapes match the core types; the few desktop-only shapes
// are written out below.
//
//   cargo build -p evyro-cli && node crates/evyro-desktop/tests/ui-smoke.cjs
//
// Needs Node 18+ and Playwright (`npm i -g playwright && npx playwright install chromium`
// if it is not already installed).

"use strict";
const { chromium } = require("playwright");
const { execFileSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const ROOT = path.resolve(__dirname, "../../..");
const UI = path.join(ROOT, "crates/evyro-desktop/ui/index.html");
const EXE = process.env.EVYRO_BIN || path.join(ROOT, "target/debug", process.platform === "win32" ? "evyro.exe" : "evyro");

// ---------- scratch project, driven by the real CLI ----------
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "evyro-ui-"));
const repo = path.join(tmp, "my-app");
fs.mkdirSync(repo);
const env = { ...process.env, EVYRO_HOME: path.join(tmp, "home"), EVYRO_NO_APP_LOOKUP: "1" };
const git = (...a) => execFileSync("git", a, { cwd: repo, env, stdio: "pipe" });
git("init", "-q", "-b", "main");
git("config", "user.email", "t@example.com");
git("config", "user.name", "t");
fs.writeFileSync(path.join(repo, "app.txt"), "one\n");
fs.mkdirSync(path.join(repo, "src"));
const LIMITER = "pub struct Limiter {\n    per_minute: u32,\n}\n\nimpl Limiter {\n    pub fn allow(&mut self, key: &str) -> bool {\n        let _ = key;\n        true\n    }\n}\n";
fs.writeFileSync(path.join(repo, "src/limiter.rs"), LIMITER);
git("add", "-A");
git("commit", "-qm", "init");
const ev = (...a) => execFileSync(EXE, a, { cwd: repo, env, stdio: "pipe" }).toString();
const evj = (...a) => JSON.parse(ev("--json", ...a));
ev("init");
ev("task", "add", "Add rate limiting", "--accept", "429 after 100 a minute", "--ready");
ev("task", "start", "T-1", "--agent", "claude-code");
ev("checkpoint", "--done", "limiter", "--next", "write the test");
ev("handoff", "--to", "codex", "--task", "T-1");
ev("decide", "Store", "Token bucket in memory");
fs.appendFileSync(path.join(repo, "app.txt"), "two\n");

const project = evyro_project();
function evyro_project() {
  const ps = evyj("projects");
  return ps[0].project;
}
function evyj(...a) { return evj(...a); }

const task = {
  id: "tsk_1", key: "T-1", title: "Add rate limiting", status: "in_progress", kind: "task", priority: 2,
  acceptance: ["429 after 100 a minute"], description: "", constraints: [], files: [], created_at: new Date().toISOString(),
};
const FIX = {
  app_info: { product: "Evyro", version: "0.1.0", data_dir: env.EVYRO_HOME, schema_version: 2, git: "git" },
  list_projects: [project],
  overview: {
    project, active_task: task, owner: { id: "ses_1", harness: "claude-code" }, git: {
      branch: "main", head: "abc123def456", dirty: true, files: [{ path: "app.txt", status: " M" }],
    },
    health: { task_counts: { in_progress: 1 }, findings: [] }, pending_handoff: evj("handoffs")[0],
    active_sessions: evj("sessions"), events: [
      { kind: "checkpoint.created", payload: { trigger: "manual" }, created_at: new Date().toISOString() },
      { kind: "agent.handed_to_app", payload: { target: "chatgpt", opened: true }, created_at: new Date().toISOString() },
    ], latest_checkpoint: { created_at: new Date().toISOString(), trigger: "manual", next_action: "write the test" }, recovered: [],
  },
  list_tasks: [{ task, criteria_met: 0, criteria_total: 1, owner: "claude-code", blocked_by: [] }],
  repo_changes: { files: [{ path: "app.txt", status: " M" }, { path: "new.rs", status: "??" }],
    diff: "diff --git a/app.txt b/app.txt\nindex 1..2 100644\n--- a/app.txt\n+++ b/app.txt\n@@ -1 +1,2 @@\n one\n+two\n", redactions: 0, truncated: false },
  list_handoffs: evj("handoffs"),
  list_sessions: evj("sessions"),
  list_decisions: evj("decisions"),
  search_memory: [],
  memory_categories: ["fact", "note"],
  list_targets: evj("targets"),
  detect_agents: evj("agents"),
  target_options: {
    harnesses: [{ id: "claude-code", display_name: "Claude Code", providers: ["anthropic"], app_login: true }, { id: "opencode", display_name: "OpenCode", providers: ["openrouter"], app_login: false }],
    providers: [{ id: "openrouter", display_name: "OpenRouter", key_env: "OPENROUTER_API_KEY", local: false, example_model: "anthropic/claude-sonnet-4.5" }, { id: "ollama", display_name: "Ollama", key_env: null, local: true, example_model: "qwen2.5-coder" }],
    ides: [{ id: "cursor-ide", display_name: "Cursor" }], chat_apps: [{ id: "chatgpt", display_name: "ChatGPT" }],
  },
  run_doctor: [{ name: "evyro", status: "ok", detail: "0.1.0" }],
  // Code search results come from the real CLI on the scratch repo.
  find_files: evj("search", "files", "lim"),
  search_text: evj("search", "text", "allow"),
  find_symbols: evj("search", "symbols", "allow"),
  // Skills come from the real CLI too.
  skills_list: evj("skills", "list"),
  view_file: { path: "src/limiter.rs", text: LIMITER, lines: LIMITER.split("\n").length - 1, redactions: 0, symbols: evj("search", "outline", "src/limiter.rs") },
};

// ---------- browser ----------
(async () => {
  const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
  const page = await browser.newPage({ viewport: { width: 1300, height: 860 } });
  const errors = [];
  const calls = [];
  page.on("pageerror", (e) => errors.push("page error: " + e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push("console: " + m.text()); });
  await page.exposeFunction("__mockLog", (c) => calls.push(c));
  await page.addInitScript((fix) => {
    const listeners = {};
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          window.__mockLog(cmd);
          if (cmd in fix) return JSON.parse(JSON.stringify(fix[cmd]));
          if (cmd === "add_target") return args.target;
          // Skills: the real list, with on/off kept in memory.
          const sk = (window.__skills = window.__skills || fix.skills_list);
          if (cmd === "list_skills") return { skills: JSON.parse(JSON.stringify(sk)), project_dir: "/repo/.evyro/skills", global_dir: "/home/.config/evyro/skills",
            sync_targets: [{ harness: "claude-code", file: ".claude/skills/<name>/SKILL.md" }, { harness: "codex", file: "AGENTS.md" }] };
          if (cmd === "skill_text") { const k = sk.find((x) => x.name === args.name); return { skill: k, text: "# " + k.name + "\n\nSteps for " + k.name + "." }; }
          if (cmd === "set_skill_enabled") { const k = sk.find((x) => x.name === args.name); k.enabled = args.enabled; k.enabled_by = args.global ? "global" : "project"; return k; }
          if (cmd === "sync_skills") return { harness: args.harness, written: [], unchanged: [], removed: [], skipped: [{ name: "AGENTS.md", reason: "AGENTS.md has instructions someone wrote and no Evyro section, so it was not changed" }] };
          // Updates: a newer version is published.
          if (cmd === "check_update") return { configured: true, available: true, current: "0.1.0", version: "0.2.0", notes: "Tray icon and auto-update.", date: null };
          if (cmd === "install_update") { window.__installed = true; return null; }
          // Remote access, kept in memory.
          const rm = (window.__rm = window.__rm || { running: false, devices: [] });
          if (cmd === "remote_status") return rm.running ? { running: true, url: "http://" + rm.host + ":" + rm.port + "/", reach: "loopback" } : { running: false };
          if (cmd === "remote_start") { Object.assign(rm, { running: true, host: args.host, port: args.port }); return { running: true }; }
          if (cmd === "remote_stop") { rm.running = false; return null; }
          if (cmd === "remote_devices") return JSON.parse(JSON.stringify(rm.devices));
          if (cmd === "remote_pair") {
            const device = { id: "dev_0" + (rm.devices.length + 1) + "abcdef", name: args.name, scope: args.scope, state: "waiting", created_at: new Date().toISOString(), last_seen_at: null, last_address: null };
            rm.devices.push(device); return { device, code: "K7QM-4XPZ", expires_at: new Date(Date.now() + 6e5).toISOString() };
          }
          if (cmd === "remote_revoke") { const d = rm.devices.find((d) => d.id === args.id); d.state = "revoked"; return d; }
          // Review comments, kept in memory.
          const rv = (window.__rv = window.__rv || []);
          if (cmd === "list_review_comments") return rv.filter((c) => args.all || c.status !== "resolved");
          if (cmd === "add_review_comment") {
            const c = { id: "rvc_" + (rv.length + 1) + "0000000", project_id: args.projectId, task_id: null, path: args.path, line: args.line, snippet: args.snippet || "",
              body: args.body, status: "open", author: "human", commit_sha: null, created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
            rv.push(c); return c;
          }
          if (cmd === "set_review_status") { const c = rv.find((c) => c.id === args.id); c.status = args.status; return c; }
          if (cmd === "delete_review_comment") { window.__rv = rv.filter((c) => c.id !== args.id); return null; }
          if (cmd === "review_prompt_text") {
            const open = rv.filter((c) => c.status === "open");
            const prompt = open.map((c, i) => `${i + 1}. ${c.path}${c.line ? ":" + c.line : ""}\n   ${c.body}`).join("\n");
            if (args.markSent) open.forEach((c) => (c.status = "sent"));
            return { prompt, count: open.length };
          }
          // Terminals: a stand-in that echoes input back, like a PTY would.
          const terms = (window.__terms = window.__terms || []);
          if (cmd === "term_list") return JSON.parse(JSON.stringify(terms));
          if (cmd === "term_open") {
            const t = { id: "term_" + (terms.length + 1), title: args.kind === "agent" ? `${args.target} on ${args.task}` : "Shell", kind: args.kind,
              target: args.target, task: args.task, project_id: args.projectId, cwd: "/repo", command: "evyro run", started_at: Math.floor(Date.now() / 1000), running: true, exit_code: null };
            terms.push(t);
            setTimeout(() => window.__emit("term-output", { id: t.id, data: "\x1b[2m[evyro] launching Claude Code for target 'claude-code' (session ses_01a2)\x1b[0m\r\n\r\n\x1b[1;35m*\x1b[0m Reading .evyro/CURRENT_TASK.md\r\n  T-1 Add rate limiting. Next: write the test\r\n\x1b[1;35m*\x1b[0m Writing tests/limiter.rs\r\n\x1b[32m✓\x1b[0m evyro verify -- cargo test (12 passed)\r\nagent ready\r\n", end: 1 }), 50);
            return t;
          }
          if (cmd === "term_snapshot") return { data: "", end: 0 };
          if (cmd === "term_resize") return null;
          if (cmd === "term_write") window.__lastTermWrite = (window.__lastTermWrite || "") + args.data;
          if (cmd === "term_write") { setTimeout(() => window.__emit("term-output", { id: args.id, data: args.data, end: 0 }), 10); return null; }
          if (cmd === "term_close") { window.__terms = terms.filter((t) => t.id !== args.id); return null; }
          // Chat threads (ACP): a stand-in agent that plans, asks permission, edits and answers.
          const chats = (window.__chats = window.__chats || {});
          const say = (id, item) => { chats[id].log.push(item); window.__emit("chat-event", { id, item }); };
          const upd = (id, update) => say(id, { t: "update", update });
          const setc = (id, patch) => { Object.assign(chats[id].info, patch); window.__emit("chat-state", JSON.parse(JSON.stringify(chats[id].info))); };
          if (cmd === "acp_agents") return [{ harness: "codex", display_name: "Codex CLI (Codex ACP adapter)", command: "/usr/local/bin/codex-acp", install: "npm install -g @agentclientprotocol/codex-acp", evidence: "" },
            { harness: "gemini", display_name: "Gemini CLI", command: null, install: "npm install -g @google/gemini-cli", evidence: "" }];
          if (cmd === "chat_list") return Object.values(chats).map((c) => JSON.parse(JSON.stringify(c.info)));
          if (cmd === "chat_log") return JSON.parse(JSON.stringify({ info: chats[args.id].info, log: chats[args.id].log }));
          if (cmd === "chat_open") {
            const id = "chat_" + (Object.keys(chats).length + 1);
            const info = { id, kind: "chat", title: `${args.target} on ${args.task}`, target: args.target, task: args.task, project_id: args.projectId, started_at: Math.floor(Date.now() / 1000), running: true, busy: true, state: "starting", exit_code: null, command: "ACP chat" };
            chats[id] = { info, log: [] };
            setTimeout(() => {
              setc(id, { state: "ready", agent: "Codex", session: "ses_01chat", cwd: "/repo" });
              say(id, { t: "user", text: { context: "Work on task T-1 \"Add rate limiting\"..." } });
              upd(id, { sessionUpdate: "plan", entries: [{ content: "Read the limiter", status: "completed" }, { content: "Add a test for 429", status: "in_progress" }, { content: "Run the tests", status: "pending" }] });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "I read the task through Evyro. " } });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Next I add a test for the 429 path." } });
              upd(id, { sessionUpdate: "tool_call", toolCallId: "c1", title: "Read src/limiter.rs", kind: "read", status: "completed", locations: [{ path: "/repo/src/limiter.rs" }] });
              upd(id, { sessionUpdate: "tool_call", toolCallId: "c2", title: "Edit tests/limiter.rs", kind: "edit", status: "pending",
                content: [{ type: "diff", path: "/repo/tests/limiter.rs", oldText: "#[test]\nfn allows() {}\n", newText: "#[test]\nfn allows() {}\n#[test]\nfn blocks_after_100() {}\n" }] });
              say(id, { t: "permission", request: 7, params: { sessionId: "s", toolCall: { toolCallId: "c2", title: "Edit tests/limiter.rs", kind: "edit" },
                options: [{ optionId: "allow", name: "Allow", kind: "allow_once" }, { optionId: "always", name: "Always allow", kind: "allow_always" }, { optionId: "reject", name: "Reject", kind: "reject_once" }] } });
            }, 60);
            return JSON.parse(JSON.stringify(info));
          }
          if (cmd === "chat_permission") {
            const id = args.id;
            setTimeout(() => {
              say(id, { t: "permission_answer", request: args.request, option: args.optionId });
              upd(id, { sessionUpdate: "tool_call_update", toolCallId: "c2", status: "completed" });
              say(id, { t: "file", op: "write", path: "tests/limiter.rs", bytes: 64 });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Added blocks_after_100." } });
              say(id, { t: "turn_end", reason: "end_turn" });
              setc(id, { busy: false });
            }, 30);
            return null;
          }
          if (cmd === "chat_send") {
            const id = args.id;
            setTimeout(() => {
              say(id, { t: "user", text: args.text });
              setc(id, { busy: true });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "You said: " + args.text } });
              say(id, { t: "turn_end", reason: "end_turn" });
              setc(id, { busy: false });
            }, 30);
            return null;
          }
          if (cmd === "chat_cancel") return null;
          if (cmd === "chat_close") { delete chats[args.id]; return null; }
          throw { code: "not_mocked", message: `no fixture for ${cmd}` };
        },
      },
      event: { listen: async (name, cb) => { listeners[name] = cb; return () => {}; } },
    };
    window.__emit = (name, payload) => listeners[name] && listeners[name]({ payload });
  }, FIX);
  await page.goto(require("url").pathToFileURL(UI).href);

  const step = async (name, fn) => {
    try { await fn(); console.log("PASS  " + name); }
    catch (e) { errors.push(`${name}: ${e.message.split("\n").slice(0, process.env.SMOKE_DEBUG ? 4 : 1).join(" / ")}`); console.log("FAIL  " + name); }
  };
  const tab = (v) => page.click(`.tab[data-view="${v}"]`);

  await step("overview shows the active task", async () => {
    await page.waitForSelector("text=Add rate limiting");
    await page.waitForSelector("text=Handoff waiting");
  });
  await step("tasks board", async () => { await tab("tasks"); await page.waitForSelector(".task-card"); });
  await step("changes view renders the diff", async () => {
    await tab("changes");
    await page.waitForSelector(".diff .add .code");
    if ((await page.textContent(".diff .add .code")).trim() !== "+two") throw new Error("diff line");
    // The added line is line 2 of the new file.
    if ((await page.textContent(".diff .add .ln.can")).trim() !== "2") throw new Error("line number");
  });
  await step("handoffs list", async () => { await tab("handoffs"); await page.waitForSelector("table tbody tr"); });
  await step("sessions", async () => { await tab("sessions"); await page.waitForTimeout(200); });
  await step("decisions and memory", async () => { await tab("knowledge"); await page.waitForSelector("text=Store"); });
  await step("AI targets grouped by kind", async () => {
    await tab("agents");
    await page.waitForSelector("text=Coding agent");
    await page.waitForSelector("text=Chat app");
  });
  await step("add-target form shows key fields for an API key", async () => {
    await page.click("text=+ Add target");
    await page.selectOption(".modal select >> nth=0", "api");
    if (!(await page.isVisible("text=API key from"))) throw new Error("key field hidden");
    await page.fill(".modal input >> nth=0", "or-test");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".modal", { state: "detached" });
  });
  await step("agent thread runs in the window", async () => {
    await tab("threads");
    await page.waitForSelector("text=Work with an AI right here");
    await page.click("text=New agent thread");
    await page.waitForSelector(".modal select");
    await page.selectOption(".modal select >> nth=1", "T-1");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".thread-item");
    await page.waitForFunction(() => document.querySelector(".xterm-rows")?.textContent.includes("agent ready"));
    await page.click(".xterm");
    await page.keyboard.type("hi");
    await page.waitForFunction(() => document.querySelector(".xterm-rows")?.textContent.includes("agent readyhi") || document.querySelector(".xterm-rows")?.textContent.includes("hi"));
    await page.evaluate(() => { window.__terms[0].running = false; window.__terms[0].exit_code = 0; window.__emit("term-exit", { id: "term_1", code: 0 }); });
    await page.waitForSelector(".thread-item >> text=exit 0");
    await tab("overview");
    await tab("threads");
    await page.waitForFunction(() => document.querySelector(".xterm-rows")?.textContent.includes("agent ready"));
  });
  await step("review comments: comment on a line and send to a running agent", async () => {
    await page.evaluate((pid) => window.__terms.push({ id: "term_9", title: "codex on T-1", kind: "agent", target: "codex", task: "T-1",
      project_id: pid, cwd: "/repo", command: "evyro run", started_at: 1, running: true, exit_code: null }), project.id);
    await tab("changes");
    await page.waitForSelector(".diff .add .ln.can");
    await page.click(".diff .add .ln.can");
    await page.fill(".rv-form textarea", "Read the limit from config");
    await page.click(".rv-form .btn.primary");
    await page.waitForSelector(".rv-comment >> text=Read the limit from config");
    await page.waitForSelector("text=1 open comment");
    await page.click("text=Send comments to agent");
    await page.waitForSelector(".rv-preview >> text=app.txt:2");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".modal", { state: "detached" });
    await page.waitForFunction(() => window.__lastTermWrite && window.__lastTermWrite.includes("Read the limit from config"));
    await page.waitForSelector(".rv-comment.sent");
    await page.click(".rv-comment >> text=Resolve");
    await page.waitForSelector(".rv-comment", { state: "detached" });
  });
  await step("chat thread over ACP: plan, permission, diff, reply", async () => {
    await tab("threads");
    await page.click("text=+ Chat");
    await page.waitForSelector(".modal >> text=Ready:");
    await page.selectOption(".modal select >> nth=1", "T-1");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".chat-perm.open >> text=Edit tests/limiter.rs");
    await page.waitForSelector(".chat-plan >> text=Add a test for 429");
    await page.waitForSelector(".chat-diff >> text=+ fn blocks_after_100() {}");
    await page.waitForSelector(".thread-item >> text=needs you");
    await page.click(".chat-perm.open >> text=Allow");
    await page.waitForSelector(".chat-perm >> text=You chose: Allow");
    await page.waitForSelector(".chat-tool.completed >> text=Edit tests/limiter.rs");
    await page.waitForSelector(".chat-note >> text=Wrote tests/limiter.rs");
    await page.waitForSelector(".thread-item >> text=your turn");
    await page.fill("#chat-input", "thanks");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".chat-msg.agent >> text=You said: thanks");
    await tab("overview");
    await tab("threads");
    await page.click(".thread-item >> text=chat");
    await page.waitForSelector(".chat-msg.agent >> text=You said: thanks");
    await page.click("text=End chat");
    await page.click(".modal >> text=Stop it");
    await page.waitForFunction(() => !document.querySelector(".chat"));
  });
  await step("skills: read one, turn it off, sync to an agent", async () => {
    await tab("skills");
    await page.waitForSelector(".skill-row >> text=debugging");
    if ((await page.$$(".skill-row")).length !== 15) throw new Error("expected 15 skills");
    await page.click(".skill-row >> text=debugging");
    await page.waitForSelector(".modal pre.skill-text >> text=Steps for debugging");
    await page.click(".modal .actions .btn");
    const row = page.locator(".skill-row", { hasText: "test-driven-development" });
    await row.locator("button", { hasText: "Turn off" }).click();
    await page.click(".modal .actions .btn.primary");
    await page.waitForSelector(".skill-row:has-text('test-driven-development') .pill:text-is('off')");
    await page.fill(".card input[type=text]", "release");
    await page.waitForFunction(() => document.querySelectorAll(".skill-row").length < 15);
    await page.click("text=Add to agent files…");
    await page.selectOption(".modal select", "codex");
    await page.click(".modal .actions .btn.primary");
    await page.waitForSelector(".modal >> text=Skipped AGENTS.md");
    await page.click(".modal .actions .btn");
  });
  await step("settings", async () => { await tab("settings"); await page.waitForSelector("text=Doctor"); });
  await step("updates: a newer version is offered and installs on request", async () => {
    await page.waitForSelector(".card >> text=Evyro 0.2.0 is available");
    await page.waitForSelector("#side-foot .update-btn");
    await page.click(".card >> text=Install and restart");
    await page.waitForSelector(".modal >> text=Tray icon and auto-update.");
    await page.click(".modal .actions .btn.primary");
    await page.waitForFunction(() => window.__installed === true);
  });
  await step("remote access: start serving, pair a device, revoke it", async () => {
    await page.waitForSelector("h2:text-is('Remote access')");
    await page.click("text=Start serving");
    await page.waitForSelector(".card >> text=http://127.0.0.1:7420/");
    await page.click("text=+ Pair a device");
    await page.fill(".modal input[type=text]", "My phone");
    await page.click(".modal .actions .btn.primary");
    await page.waitForSelector(".modal .pair-code >> text=K7QM-4XPZ");
    await page.click(".modal .actions .btn");
    await page.waitForSelector(".list li:has-text('My phone') .pill:text-is('waiting')");
    await page.click(".list li:has-text('My phone') >> text=Revoke");
    await page.waitForSelector(".list li:has-text('My phone') .pill:text-is('revoked')");
    await page.click(".card >> text=Stop");
    await page.waitForSelector("text=Start serving");
  });
  // Accessibility: axe-core (WCAG 2 A and AA rules) on every screen. Serious
  // and critical problems fail the run. Runs when axe-core is installed.
  let axePath = null;
  try { axePath = require.resolve("axe-core/axe.min.js", { paths: [process.cwd(), ...(process.env.AXE_DIR ? [process.env.AXE_DIR] : [])] }); } catch {}
  if (axePath) {
    await step("accessibility: no serious WCAG problems on any screen, light and dark", async () => {
      const found = [];
      for (const scheme of ["light", "dark"]) {
      await page.emulateMedia({ colorScheme: scheme });
      for (const v of ["overview", "tasks", "changes", "threads", "handoffs", "sessions", "knowledge", "skills", "agents", "settings"]) {
        await tab(v);
        await page.waitForTimeout(250);
        if (!(await page.evaluate(() => !!window.axe))) await page.addScriptTag({ path: axePath });
        const res = await page.evaluate(async () => {
          const r = await window.axe.run(document, { preload: false, runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] } });
          return r.violations.map((x) => ({ id: x.id, impact: x.impact, help: x.help, nodes: x.nodes.slice(0, 3).map((n) => { const d = n.any[0] && n.any[0].data; return n.target.join(" ") + (d && d.fgColor ? ` ${d.fgColor} on ${d.bgColor} = ${d.contrastRatio}` : ""); }) }));
        });
        for (const x of res) found.push({ screen: `${v}, ${scheme}`, ...x });
      }
      }
      await page.emulateMedia({ colorScheme: "light" });
      for (const x of found) console.log(`  a11y ${x.impact}: [${x.screen}] ${x.id}: ${x.help} (${x.nodes.join(", ")})`);
      const bad = found.filter((x) => x.impact === "serious" || x.impact === "critical");
      if (bad.length) throw new Error(`${bad.length} serious or critical accessibility problems`);
    });
  } else {
    console.log("SKIP  accessibility (axe-core not installed)");
  }
  // Keyboard only: on every screen, everything that reacts to a click can be
  // reached with Tab, and whatever Tab reaches shows where the focus is.
  await step("keyboard only: every control reachable with Tab and visibly focused", async () => {
    const found = [];
    for (const v of ["overview", "tasks", "changes", "threads", "handoffs", "sessions", "knowledge", "skills", "agents", "settings"]) {
      await tab(v);
      await page.waitForTimeout(250);
      // Things that look clickable (pointer cursor) but are not in the Tab order.
      const unreachable = await page.evaluate(() => {
        const out = [];
        for (const el of document.querySelectorAll("body *")) {
          const cs = getComputedStyle(el);
          if (cs.cursor !== "pointer" || el.closest("[hidden], [aria-hidden=true]") || !el.getClientRects().length) continue;
          if (el.parentElement && getComputedStyle(el.parentElement).cursor === "pointer") continue; // the parent is the control
          if (el.tabIndex >= 0 || el.closest("label, summary, a[href], button")) continue;
          if (el.matches(".xterm *, .xterm")) continue; // the terminal takes keys itself
          if (el.matches(".diff .ln.can")) continue; // each line's "+" button does the same
          out.push(el.tagName.toLowerCase() + (el.className && typeof el.className === "string" ? "." + el.className.trim().split(/\s+/).join(".") : ""));
        }
        return [...new Set(out)];
      });
      for (const u of unreachable) found.push(`[${v}] not reachable with Tab: ${u}`);
      // Walk the Tab order and check each stop shows a focus indicator.
      // Start each walk at the top of the page, wherever focus was left.
      await page.evaluate(() => {
        const start = document.createElement("span");
        start.tabIndex = -1;
        document.body.prepend(start);
        start.focus();
        start.remove();
      });
      const seen = new Set();
      for (let i = 0; i < 80; i++) {
        await page.keyboard.press("Tab");
        const r = await page.evaluate((walk) => {
          const el = document.activeElement;
          if (!el || el === document.body) return null;
          // Back at an element this walk already passed: the Tab order wrapped.
          if (el.dataset.kbdWalk === walk) return { again: true };
          el.dataset.kbdWalk = walk;
          if (el.closest(".xterm")) return { key: "xterm", ok: true };
          const id = el.tagName.toLowerCase() + (el.className && typeof el.className === "string" ? "." + el.className.trim().split(/\s+/).join(".") : "") + `#len:${(el.textContent || "").trim().length}`; // no page text in CI logs
          const cs = getComputedStyle(el);
          const ok = (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0) || (cs.boxShadow && cs.boxShadow !== "none");
          return { key: id, ok };
        }, `${v}-${Date.now()}`);
        if (!r || r.again) break;
        seen.add(r.key);
        if (!r.ok) found.push(`[${v}] no visible focus: ${r.key}`);
      }
      if (process.env.SMOKE_DEBUG) console.log(`  kbd [${v}] ${seen.size} Tab stops`);
      await page.keyboard.press("Escape");
    }
    // The terminal keeps Tab for the program in it; F6 leaves it.
    await tab("threads");
    await page.click(".term-host");
    await page.keyboard.press("F6");
    const out = await page.evaluate(() => !document.activeElement.closest(".xterm") && document.activeElement.matches(".thread-head button"));
    if (!out) found.push("[threads] F6 does not leave the terminal");
    // A row opened from the keyboard alone.
    await tab("agents");
    await page.focus("tr.clickable");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.modal[aria-label^="Target "]');
    await page.keyboard.press("Escape");
    await page.waitForSelector(".modal", { state: "detached" });
    for (const f of found) console.log("  kbd " + f);
    if (found.length) throw new Error(`${found.length} keyboard problems`);
  });
  await step("command palette filters and runs", async () => {
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".palette input");
    await page.keyboard.type("go to tasks");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.tab.active[data-view="tasks"]');
  });
  await step("code search: find a file, search text, jump to a symbol", async () => {
    await page.keyboard.press("Control+p");
    await page.waitForSelector(".palette.search .search-row >> text=limiter.rs");
    await page.keyboard.type("lim");
    await page.waitForSelector(".search-row mark");
    await page.keyboard.press("Tab");
    await page.waitForSelector(".seg-btn.active >> text=Text");
    await page.fill(".palette.search > input", "allow");
    await page.waitForSelector(".search-row >> text=src/limiter.rs:6");
    await page.waitForSelector(".search-status >> text=1 match in 1 file");
    await page.keyboard.press("Tab");
    await page.waitForSelector(".seg-btn.active >> text=Symbols");
    await page.waitForSelector(".search-row .sym-kind >> text=function");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".file-viewer .fv-line.hit[data-line=\"6\"]");
    await page.waitForSelector(".fv-outline >> text=Limiter");
    await page.click(".fv-sym >> nth=0");
    await page.waitForSelector(".fv-line.hit[data-line=\"1\"]");
    await page.keyboard.press("Escape");
    await page.waitForSelector(".file-viewer", { state: "detached" });
  });
  await step("keyboard shortcut switches tabs", async () => {
    await page.click(".tab[data-view=\"tasks\"]");
    await page.keyboard.press("Control+3");
    await page.waitForSelector('.tab.active[data-view="changes"]');
  });
  await step("live event refreshes the screen", async () => {
    await tab("overview");
    await page.waitForSelector("text=Add rate limiting");
    const before = calls.filter((c) => c === "overview").length;
    await page.evaluate((pid) => window.__emit("evyro-events", [{ project_id: pid, kind: "checkpoint.created", payload: {} }]), project.id);
    await page.waitForFunction((n) => true, before);
    await page.waitForTimeout(800);
    if (calls.filter((c) => c === "overview").length <= before) throw new Error("no refresh");
  });
  // EVYRO_SHOTS=docs/assets writes the README screenshots (light and dark).
  if (process.env.EVYRO_SHOTS) {
    await step("screenshots", async () => {
      const dir = path.resolve(process.env.EVYRO_SHOTS);
      fs.mkdirSync(dir, { recursive: true });
      for (const scheme of ["light", "dark"]) {
        await page.emulateMedia({ colorScheme: scheme });
        for (const v of ["overview", "tasks", "changes", "threads", "agents"]) {
          await tab(v);
          await page.waitForTimeout(400);
          await page.evaluate(() => document.querySelectorAll(".toast").forEach((t) => t.remove()));
          await page.screenshot({ path: path.join(dir, `screen-${v}-${scheme}.png`) });
        }
      }
      await page.emulateMedia({ colorScheme: "light" });
    });
  }
  await step("first-run setup when there are no projects", async () => {
    await page.evaluate(() => { state.projects = []; state.project = null; setTab("overview"); });
    await page.waitForSelector("ol.setup");
  });

  await browser.close();
  fs.rmSync(tmp, { recursive: true, force: true });
  const real = errors.filter((e) => !/not_mocked/.test(e));
  if (real.length) {
    console.error("\n" + real.join("\n"));
    process.exit(1);
  }
  console.log("\nUI smoke test passed.");
})().catch((e) => { console.error(e); process.exit(1); });
