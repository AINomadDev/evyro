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
  // The shared-context panel reads the active task's criteria; their shape comes from the real CLI.
  task_detail: { task, criteria: evj("task", "show", "T-1").acceptance, dependencies: [], blocked_by: [], owner: null,
    verifications: [], checkpoints: [], comments: [], allowed_next: ["in_review", "blocked"] },
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
  // SMOKE_THROTTLE=6 runs the page six times slower, to reproduce what a slow CI machine shows.
  if (process.env.SMOKE_THROTTLE) {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: Number(process.env.SMOKE_THROTTLE) });
  }
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
          if (cmd === "list_sessions" && window.__sessions) return window.__sessions;
          if (cmd in fix) return JSON.parse(JSON.stringify(fix[cmd]));
          if (cmd === "comparison_list") return window.__comparisons || [];
          if (cmd === "comparison_create") {
            const run = { id: "cmp_fixture", base: "abcd1234ef002102abfe", preferred: null,
              candidates: args.targets.map((target, i) => ({ key: `T-${i + 2}`, target, worktree: `/scratch/comparison/${i}`, branch: `feature/compare-fixture-${i}` })) };
            (window.__comparisons ||= []).push(run);
            return run;
          }
          if (cmd === "comparison_results") return { comparison: window.__comparisons.find((r) => r.id === args.id), results: [
            { status: "in_review", running: false, diff: "+first candidate", verification: { passed: true, command: ["cargo", "test"] } },
            { status: "in_progress", running: false, diff: "+second candidate", verification: { passed: false, command: ["cargo", "test"] }, verification_current: false },
          ] };
          if (cmd === "run_verification" && args.kind === "test") { (window.__verified ||= []).push(args.task); return { receipt: { passed: true }, output_tail: "" }; }
          if (cmd === "comparison_prefer") { window.__comparisons.find((r) => r.id === args.id).preferred = args.candidate; return null; }
          if (cmd === "comparison_preview_stage") {
            if (window.__blockStagePreview) throw new Error("Commit or set aside changes in the destination checkout before staging");
            return window.__stagePlan = {
              candidate: window.__comparisons.find((r) => r.id === args.id).preferred, candidate_commit: "cd5d5e123456789",
              destination: "/scratch/my-app", destination_branch: "main", destination_commit: "abcd1234ef002102abfe",
              files: ["app.txt", "image.bin"],
            };
          }
          if (cmd === "comparison_stage") {
            if (JSON.stringify(args.plan) !== JSON.stringify(window.__stagePlan) || args.id !== "cmp_fixture" || args.task !== "T-1") {
              throw new Error("Stage did not receive the reviewed plan");
            }
            window.__staged = args.plan.candidate;
            return null;
          }
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
          if (cmd === "remote_tailscale") {
            if (rm.public) return { state: "public" };
            if (args.enable) rm.private = true;
            return rm.private ? { state: "ready", url: "https://mac.test-tailnet.ts.net:8443/" } : { state: "setup", port: rm.port, https_port: 8443 };
          }
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
            return { prompt, count: open.length, ids: open.map((c) => c.id) };
          }
          if (cmd === "review_mark_sent") {
            rv.filter((c) => c.project_id === args.projectId && c.status === "open" && args.ids.includes(c.id)).forEach((c) => (c.status = "sent"));
            return null;
          }
          // Terminals: a stand-in that echoes input back, like a PTY would.
          const terms = (window.__terms = window.__terms || []);
          if (cmd === "context_packet") return { text: "Terminal task context draft", packet: { approx_tokens: 5 } };
          if (cmd === "agent_quota") {
            if (window.__quotaOverrides?.[args.target]) return window.__quotaOverrides[args.target];
            if (args.target === "codex") return { state: "ready", windows: [{ remaining: 73, minutes: 300 }], credits: null, observed_at: new Date().toISOString(), stale: true };
            if (args.target === "antigravity-cli") return { state: "ready", windows: [{ label: "Gemini", remaining: 26, minutes: 300 }], credits: null, observed_at: new Date().toISOString(), source: "Antigravity CLI" };
            if (args.target === "claude-code") return { state: "setup", capture_command: "evyro capture-usage claude-code" };
            return { state: "unavailable" };
          }
          if (cmd === "opencode_usage") return { state: "stats", input: 1234, output: 500, cost: 0.15, observed_at: new Date().toISOString(), source: "OpenCode stats" };
          if (cmd === "handoff_checkpoint") return { summary: "Reviewed the existing limiter.", completed: ["Found the rate limit tests"], remaining: ["Add a 429 test"], next_action: "Write the failing test first" };
          if (cmd === "prepare_handoff") {
            window.__handoff = args.input;
            return { record: { id: "hnd_continue" } };
          }
          if (cmd === "handoff_detail" && args.handoffId === "hnd_continue") return {
            record: { id: "hnd_continue", to_harness: window.__handoff.to, status: "prepared", quality: { met: 1, total: 1, criteria: [] } },
            contract: { task: { key: window.__handoff.task } }, integrity_ok: true,
            markdown: "Reviewed the existing limiter.", resume_prompt: "Continue T-1. Write the failing test first."
          };
          if (cmd === "history_status_cmd" || cmd === "history_set" || cmd === "history_clear") {
            const hs = (window.__history ||= { keep: { threads: 60, chats: 60 }, saved: { threads: 3, chats: 2 }, max: 500 });
            const removed = { threads: 0, chats: 0 };
            if (cmd === "history_set") {
              (window.__historyCalls ||= []).push({ cmd, ...args });
              hs.keep = { threads: args.threadsKeep, chats: args.chatsKeep };
              removed.threads = Math.max(0, hs.saved.threads - args.threadsKeep); hs.saved.threads -= removed.threads;
              removed.chats = Math.max(0, hs.saved.chats - args.chatsKeep); hs.saved.chats -= removed.chats;
            }
            if (cmd === "history_clear") {
              (window.__historyCalls ||= []).push({ cmd, ...args });
              if (args.what !== "chats") { removed.threads = hs.saved.threads; hs.saved.threads = 0; }
              if (args.what !== "threads") { removed.chats = hs.saved.chats; hs.saved.chats = 0; }
            }
            return JSON.parse(JSON.stringify({ ...hs, removed }));
          }
          if (cmd === "pr_feedback") return { task: args.task, branch: "feat/x", pr: { number: 5, url: "https://github.com/o/r/pull/5", state: "OPEN" }, pending: 0,
            failed: [{ name: "test (ubuntu)", url: null, log_tail: "boom" }], review: [{ author: "reviewer", path: "src/a.rs", line: 12, body: "rename" }], actionable: true,
            prompt: "The pull request for T-1 needs changes. FIX-ME-PROMPT" };
          if (cmd === "create_sample") { window.__sampleCalls = (window.__sampleCalls || 0) + 1; return { created: true, tasks: ["T-1", "T-2"], project: { id: window.__projectId, name: "Evyro sample", root_path: "/data/samples/evyro-sample" } }; }
          if (cmd === "tour_status") return window.__tour || { started: false, checkpoint: false, verified: false, handoff: false };
          if (cmd === "plan_get") return JSON.parse(JSON.stringify({ task: args.task, plan: window.__plan || null }));
          if (cmd === "plan_decide") {
            window.__planDecision = args;
            window.__plan = { ...window.__plan, status: args.approve ? "approved" : "rejected", note: args.note };
            return JSON.parse(JSON.stringify(window.__plan));
          }
          if (cmd === "next_work") return { task: { key: "T-9", title: "Next ready task", priority: 0 }, agent: "codex" };
          if (cmd === "term_stop") { (window.__stopped ||= []).push(args.id); const t = terms.find((x) => x.id === args.id); if (t) t.running = false; return null; }
          if (cmd === "switch_owner") { window.__switch = args; return { handoff: { record: { id: "hnd_switch" } }, from: { harness: "claude-code" }, stopped: true }; }
          if (cmd === "term_list") return JSON.parse(JSON.stringify(terms));
          if (cmd === "term_open") {
            window.__lastTermOpen = args;
            const earlier = terms.find((t) => t.id === args.resumeId);
            const t = { id: earlier?.id || "term_" + (terms.length + 1), title: args.title || (args.kind === "agent" ? `${args.target} on ${args.task}` : "Shell"), kind: args.kind,
              target: args.target, task: args.task, project_id: args.projectId, cwd: "/repo", command: "evyro run", started_at: earlier?.started_at || Math.floor(Date.now() / 1000), running: true, exit_code: null };
            if (earlier) terms[terms.indexOf(earlier)] = t;
            else terms.push(t);
            const output = "\x1b[?2004h\x1b[2m[evyro] launching Claude Code for target 'claude-code' (session ses_01a2)\x1b[0m\r\n\r\n\x1b[1;35m*\x1b[0m Reading .evyro/CURRENT_TASK.md\r\n  T-1 Add rate limiting. Next: write the test\r\n\x1b[1;35m*\x1b[0m Writing tests/limiter.rs\r\n\x1b[32m✓\x1b[0m evyro verify -- cargo test (12 passed)\r\nagent ready\r\n";
            setTimeout(() => {
              const snapshots = (window.__snap ||= {});
              const previous = earlier ? snapshots[t.id] + "\r\n[Resumed Evyro thread]\r\n" : "";
              snapshots[t.id] = previous + output;
              window.__emit("term-output", { id: t.id, data: output, end: snapshots[t.id].length });
            }, 50);
            return t;
          }
          if (cmd === "term_snapshot") {
            const startup = window.__startupSnapshots?.[args.id];
            if (startup) return args.attach ? startup : { data: startup.data + startup.live, end: startup.end };
            const d = (window.__snap || {})[args.id];
            if (d !== undefined) return { data: d, end: d.length };
            return terms.some((t) => t.id === args.id && /^(term_9|term_own)$/.test(t.id)) ? { data: "\x1b[?2004h", end: 8 } : { data: "", end: 0 };
          }
          if (cmd === "release_session") { window.__released = args.confirmed === true ? args.sessionId : "unconfirmed"; return { session_id: args.sessionId, harness: "claude-code", checkpoint_id: "chk_1", changed_since_last_checkpoint: [] }; }
          if (cmd === "term_resize") return null;
          if (cmd === "term_write") {
            if (window.__rejectWrite === args.id) throw { message: "Test recipient is unavailable" };
            window.__lastTermWrite = (window.__lastTermWrite || "") + args.data;
            (window.__writes ||= []).push(args);
          }
          if (cmd === "term_write") { setTimeout(() => window.__emit("term-output", { id: args.id, data: args.data, end: 0 }), 10); return null; }
          if (cmd === "term_close") { window.__terms = terms.filter((t) => t.id !== args.id); return null; }
          // Chat threads (ACP): a stand-in agent that plans, asks permission, edits and answers.
          const chats = (window.__chats = window.__chats || {});
          const say = (id, item) => { chats[id].log.push(item); window.__emit("chat-event", { id, item }); };
          const upd = (id, update) => say(id, { t: "update", update });
          const setc = (id, patch) => { Object.assign(chats[id].info, patch); window.__emit("chat-state", JSON.parse(JSON.stringify(chats[id].info))); };
          if (cmd === "acp_agents") return [{ harness: "codex", display_name: "Codex CLI (Codex ACP adapter)", command: "/usr/local/bin/codex-acp", install: "npm install -g @agentclientprotocol/codex-acp", evidence: "" },
            { harness: "gemini", display_name: "Gemini CLI", command: null, install: "npm install -g @google/gemini-cli", evidence: "" },
            { harness: "antigravity-cli", command: "agy" }, { harness: "opencode", command: "opencode" }];
          if (cmd === "chat_list") return Object.values(chats).map((c) => JSON.parse(JSON.stringify(c.info)));
          if (cmd === "chat_search_saved") {
            const query = args.query.toLowerCase();
            return Object.values(chats).filter(({ info }) => info.archived && info.project_id === args.projectId).flatMap(({ info, log }) =>
              log.flatMap((item, message_index) => {
                const excerpt = item.t === "user" ? item.text
                  : item.update?.sessionUpdate === "agent_message_chunk" ? item.update.content.text : "";
                return typeof excerpt === "string" && excerpt.toLowerCase().includes(query) ? [{ id: info.id, title: info.title, message_index, excerpt }] : [];
              }));
          }
          if (cmd === "chat_log") return JSON.parse(JSON.stringify({ info: chats[args.id].info, log: chats[args.id].log, draft: chats[args.id].info.draft_available ? "Draft for the rate limit task" : null }));
          if (cmd === "chat_open") {
            const id = "chat_" + (Object.keys(chats).length + 1);
            const info = { id, kind: "chat", title: `${args.target} on ${args.task}`, target: args.target, task: args.task, project_id: args.projectId, started_at: Math.floor(Date.now() / 1000), running: true, busy: true, state: "starting", exit_code: null, command: "ACP chat" };
            chats[id] = { info, log: [] };
            setTimeout(() => setc(id, { state: "ready", busy: false, draft_available: true, agent: "Codex", session: "ses_01chat", cwd: "/repo", model_choice: {
              current: "fast", options: [{ value: "fast", name: "Fast" }, { value: "deep", name: "Deep" }],
            } }), 60);
            return JSON.parse(JSON.stringify(info));
          }
          if (cmd === "chat_set_model") {
            window.__modelChange = args;
            const active = chats[args.id].info;
            setc(args.id, { model_choice: { ...active.model_choice, current: args.model } });
            return null;
          }
          if (cmd === "chat_send" && chats[args.id].info.draft_available) {
              const id = args.id;
              setc(id, { busy: true, draft_available: false });
              say(id, { t: "user", text: args.text });
              upd(id, { sessionUpdate: "plan", entries: [{ content: "Read the limiter", status: "completed" }, { content: "Add a test for 429", status: "in_progress" }, { content: "Run the tests", status: "pending" }] });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "I read the task through Evyro. " } });
              upd(id, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Next I add a test for the 429 path." } });
              upd(id, { sessionUpdate: "tool_call", toolCallId: "c1", title: "Read src/limiter.rs", kind: "read", status: "completed", locations: [{ path: "/repo/src/limiter.rs" }] });
              upd(id, { sessionUpdate: "tool_call", toolCallId: "c2", title: "Edit tests/limiter.rs", kind: "edit", status: "pending",
                content: [{ type: "diff", path: "/repo/tests/limiter.rs", oldText: "#[test]\nfn allows() {}\n", newText: "#[test]\nfn allows() {}\n#[test]\nfn blocks_after_100() {}\n" }] });
              say(id, { t: "permission", request: 7, params: { sessionId: "s", toolCall: { toolCallId: "c2", title: "Edit tests/limiter.rs", kind: "edit" },
                options: [{ optionId: "allow", name: "Allow", kind: "allow_once" }, { optionId: "always", name: "Always allow", kind: "allow_always" }, { optionId: "reject", name: "Reject", kind: "reject_once" }] } });
            return null;
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
          if (cmd === "chat_close") {
            if (args.forget === false) Object.assign(chats[args.id].info, { running: false, busy: false, waiting: 0, archived: true, draft_available: false });
            else delete chats[args.id];
            return null;
          }
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
    catch (e) {
      errors.push(`${name}: ${e.message.split("\n").slice(0, process.env.SMOKE_DEBUG ? 18 : 1).join(" / ")}`); console.log("FAIL  " + name);
      // What was on screen, so a failure on a machine we cannot see is still explainable.
      try {
        const seen = await page.evaluate(() => ({
          modals: [...document.querySelectorAll("#modal-root .modal, #palette-root .modal")].map((m) => (m.getAttribute("aria-label") || m.querySelector("h2")?.textContent || "modal").slice(0, 60)),
          focus: document.activeElement ? `${document.activeElement.tagName}.${String(document.activeElement.className).slice(0, 40)}` : null,
          tab: document.querySelector(".tab.active")?.dataset.view, toasts: [...document.querySelectorAll("#toast, .toast")].map((t) => t.textContent.slice(0, 80)),
          threads: (window.state?.threads || []).map((t) => `${t.id}:${t.running ? "run" : "end"}`).join(","),
        }));
        console.log("  screen: " + JSON.stringify(seen));
      } catch { /* the page is gone */ }
    }
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
  await step("history: handoffs and sessions on one screen", async () => {
    await tab("history");
    await page.waitForSelector("table tbody tr");
    await page.waitForSelector("h2:text-is('Sessions')");
  });
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
    await tab("workspace");
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
    await tab("workspace");
    await tab("workspace");
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
    await page.evaluate(() => { window.__rejectWrite = "term_9"; });
    try {
      await page.click(".modal .btn.primary");
      await page.waitForSelector(".modal .btn.primary:enabled");
      if (await page.evaluate(() => window.__rv.some((c) => c.status === "sent"))) throw new Error("a failed delivery marked the review note as sent");
    } finally { await page.evaluate(() => { window.__rejectWrite = null; }); }
    await page.evaluate(() => invoke("add_review_comment", { projectId: state.project.id, path: "app.txt", body: "Check behavior after reload" }));
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".modal", { state: "detached" });
    await page.waitForFunction(() => window.__lastTermWrite && window.__lastTermWrite.includes("Read the limit from config"));
    await page.waitForSelector(".rv-comment.sent");
    if (await page.evaluate(() => window.__rv.find((c) => c.body === "Check behavior after reload").status !== "open"
      || window.__lastTermWrite.includes("Check behavior after reload"))) throw new Error("a late note was included in the earlier review batch");
    await page.click(".rv-comment.sent >> text=Resolve");
    await page.waitForSelector(".rv-comment.sent", { state: "detached" });
  });
  await step("PR feedback shows what is failing and sends the prompt to the AI on the task", async () => {
    await page.evaluate(() => { window.__lastTermWrite = null; return loadThreads().then(() => prFeedbackDialog({ key: "T-1" })); });
    const dialog = page.getByRole("dialog", { name: "PR feedback for T-1" });
    await dialog.getByText("1 failing check and 1 open review comment").waitFor();
    await dialog.getByText("FIX-ME-PROMPT").waitFor();
    await dialog.getByRole("button", { name: "Send to codex on T-1" }).click();
    // It asks first; cancelling sends nothing and brings the dialog back.
    const sure = page.getByRole("dialog").filter({ hasText: "Send this to codex on T-1?" });
    await sure.getByText(/may include secrets/).waitFor();
    await sure.getByRole("button", { name: "Cancel" }).click();
    await dialog.getByText("FIX-ME-PROMPT").waitFor();
    if (await page.evaluate(() => window.__lastTermWrite)) throw new Error("sent without confirmation");
    await dialog.getByRole("button", { name: "Send to codex on T-1" }).click();
    await page.getByRole("dialog").filter({ hasText: "Send this to codex on T-1?" }).getByRole("button", { name: "Send", exact: true }).click();
    await page.waitForFunction(() => window.__lastTermWrite && window.__lastTermWrite.includes("FIX-ME-PROMPT"));
    await dialog.waitFor({ state: "detached" });
  });
  await step("chat thread over ACP: plan, permission, diff, reply", async () => {
    await tab("workspace");
    await page.click("text=+ Chat");
    await page.selectOption(".modal select >> nth=0", "codex");
    await page.waitForSelector(".modal >> text=Ready:");
    await page.selectOption(".modal select >> nth=1", "T-1");
    await page.click(".modal .btn.primary");
    await page.waitForFunction(() => document.querySelector(".chat-input")?.value === "Draft for the rate limit task");
    if (await page.locator(".chat-msg.you").count()) throw new Error("opening a chat submitted the draft");
    const chatId = await page.locator(".chat-input").getAttribute("data-id");
    await page.getByLabel("Chat model").selectOption("deep");
    await page.waitForFunction((id) => window.__modelChange?.id === id && window.__modelChange?.model === "deep", chatId);
    if ((await page.inputValue(".chat-input")) !== "Draft for the rate limit task") throw new Error("changing a model cleared the draft");
    if (await page.locator(".chat-msg.you").count()) throw new Error("changing a model submitted a prompt");
    await page.click(".chat-send");
    await page.waitForSelector(".chat-perm.open >> text=Edit tests/limiter.rs");
    await page.waitForSelector(".chat-plan >> text=Add a test for 429");
    await page.waitForSelector(".chat-diff >> text=+ fn blocks_after_100() {}");
    await page.waitForSelector(".thread-item >> text=needs you");
    await page.click(".chat-perm.open >> text=Allow");
    await page.waitForSelector(".chat-perm >> text=You chose: Allow");
    await page.waitForSelector(".chat-tool.completed >> text=Edit tests/limiter.rs");
    await page.waitForSelector(".chat-note >> text=Wrote tests/limiter.rs");
    await page.waitForSelector(".thread-item >> text=your turn");
    await page.fill(".chat-input", "thanks");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".chat-msg.agent >> text=You said: thanks");
    await tab("workspace");
    await tab("workspace");
    await page.click(".thread-item >> text=chat");
    await page.waitForSelector(".chat-msg.agent >> text=You said: thanks");
    await tab("changes");
    await page.click("text=Send comments to agent");
    await page.getByLabel("Send to").selectOption(chatId);
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".rv-comment.sent >> text=Check behavior after reload");
    await page.click(".rv-comment.sent >> text=Resolve");
    await page.waitForSelector(".rv-comment.sent", { state: "detached" });
    await tab("workspace");
    await page.waitForSelector(".chat-msg.agent >> text=Check behavior after reload");
    await page.click("text=End chat");
    await page.click(".modal >> text=Stop it");
    await page.waitForFunction(() => !document.querySelector(".chat"));
    await page.locator("#search-btn").click();
    await page.getByRole("tab", { name: "Saved chats" }).click({ timeout: 3000 });
    await page.getByRole("textbox", { name: "Search", exact: true }).fill("you said: thanks");
    await page.waitForSelector(".search-row mark >> text=You said: thanks");
    await page.keyboard.press("Enter");
    await page.waitForSelector(".chat-search-hit:focus >> text=You said: thanks");
    if (await page.locator(".chat-input, .chat-send, .chat-perm.open").count()) throw new Error("saved chat still offers live controls");
    await page.getByRole("button", { name: "Delete transcript", exact: true }).click();
    await page.locator(".modal").getByRole("button", { name: "Delete transcript", exact: true }).click();
    await page.waitForSelector(".thread-item:has-text('saved chat')", { state: "detached" });
    await page.locator("#search-btn").click();
    await page.waitForSelector(".search-status >> text=No matches in saved chats");
    await page.keyboard.press("Escape");
  });

  // ---------- multi-AI workspace ----------
  const setPaneCount = async (n) => {
    await page.locator(".ws-layout input").fill(String(n));
    await page.locator(".ws-layout input").blur();
    await page.waitForFunction((count) => document.querySelectorAll(".ws-grid > .pane").length === count, n);
  };
  await step("workspace: two AIs side by side, each pane shows what it shares", async () => {
    await tab("workspace");
    await setPaneCount(1);
    await page.click(".thread-item:has-text('codex on T-1')");
    await page.click(".side-new >> text=+ Agent");
    await page.waitForSelector(".modal select");
    await page.selectOption(".modal select >> nth=1", "T-1");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".modal", { state: "detached" });
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid > .pane[data-id]").length === 2);
    await page.waitForFunction(() => document.querySelector(".ws-input").value === "Terminal task context draft");
    await page.fill(".ws-input", "");
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid .pane .xterm").length === 2);
    await page.waitForSelector(".pane.focus");
    await page.waitForSelector(".sees >> text=task T-1");
    await page.waitForSelector(".sees >> text=1 decision");
    // The sidebar groups threads by state.
    await page.waitForSelector(".side-group >> text=Working");
    await page.waitForSelector(".side-group >> text=Ended");
    await page.waitForSelector("#statusbar >> text=2 AIs running");
    // A missing value must never show up as the word "null" or "undefined".
    const stray = await page.evaluate(() => [".ws", "#ctx", "#statusbar", "#side-threads"].map((q) => document.querySelector(q).innerText).join("\n").match(/[^\n]*\b(?:null|undefined)\b[^\n]*/));
    if (stray) throw new Error(`"${stray[0]}" shown in the window`);
    // Same for a project with no checkpoint yet.
    const strip = await page.evaluate(() => {
      const old = state.ctx, el = document.createElement("div");
      state.ctx = { ...old, overview: { ...old.overview, latest_checkpoint: null } };
      fillSees(el, { task: null });
      state.ctx = old;
      return el.textContent;
    });
    if (/null|undefined/.test(strip)) throw new Error("strip shows: " + strip);
  });
  await step("starting the same AI again points to the thread already running", async () => {
    await page.click(".side-new >> text=+ Agent");
    await page.waitForSelector(".modal select");
    await page.selectOption(".modal select >> nth=1", "T-1");
    await page.waitForSelector(".dup-note >> text=already running");
    await page.click(".dup-note >> text=Show it");
    await page.waitForSelector(".modal", { state: "detached" });
  });
  await step("threads from an earlier run are listed and resume; a stuck task can be freed", async () => {
    await page.evaluate((pid) => {
      window.__terms.push({ id: "term_old_1", title: "codex on T-1", kind: "agent", target: "codex", task: "T-1", project_id: pid, cwd: "/repo",
        command: "evyro run", started_at: 2, running: false, exit_code: 5, restored: true, stopped_by_app: false });
      window.__snap = { term_old_1: "error: conflict: T-1 is owned by session ses_01dead (claude-code); hand off with `evyro handoff --to codex` or end that session first\r\n" };
    }, project.id);
    await page.evaluate(() => loadThreads().then(() => render()));
    await page.waitForSelector(".thread-item:has-text('codex on T-1') >> text=earlier run");
    await page.click(".thread-item:has-text('earlier run')");
    await page.waitForSelector(".pane:has(.thread-title:text-is('codex on T-1')) >> text=Resume");
    await page.waitForSelector(".pane-fix >> text=Free T-1 and retry");
    const before = await page.evaluate(() => window.__terms.length);
    await page.click(".pane-fix >> text=Free T-1 and retry");
    await page.getByRole("button", { name: "Free it", exact: true }).click();
    await page.waitForFunction((n) => window.__released === "ses_01dead" && window.__terms.length === n && window.__terms.some((t) => t.id === "term_old_1" && t.running), before);
    const started = await page.evaluate(() => window.__terms.find((t) => t.id === "term_old_1"));
    if (started.target !== "codex" || started.task !== "T-1") throw new Error("retried as " + JSON.stringify(started));
    await page.waitForFunction(() => window.__snap.term_old_1.includes("Resumed Evyro thread"));
    // Clean up so later steps see the threads they expect.
    await page.evaluate(async () => { window.__terms = window.__terms.filter((t) => t.id !== "term_old_1"); await loadThreads(); render(); });
  });
  await step("usage panel shows account quota and a named session's separate credit balance", async () => {
    await page.waitForSelector("#ctx >> text=Usage, by AI");
    await page.waitForSelector(".ctx-quota[data-ai=codex] >> text=73% left");
    await page.waitForSelector(".ctx-quota[data-ai=codex] >> text=Last saved reading");
    await page.waitForSelector(".ctx-quota[data-ai=antigravity-cli] >> text=26% left");
    await page.waitForSelector(".ctx-quota[data-ai=opencode] >> text=1,234 input");
    await page.evaluate((pid) => {
      window.__terms.push({ id: "term_budget", title: "Review budget", kind: "agent", target: "copilot", project_id: pid,
        started_at: 1, running: false, credit_budget: 30, credits_used: 2.125 });
      render();
    }, project.id);
    await page.waitForSelector("#ctx .ctx-usage-threads >> text=Review budget");
    await page.waitForSelector("#ctx .ctx-usage-threads >> text=27.875 of 30 credits left");
    await page.evaluate(() => { window.__terms = window.__terms.filter((t) => t.id !== "term_budget"); render(); });
  });
  await step("an AI that claimed a task itself shows it; the header names each AI once", async () => {
    // Started without a task, then ran `evyro task start` itself: list_tasks names claude-code as owner of T-1.
    await page.evaluate((pid) => window.__terms.push({ id: "term_own", title: "claude-own", kind: "agent", target: "claude-code", task: null,
      project_id: pid, cwd: "/repo", command: "evyro run", started_at: 1, running: true, exit_code: null }), project.id);
    await page.evaluate(() => { window.__sessions = ["claude-code", "claude-code", "codex", "claude-code"].map((h, i) => ({ id: "ses_" + i, harness: h, status: "running" })); });
    await page.evaluate(() => Promise.all([loadThreads(), updateLiveBadge()]));
    await page.waitForSelector(".thread-item:has-text('claude-own') >> text=task T-1", { timeout: 8000 });
    await page.waitForFunction(() => document.querySelector("#live-badge").textContent === "With claude-code, codex", null, { timeout: 8000 });
    await page.evaluate(async () => { window.__terms = window.__terms.filter((t) => t.id !== "term_own"); window.__sessions = null; await loadThreads(); await updateLiveBadge(); });
    await page.waitForSelector(".thread-item:has-text('claude-own')", { state: "detached" });
  });
  await step("shared context panel: task, criteria, handoff, decisions, activity", async () => {
    await page.waitForSelector("#ctx:not(.hidden) .ctx-task >> text=Add rate limiting");
    await page.waitForSelector("#ctx .ctx-crit >> text=429 after 100 a minute");
    await page.waitForSelector("#ctx >> text=Review");
    await page.waitForSelector("#ctx .ctx-list >> text=Token bucket in memory");
    await page.waitForSelector("#ctx .ctx-feed li");
  });
  await step("context panel sends a decision to the focused AI", async () => {
    await page.evaluate(() => { window.__lastTermWrite = ""; });
    await page.click("#ctx .ctx-list .ctx-send");
    await page.waitForSelector(".toast >> text=Sent the decision");
    if (!(await page.evaluate(() => window.__lastTermWrite.includes("Token bucket in memory")))) throw new Error("decision text was not sent");
  });
  await step("composer sends one message to several AIs at once", async () => {
    await page.evaluate(() => { window.__lastTermWrite = ""; });
    await page.getByRole("button", { name: "All AIs", exact: true }).click();
    if ((await page.$$(".ws-to .chip.toggle[aria-pressed=true]")).length !== 2) throw new Error("expected both running AIs selected");
    await page.fill(".ws-input", "Use the token bucket decision");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => ((window.__lastTermWrite || "").match(/Use the token bucket decision/g) || []).length === 2);
    await page.waitForSelector(".ws-send-status >> text=Sent to 2 AIs");
    // Pick one recipient only.
    await page.click(".ws-to .chip.toggle >> nth=0");
    await page.waitForFunction(() => document.querySelectorAll(".ws-to .chip.toggle[aria-pressed=true]").length === 1);
  });
  await step("named threads, focus mode and retrying only the failed recipient", async () => {
    // Earlier resume coverage can leave a free slot. Put both running AIs in
    // the grid so the new thread has to expand the row.
    await page.evaluate(() => {
      for (const t of projThreads().filter((thread) => thread.running && thread.kind === "agent")) placeThread(t.id);
      render();
    });
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid > .pane[data-id]").length === 2);
    await page.getByLabel("Columns", { exact: true }).selectOption({ label: "Side by side" });
    await page.click(".side-new >> text=+ Agent");
    await page.getByLabel("Thread name", { exact: false }).fill("Compare implementations");
    await page.click(".modal .btn.primary");
    await page.waitForSelector(".pane .thread-title:text-is('Compare implementations')");
    const panes = await page.locator(".ws-grid > .pane[data-id]").evaluateAll((nodes) => nodes.map((el) => ({ x: el.offsetLeft, y: el.offsetTop })));
    if (panes.length !== 3 || !panes.every((p, i) => p.y === panes[0].y && (i === 0 || p.x > panes[i - 1].x))) {
      throw new Error(`third thread did not extend the side-by-side row: ${JSON.stringify(panes)}`);
    }
    await page.waitForFunction(() => {
      const grid = document.querySelector(".ws-grid");
      const pane = grid.querySelector(".pane.focus");
      return pane?.textContent.includes("Compare implementations") && grid.scrollLeft > 0
        && pane.getBoundingClientRect().right <= grid.getBoundingClientRect().right + 1;
    });
    await page.getByRole("button", { name: "Focus pane", exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid .pane:not([hidden])").length === 1);
    const initialFocus = await page.locator(".ws-grid .pane:not([hidden])").getAttribute("data-id");
    await page.getByLabel("Focus thread").selectOption({ index: 0 });
    await page.waitForFunction((prior) => document.querySelector(".ws-grid .pane:not([hidden])")?.dataset.id !== prior, initialFocus);
    await page.getByLabel("Focus thread").selectOption(initialFocus);
    await page.getByRole("button", { name: "All AIs", exact: true }).click();
    const ids = await page.evaluate(() => {
      const ids = state.threads.filter((t) => t.running && t.kind === "agent").map((t) => t.id);
      window.__rejectWrite = ids[0];
      window.__writes = [];
      return ids;
    });
    await page.fill(".ws-input", "Keep the draft\non failure");
    await page.click(".ws-compose .btn.primary");
    await page.waitForSelector(".ws-send-status >> text=Test recipient is unavailable");
    if ((await page.inputValue(".ws-input")) !== "Keep the draft\non failure") throw new Error("failed delivery lost the draft");
    if ((await page.$$(".ws-to .chip.toggle[aria-pressed=true]")).length !== 1) throw new Error("retry would duplicate a delivery");
    await page.evaluate(() => { window.__rejectWrite = null; });
    await page.click(".ws-compose .btn.primary");
    await page.waitForFunction(() => document.querySelector(".ws-input").value === "");
    const delivered = await page.evaluate(() => window.__writes.filter((w) => w.data.includes("Keep the draft")));
    if (delivered.length !== ids.length || !delivered.every((w) => w.data === "\x1b[200~Keep the draft\ron failure\x1b[201~")) throw new Error("multiline delivery was split or duplicated");
    await page.getByRole("button", { name: "Exit focus", exact: true }).click();
    await page.click(".pane:has(.thread-title:text-is('Compare implementations')) >> text=Stop");
    await page.click(".modal >> text=Stop it");
  });
  await step("composer preserves the message at Copilot's trust prompt", async () => {
    await page.evaluate((pid) => {
      window.__terms.push({ id: "term_trust", title: "Copilot trust test", kind: "agent", target: "copilot", task: null,
        project_id: pid, command: "evyro run", started_at: 1, running: true });
      window.__snap.term_trust = "\x1b[?2004hConfirm folder trust\r\nDo you trust the files in this folder?";
      window.__writes = [];
      render();
    }, project.id);
    await page.waitForSelector(".ws-to >> text=Copilot trust test");
    while ((await page.$$(".ws-to .chip.toggle[aria-pressed=true]:not(:has-text('Copilot trust test'))")).length) {
      await page.locator(".ws-to .chip.toggle[aria-pressed=true]:not(:has-text('Copilot trust test'))").first().click();
    }
    const recipient = page.locator(".ws-to .chip.toggle:has-text('Copilot trust test')");
    if ((await recipient.getAttribute("aria-pressed")) === "false") await recipient.click();
    await page.fill(".ws-input", "Wait for folder trust");
    await page.click(".ws-compose .btn.primary");
    await page.waitForSelector(".ws-send-status >> text=Review Copilot's folder trust prompt");
    if ((await page.inputValue(".ws-input")) !== "Wait for folder trust" || (await page.evaluate(() => window.__writes.length))) throw new Error("trust prompt received composer input");
    await page.fill(".ws-input", "");
    await page.waitForSelector(".ws-send-status", { state: "hidden" });
    await page.evaluate(() => { window.__terms = window.__terms.filter((t) => t.id !== "term_trust"); render(); });
  });
  await step("layouts: adjustable pane count and columns", async () => {
    await setPaneCount(1);
    const remove = page.getByRole("button", { name: "Remove pane" });
    const add = page.getByRole("button", { name: "Add pane" });
    if (!(await remove.isDisabled())) throw new Error("cannot remove the last pane");
    await setPaneCount(5);
    await page.waitForSelector(".pane-empty >> text=Empty pane");
    await add.focus();
    await page.keyboard.press("Space");
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid > .pane").length === 6);
    await page.waitForFunction(() => document.activeElement.getAttribute("aria-label") === "Add pane");
    await remove.click();
    await page.waitForFunction(() => document.querySelectorAll(".ws-grid > .pane").length === 5);
    await page.getByLabel("Columns", { exact: true }).selectOption("3");
    await page.waitForFunction(() => document.querySelector(".ws-grid")?.style.getPropertyValue("--pane-columns") === "3");
    await page.getByLabel("Columns", { exact: true }).selectOption("0");
    await setPaneCount(2);
    const viewport = page.viewportSize();
    await page.setViewportSize({ ...viewport, width: 980 });
    const compact = await page.evaluate(() => {
      const density = appearance().density;
      saveAppearance({ density: "compact" });
      const group = document.querySelector(".ws-layout");
      const controls = [...group.querySelectorAll("button, input, select")];
      const boxes = controls.map((el) => el.getBoundingClientRect());
      const aligned = boxes.every((box) => box.top === boxes[0].top && box.height === boxes[0].height);
      const centered = controls.filter((el) => el.tagName === "BUTTON").every((el) => getComputedStyle(el).paddingInlineStart === "0px");
      saveAppearance({ density });
      return aligned && centered;
    });
    await page.setViewportSize(viewport);
    if (!compact) throw new Error("compact toolbar controls must share a baseline and keep the stepper icons centered");
  });
  await step("restored shell is read-only until restarted in the same pane", async () => {
    await page.evaluate((pid) => {
      window.__terms.push({ id: "term_saved_shell", title: "Saved shell", kind: "shell", target: null, task: null,
        project_id: pid, cwd: "/repo", command: "/bin/zsh -l", started_at: 1, running: false, restored: true, stopped_by_app: true });
      (window.__snap ||= {}).term_saved_shell = "Saved shell output\r\n$ ";
      window.__writes = [];
      render();
    }, project.id);
    await page.locator(".thread-item").filter({ hasText: "Saved shell" }).click();
    const pane = page.locator('.ws-grid > .pane[data-id="term_saved_shell"]');
    await pane.locator(".xterm").waitFor();
    const restart = pane.getByRole("button", { name: "Restart shell" });
    if (!(await restart.count())) throw new Error("stopped shell has no restart action");
    await pane.locator(".xterm").click();
    await page.keyboard.type("ignored");
    if (await page.evaluate(() => window.__writes.some((w) => w.id === "term_saved_shell"))) throw new Error("stopped shell received input");
    const before = await page.evaluate(() => window.__terms.length);
    await restart.click();
    await page.waitForFunction((count) => window.__terms.length === count && window.__terms.some((t) => t.id === "term_saved_shell" && t.running), before);
    await pane.locator(".xterm").click();
    await page.keyboard.type("echo ready");
    await page.waitForFunction(() => window.__writes.filter((w) => w.id === "term_saved_shell").map((w) => w.data).join("").includes("echo ready"));
    await page.waitForFunction(() => window.__snap.term_saved_shell.includes("Saved shell output") && window.__snap.term_saved_shell.includes("Resumed Evyro thread"));
    await pane.getByRole("button", { name: "Stop", exact: true }).click();
    await page.getByRole("button", { name: "Stop it" }).click();
  });
  await step("terminal drawer runs a shell under the panes", async () => {
    await page.click(".ws-bar >> text=Terminal");
    await page.click(".ws-drawer >> text=Start a shell here");
    await page.waitForSelector(".ws-drawer .xterm");
    // The shell lives in the drawer, not in a pane.
    if (await page.$(".ws-grid .pane .thread-title:text-is('Shell')")) throw new Error("shell also shown in a pane");
    await page.click(".ws-drawer-head");
    await page.keyboard.press("Control+`");
    await page.waitForSelector(".ws-drawer", { state: "detached" });
  });
  await step("inspector Changes tab lists changed files", async () => {
    await page.click(".ctx-tab[data-pane=changes]");
    await page.waitForSelector("#ctx .ctx-file >> text=app.txt");
    await page.click(".ctx-tab[data-pane=context]");
    await page.waitForSelector("#ctx .ctx-task");
  });
  await step("shared context hides and comes back", async () => {
    await page.locator("#ctx-btn").focus();
    await page.keyboard.press(process.platform === "darwin" ? "Meta+." : "Control+.");
    await page.waitForSelector("#ctx.hidden", { state: "attached" });
    await page.click("#ctx-btn");
    await page.waitForSelector("#ctx:not(.hidden)");
  });
  await step("task board shows the AIs working on each card", async () => {
    await tab("tasks");
    await page.waitForSelector(".task-card.live .card-ai");
    await page.click(".task-card.live .card-ai >> text=Open");
    await page.waitForSelector('.tab.active[data-view="workspace"]');
  });
  await step("a task card keeps keyboard focus through a live refresh", async () => {
    await tab("tasks");
    const card = page.locator(".task-card.live").first();
    await card.focus();
    await page.waitForTimeout(5300);
    if (!(await card.evaluate((el) => document.activeElement === el))) throw new Error("the Tasks board poll dropped keyboard focus");
    await page.evaluate(() => window.__emit("evyro-events", [{ project_id: state.project.id, kind: "task.updated" }]));
    await page.waitForTimeout(450);
    if (!(await card.evaluate((el) => document.activeElement === el))) throw new Error("a live task event dropped keyboard focus");
    await page.keyboard.press("Enter");
    await page.waitForSelector("#drawer:not(.hidden) #drawer-body >> text=429 after 100 a minute");
    await page.keyboard.press("Escape");
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
  await step("appearance: theme, accent, fonts and layout apply at once", async () => {
    await tab("settings");
    await page.waitForSelector(".appearance");
    await page.click('.ap-theme[data-theme-preview="midnight"]');
    await page.waitForSelector('html[data-theme="midnight"][data-scheme="dark"]', { state: "attached" });
    await page.click('.ap-accent[aria-label="Signal cyan"]');
    const acc = await page.evaluate(() => document.documentElement.style.getPropertyValue("--accent"));
    if (acc !== "#22d3ee") throw new Error("accent " + acc);
    await page.selectOption('select[aria-label="Code and terminal font"]', "fira");
    if (!(await page.evaluate(() => document.documentElement.style.getPropertyValue("--mono").includes("Fira Code")))) throw new Error("mono font");
    await page.click(".appearance .seg-btn >> text=Compact");
    await page.waitForSelector('html[data-density="compact"]', { state: "attached" });
    await page.click(".appearance .seg-btn >> text=Left");
    await page.waitForSelector('html[data-inspector="left"]', { state: "attached" });
    const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("evyro.appearance")));
    if (saved.theme !== "midnight" || saved.accent !== "cyan" || saved.monoFont !== "fira") throw new Error("not saved");
    // Terminals follow the font.
    const tf = await page.evaluate(() => [...XTERMS.values()][0].term.options.fontFamily);
    if (!tf.includes("Fira Code")) throw new Error("terminal font " + tf);
    // Bad settings are ignored, not applied.
    await page.evaluate(() => saveAppearance({ theme: "neon", customUiFont: "x;}body{color:red", uiSize: 99 }));
    const after = await page.evaluate(() => ({ theme: appearance().theme, font: appearance().customUiFont, size: appearance().uiSize }));
    if (after.theme !== "midnight" || after.font !== "" || after.size !== 18) throw new Error("bad input applied: " + JSON.stringify(after));
    await page.click(".appearance >> text=Reset");
    await page.waitForFunction(() => document.documentElement.dataset.theme !== "midnight" && document.documentElement.dataset.inspector === "right");
  });
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
    await page.getByRole("button", { name: "Check private address" }).click();
    await page.getByRole("button", { name: "Enable private address" }).click();
    await page.waitForSelector("text=https://mac.test-tailnet.ts.net:8443/");
    await page.getByRole("button", { name: "Pair over Tailscale" }).click({ timeout: 3000 });
    await page.fill(".modal input[type=text]", "My phone");
    await page.evaluate(() => { window.__rm.public = true; });
    await page.click(".modal .actions .btn.primary");
    await page.waitForSelector(".toast >> text=The private address changed");
    if (await page.evaluate(() => window.__rm.devices.length)) throw new Error("a pairing code was created for a public route");
    await page.evaluate(() => { window.__rm.public = false; });
    await page.click(".modal .actions .btn.primary");
    await page.waitForSelector(".modal .pair-code >> text=K7QM-4XPZ");
    await page.waitForSelector(".modal >> text=https://mac.test-tailnet.ts.net:8443/");
    await page.getByRole("button", { name: "Copy pairing code" }).waitFor();
    await page.getByRole("dialog", { name: "Pairing code" }).getByRole("button", { name: "Copy address", exact: true }).waitFor();
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
    await step("accessibility: no serious WCAG problems on any screen, in every theme", async () => {
      const found = [];
      // System light and dark, then the two extra themes from Appearance settings.
      for (const scheme of ["light", "dark", "midnight", "paper", "contrast-light", "contrast-dark"]) {
      if (["light", "dark"].includes(scheme)) await page.emulateMedia({ colorScheme: scheme });
      else await page.evaluate((t) => saveAppearance({ theme: t }), scheme);
      for (const v of ["workspace", "tasks", "changes", "history", "knowledge", "skills", "agents", "settings"]) {
        await tab(v);
        await page.waitForTimeout(250);
        if (!(await page.evaluate(() => !!window.axe))) await page.addScriptTag({ path: axePath });
        const res = await page.evaluate(async () => {
          const r = await window.axe.run(document, { preload: false, runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] } });
          return r.violations.map((x) => ({ id: x.id, impact: x.impact, help: x.help, nodes: x.nodes.slice(0, 3).map((n) => { const d = n.any[0] && n.any[0].data; return n.target.join(" ") + (d && d.fgColor ? ` ${d.fgColor} on ${d.bgColor} = ${d.contrastRatio}` : ""); }) }));
        });
        for (const x of res) found.push({ screen: `${v}, ${scheme}`, ...x });
        if (scheme.startsWith("contrast")) {
          // The high-contrast themes promise 7:1 (WCAG AAA) for text.
          const enhanced = await page.evaluate(async () => {
            const r = await window.axe.run(document, { preload: false, runOnly: { type: "rule", values: ["color-contrast-enhanced"] } });
            return r.violations.map((x) => ({ id: x.id, impact: x.impact, help: x.help, nodes: x.nodes.slice(0, 3).map((n) => { const d = n.any[0] && n.any[0].data; return n.target.join(" ") + (d && d.fgColor ? ` ${d.fgColor} on ${d.bgColor} = ${d.contrastRatio}` : ""); }) }));
          });
          for (const x of enhanced) found.push({ screen: `${v}, ${scheme}`, ...x, impact: "serious" });
        }
      }
      }
      await page.evaluate(() => saveAppearance({ theme: "system" }));
      await page.emulateMedia({ colorScheme: "light" });
      for (const x of found) console.log(`  a11y ${x.impact}: [${x.screen}] ${x.id}: ${x.help} (${x.nodes.join(", ")})`);
      const bad = found.filter((x) => x.impact === "serious" || x.impact === "critical");
      if (bad.length) throw new Error(`${bad.length} serious or critical accessibility problems`);
    });
  } else {
    console.log("SKIP  accessibility (axe-core not installed)");
  }
  await step("high contrast and reduced motion follow the system and the setting", async () => {
    await page.evaluate(() => saveAppearance({ theme: "system", motion: "system" }));
    // System: a request for more contrast picks the high-contrast theme of the current scheme.
    try { await page.emulateMedia({ colorScheme: "dark", contrast: "more" }); }
    catch { await page.emulateMedia({ colorScheme: "dark" }); }
    await page.evaluate(() => applyAppearance());
    const sys = await page.evaluate(() => ({ theme: document.documentElement.dataset.theme, more: matchMedia("(prefers-contrast: more)").matches }));
    if (sys.more && sys.theme !== "contrast-dark") throw new Error("system contrast request ignored: " + JSON.stringify(sys));
    await page.emulateMedia({ colorScheme: "light", contrast: "no-preference" }).catch(() => page.emulateMedia({ colorScheme: "light" }));
    // Motion: a probe that animates is stopped by the system request and by the setting, and runs otherwise.
    const probe = () => page.evaluate(() => {
      let el = document.getElementById("motion-probe");
      if (!el) { el = document.createElement("div"); el.id = "motion-probe"; el.style.cssText = "animation: pulse 1s infinite; transition: opacity 1s"; document.body.append(el); }
      return { mode: document.documentElement.dataset.motion, animation: getComputedStyle(el).animationName, blink: reducedMotion() };
    });
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.evaluate(() => applyAppearance());
    const full = await probe();
    if (full.mode !== "full" || full.animation !== "pulse") throw new Error("motion off without a request: " + JSON.stringify(full));
    await page.evaluate(() => saveAppearance({ motion: "reduce" }));
    const setting = await probe();
    if (setting.mode !== "reduce" || setting.animation !== "none" || !setting.blink) throw new Error("the setting did not stop motion: " + JSON.stringify(setting));
    await page.evaluate(() => saveAppearance({ motion: "system" }));
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.evaluate(() => applyAppearance());
    const os = await probe();
    if (os.mode !== "reduce" || os.animation !== "none") throw new Error("the system request did not stop motion: " + JSON.stringify(os));
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.evaluate(() => { document.getElementById("motion-probe")?.remove(); saveAppearance({ theme: "system", motion: "system" }); });
  });
  // Keyboard only: on every screen, everything that reacts to a click can be
  // reached with Tab, and whatever Tab reaches shows where the focus is.
  await step("keyboard only: every control reachable with Tab and visibly focused", async () => {
    const found = [];
    for (const v of ["workspace", "tasks", "changes", "history", "knowledge", "skills", "agents", "settings"]) {
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
    await tab("workspace");
    await page.click(".term-host");
    await page.keyboard.press("F6");
    const out = await page.evaluate(() => !document.activeElement.closest(".xterm") && document.activeElement.matches(".thread-head button"));
    if (!out) found.push("[workspace] F6 does not leave the terminal");
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
    await tab("workspace");
    await page.waitForSelector("text=Add rate limiting");
    const before = calls.filter((c) => c === "overview").length;
    await page.evaluate((pid) => window.__emit("evyro-events", [{ project_id: pid, kind: "checkpoint.created", payload: {} }]), project.id);
    await page.waitForFunction((n) => true, before);
    await page.waitForTimeout(800);
    if (calls.filter((c) => c === "overview").length <= before) throw new Error("no refresh");
  });
  await step("continue from a checkpoint with a compact unsent draft", async () => {
    const writes = await page.evaluate(() => {
      workspaceDraft(state.project.id).text = "";
      const t = projThreads().find((thread) => thread.kind === "agent" && thread.task === "T-1");
      placeThread(t.id);
      render();
      return (window.__writes || []).length;
    });
    await page.getByRole("button", { name: "Continue with…", exact: true }).first().click();
    await page.waitForSelector('.modal[aria-label="Continue T-1 with another AI"]');
    if ((await page.getByLabel("Next action").inputValue()) !== "Write the failing test first") throw new Error("checkpoint was not loaded");
    await page.getByRole("button", { name: "Prepare handoff" }).click();
    await page.getByRole("button", { name: "Start with this AI" }).click();
    await page.waitForFunction(() => document.querySelector(".ws-input")?.value === "Continue T-1. Write the failing test first.");
    const after = await page.evaluate(() => (window.__writes || []).length);
    if (writes !== after) throw new Error("continuation sent terminal input before confirmation");
  });
  // EVYRO_SHOTS=docs/assets writes the README screenshots (light and dark).
  if (process.env.EVYRO_SHOTS) {
    await step("screenshots", async () => {
      const dir = path.resolve(process.env.EVYRO_SHOTS);
      fs.mkdirSync(dir, { recursive: true });
      // Clear what earlier steps typed, so the terminals show only agent output.
      await page.evaluate(() => { for (const [id, x] of XTERMS) { const t = state.threads.find((y) => y.id === id); x.term.reset(); x.term.write(`\x1b[2m[evyro] launching ${t ? t.target || "shell" : "agent"} (task T-1)\x1b[0m\r\n\x1b[1;35m*\x1b[0m Reading .evyro/CURRENT_TASK.md\r\n  T-1 Add rate limiting. Next: write the test\r\n\x1b[1;35m*\x1b[0m get_decisions: Token bucket in memory\r\n\x1b[32m✓\x1b[0m evyro verify -- cargo test (12 passed)\r\n`); } });
      for (const scheme of ["light", "dark"]) {
        await page.emulateMedia({ colorScheme: scheme });
        for (const v of ["workspace", "tasks", "changes", "agents", "settings"]) {
          await tab(v);
          await page.waitForTimeout(400);
          await page.evaluate(() => document.querySelectorAll(".toast").forEach((t) => t.remove()));
          await page.screenshot({ path: path.join(dir, `screen-${v}-${scheme}.png`) });
        }
      }
      await page.emulateMedia({ colorScheme: "light" });
    });
  }
  await step("clicking a pane header focuses its terminal so Enter reaches it", async () => {
    await page.evaluate(() => {
      window.__terms.push({ id: "term_enter_probe", title: "Enter probe", kind: "shell", target: null, task: null, project_id: state.project.id, cwd: "/repo", command: "/bin/zsh -l", started_at: 1, running: true });
      window.__writes = [];
      setTab("workspace"); placeThread("term_enter_probe"); render();
    });
    const pane = page.locator('.ws-grid > .pane[data-id="term_enter_probe"]');
    await pane.locator(".xterm").waitFor();
    await page.evaluate(() => document.activeElement?.blur());
    await pane.locator(".thread-head .pane-title").click();
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => window.__writes.some((w) => w.id === "term_enter_probe" && w.data === "\r"), null, { timeout: 3000 })
      .catch(() => { throw new Error("Enter after clicking the pane header did not reach the terminal"); });
  });
  await step("replaying saved output does not send terminal replies to the program", async () => {
    await page.evaluate(() => { window.__writes = []; window.__snap.term_reply_probe = "\x1b[6n\x1b[c"; window.__terms.push({ id: "term_reply_probe", title: "Probe", kind: "shell", target: null, task: null, project_id: state.project.id, cwd: "/repo", command: "/bin/zsh -l", started_at: 1, running: true }); ensureXterm("term_reply_probe"); });
    await page.evaluate(() => XTERMS.get("term_reply_probe").loaded);
    await page.waitForTimeout(300);
    const sent = await page.evaluate(() => window.__writes.filter((w) => w.id === "term_reply_probe"));
    if (sent.length) throw new Error("replay answered terminal queries: " + JSON.stringify(sent));
  });
  await step("a live startup query in the snapshot gets one reply", async () => {
    const replies = await page.evaluate(async () => {
      const id = "term_startup_probe";
      const data = "\x1b[4;7H\x1b[6n";
      const live = "\x1b[2;5H\x1b[6n";
      const end = data.length + live.length;
      window.__startupSnapshots = { [id]: { data, live, end } };
      window.__writes = [];
      state.threads.push({ id, title: "Startup probe", kind: "shell", project_id: state.project.id, running: true });
      const x = ensureXterm(id);
      threadOutput({ id, data: live, end }); // Same bytes also arrived through the event stream.
      await x.loaded;
      await new Promise((resolve) => x.term.write("", resolve));
      return window.__writes.filter((w) => w.id === id).map((w) => w.data);
    });
    if (replies.length !== 1 || replies[0] !== "\x1b[2;5R") throw new Error(`expected one live cursor reply, got ${JSON.stringify(replies)}`);
  });
  await step("Tasks board wraps with shared context open", async () => {
    await page.setViewportSize({ width: 900, height: 700 });
    await page.evaluate(() => { setCtxOpen(true); setTab("tasks"); });
    await page.waitForSelector(".board .column:last-child");
    const fits = await page.locator(".board").evaluate((board) => board.scrollWidth <= board.clientWidth
      && board.lastElementChild.offsetTop > board.firstElementChild.offsetTop
      && board.firstElementChild.offsetHeight < 70);
    if (!fits) throw new Error("Tasks board overflows with the context panel open");
    await page.setViewportSize({ width: 1300, height: 860 });
  });
  await step("shared context leaves workspace controls clickable at the minimum window width", async () => {
    await page.setViewportSize({ width: 900, height: 600 });
    await page.evaluate(() => { setCtxOpen(true); setTab("workspace"); });
    await page.waitForSelector(".ws-compose");
    const blocked = [];
    const prior = await page.evaluate(() => {
      const { sidebarWidth, inspectorWidth, inspectorSide } = appearance();
      saveAppearance({ sidebarWidth: 360, inspectorWidth: 520 });
      return { sidebarWidth, inspectorWidth, inspectorSide };
    });
    for (const inspectorSide of ["right", "left"]) {
      await page.evaluate((side) => saveAppearance({ inspectorSide: side }), inspectorSide);
      if (process.env.EVYRO_SHOTS) await page.screenshot({ path: path.join(process.env.EVYRO_SHOTS, `workspace-900-${inspectorSide}.png`) });
      await page.fill(".ws-input", "First line\nSecond line\nThird line\nFourth line");
      if (inspectorSide === "left") await page.evaluate(() => toast("Delivery status is visible while you compose. ".repeat(8)));
      const covered = await page.evaluate(() => ["#ctx-btn", "#search-btn", ".ws-layout input", ".ws-compose-foot .primary", ".ws-meta .link-btn"].filter((selector) => {
        const el = document.querySelector(selector);
        const { x, y, width, height } = el.getBoundingClientRect();
        return !el.contains(document.elementFromPoint(x + width / 2, y + height / 2));
      }));
      if (covered.length) blocked.push(`${inspectorSide}: ${covered.join(", ")}`);
      const aligned = await page.locator(".ws-compose-foot > .row").evaluate((row) => row.firstElementChild.offsetTop === row.lastElementChild.offsetTop);
      if (!aligned) blocked.push(`${inspectorSide}: composer actions wrap`);
      if (!covered.includes(".ws-meta .link-btn")) await page.getByRole("button", { name: "Discard", exact: true }).click();
    }
    await page.evaluate(save => saveAppearance(save), prior);
    await page.setViewportSize({ width: 1300, height: 860 });
    if (blocked.length) throw new Error(`workspace controls are unreachable at 900px (${blocked.join("; ")})`);
  });
  await step("saved output keeps its recorded width in a narrower pane", async () => {
    await page.evaluate(() => {
      window.__terms.push({ id: "term_size_probe", title: "Saved size probe", kind: "shell", project_id: state.project.id,
        started_at: 1, running: false, cols: 180, rows: 30 });
      window.__snap.term_size_probe = "x".repeat(150);
      setTab("workspace"); placeThread("term_size_probe"); render();
    });
    await page.waitForSelector('.pane[data-id="term_size_probe"] .xterm');
    const result = await page.evaluate(async () => {
      const x = XTERMS.get("term_size_probe");
      await x.loaded;
      fitThread("term_size_probe");
      return { cols: x.term.cols, rows: x.term.rows,
        line: x.term.buffer.active.getLine(0).translateToString(true), scrollable: x.el.scrollWidth > x.el.clientWidth };
    });
    if (result.cols !== 180 || result.rows !== 30 || result.line.length !== 150 || !result.scrollable) {
      throw new Error("saved output did not keep its size in a scrollable pane");
    }
  });
  await step("usage banner offers a handoff and names the AI with the most usage left", async () => {
    const r = await page.evaluate(() => {
      const t = { id: "q1", project_id: state.project.id, running: true, kind: "agent", target: "claude-code", title: "claude-code", task: "T-1" };
      accountQuota.set("claude-code", { state: "ready", windows: [{ remaining: 0, minutes: 300, resets_at: Date.now() / 1000 + 3600 }], credits: null });
      accountQuota.set("codex", { state: "ready", windows: [{ remaining: 66, minutes: 10080, resets_at: Date.now() / 1000 + 3600 }], credits: null });
      const out = quotaBanner(t)?.textContent || "";
      accountQuota.set("claude-code", { state: "ready", windows: [{ remaining: 60, minutes: 300, resets_at: Date.now() / 1000 + 3600 }], credits: null });
      const fine = quotaBanner(t);
      accountQuota.delete("claude-code"); accountQuota.delete("codex");
      return { out, fine };
    });
    if (!/no usage left/.test(r.out) || !/codex has about 66%/.test(r.out)) throw new Error("banner text: " + r.out);
    if (r.fine) throw new Error("banner shown with plenty of usage left");
  });
  await step("usage banner switches the task to another AI only after the usage is confirmed spent", async () => {
    await page.evaluate(() => {
      const t = { id: "term_quick", title: "Quick switch", kind: "agent", target: "claude-code", task: "T-1",
        project_id: state.project.id, started_at: 1, running: true };
      window.__terms.push(t);
      state.threads.push(t);
      accountQuota.set("claude-code", { state: "ready", windows: [{ remaining: 0, resets_at: Date.now() / 1000 + 3600 }], credits: null });
      accountQuota.set("codex", { state: "ready", windows: [{ remaining: 66 }], credits: null });
      window.__switch = null; window.__stopped = []; window.__lastTermOpen = null;
      document.body.append(Object.assign(document.createElement("div"), { id: "quick-host" }));
      document.querySelector("#quick-host").append(quotaBanner(t));
    });
    await page.locator("#quick-host").getByRole("button", { name: "Switch to codex…", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Switch T-1 to another AI" });
    await dialog.getByText(/Only one AI works on T-1 at a time/).waitFor();
    await dialog.getByPlaceholder(/Optional, for example/).fill("small-model");
    await dialog.getByRole("button", { name: "Switch AI", exact: true }).click();
    await page.getByText("Confirm that the usage has run out first.").waitFor();
    if (await page.evaluate(() => window.__switch)) throw new Error("switched without confirmation");
    await dialog.getByLabel(/usage has run out or is about to/).check();
    await dialog.getByRole("button", { name: "Switch AI", exact: true }).click();
    await page.waitForFunction(() => window.__switch && window.__lastTermOpen && window.__lastTermOpen.target === "codex");
    const r = await page.evaluate(() => ({ sw: window.__switch, stopped: window.__stopped, open: window.__lastTermOpen }));
    if (r.sw.confirmed !== true || r.sw.to !== "codex" || r.sw.task !== "T-1" || r.sw.model !== "small-model") throw new Error("switch args: " + JSON.stringify(r.sw));
    if (!r.stopped.includes("term_quick")) throw new Error("the old AI was not stopped before the switch: " + JSON.stringify(r.stopped));
    if (r.open.handoffId !== "hnd_switch" || r.open.model !== "small-model") throw new Error("new AI did not start from the handoff: " + JSON.stringify(r.open));
    await page.evaluate(() => {
      document.querySelector("#quick-host").remove();
      accountQuota.delete("claude-code"); accountQuota.delete("codex");
      window.__terms = window.__terms.filter((t) => t.id !== "term_quick" && t.target !== "codex");
      state.threads = state.threads.filter((t) => t.id !== "term_quick" && t.target !== "codex");
    });
    await page.evaluate(() => loadThreads().then(() => render()));
  });
  await step("a thread that looks stuck says why, offers Stop and Switch, and clears when output resumes", async () => {
    const r = await page.evaluate(async () => {
      const t = { id: "term_stuck", title: "Looping AI", kind: "agent", target: "claude-code", task: "T-1",
        project_id: state.project.id, started_at: 2, running: true };
      window.__terms.push({ ...t });
      state.threads.push(t);
      const none = stuckBanner(t);
      window.__terms.find((x) => x.id === "term_stuck").stuck = "repeating the same line 31 times: trying again";
      await refreshStuck();
      const live = state.threads.find((x) => x.id === "term_stuck");
      const banner = stuckBanner(live);
      const out = { none: !!none, text: banner?.textContent || "", pill: threadStatus(live).textContent, buttons: [...(banner?.querySelectorAll("button") || [])].map((b) => b.textContent) };
      delete window.__terms.find((x) => x.id === "term_stuck").stuck;
      await refreshStuck();
      out.cleared = !stuckBanner(state.threads.find((x) => x.id === "term_stuck"));
      window.__terms = window.__terms.filter((x) => x.id !== "term_stuck");
      state.threads = state.threads.filter((x) => x.id !== "term_stuck");
      return out;
    });
    if (r.none) throw new Error("banner shown for a healthy thread");
    if (!/looks stuck: repeating the same line 31 times/.test(r.text) || !/nothing was stopped/.test(r.text)) throw new Error("banner text: " + r.text);
    if (r.pill !== "stuck?") throw new Error("pill: " + r.pill);
    if (!r.buttons.includes("Stop") || !r.buttons.some((b) => /^Switch/.test(b))) throw new Error("buttons: " + r.buttons);
    if (!r.cleared) throw new Error("banner stayed after output resumed");
  });
  await step("Plan first is offered for a task, and a waiting plan can be approved by a person", async () => {
    await page.evaluate(() => { window.__plan = null; window.__lastTermOpen = null; setTab("workspace"); newThreadDialog("T-1"); });
    const dialog = page.getByRole("dialog", { name: "New agent thread" });
    const box = dialog.getByLabel(/Plan first/);
    await box.check();
    await dialog.getByRole("button", { name: "Start", exact: true }).click();
    await page.waitForFunction(() => window.__lastTermOpen);
    if ((await page.evaluate(() => window.__lastTermOpen.planFirst)) !== true) throw new Error("plan first was not sent");
    // A task with no plan shows no card; a proposed plan shows one with Approve and Reject.
    await page.evaluate(() => { setTab("tasks"); openTask("T-1"); });
    await page.locator("#drawer").getByRole("heading", { name: "Acceptance criteria" }).waitFor({ timeout: 6000 }).catch(() => {});
    if (await page.locator("#drawer .plan-card").count()) throw new Error("plan card shown without a plan");
    await page.evaluate(() => { window.__plan = { task_id: "t", status: "proposed", body: "1. add the flag\n2. test it", note: null, proposed_by: "agent:codex", created_at: "now", decided_at: null }; openTask("T-1"); });
    const card = page.locator("#drawer").getByRole("group", { name: "Plan for T-1" });
    await card.getByText("1. add the flag").waitFor();
    await card.getByText("waiting for you").waitFor();
    await card.getByLabel("Note for the AI").fill("go ahead");
    await card.getByRole("button", { name: "Approve plan" }).click();
    await page.waitForFunction(() => window.__planDecision);
    const d = await page.evaluate(() => window.__planDecision);
    if (d.approve !== true || d.note !== "go ahead" || d.task !== "T-1") throw new Error("decision: " + JSON.stringify(d));
    await card.getByText("approved", { exact: true }).waitFor();
    if (await card.getByRole("button", { name: "Approve plan" }).count()) throw new Error("a decided plan still offers Approve");
    await page.evaluate(() => { window.__plan = null; window.__planDecision = null; });
  });
  await step("Settings can limit and clear saved history, asking before deleting", async () => {
    await page.evaluate(() => { window.__history = null; window.__historyCalls = []; setTab("settings"); });
    const card = page.locator(".card", { has: page.getByRole("heading", { name: "Saved history" }) });
    await card.getByText("3 saved now").waitFor();
    if (process.env.EVYRO_SHOTS) await card.screenshot({ path: path.join(process.env.EVYRO_SHOTS, "history-card.png") });
    await card.getByLabel("Saved threads to keep").fill("501");
    await card.getByRole("button", { name: "Save limits" }).click();
    await page.getByText("Enter whole numbers from 0 to 500.").waitFor();
    if ((await page.evaluate(() => window.__historyCalls)).length) throw new Error("an out-of-range limit was sent");
    await card.getByLabel("Saved threads to keep").fill("1");
    await card.getByRole("button", { name: "Save limits" }).click();
    await card.getByText("1 saved now").waitFor();
    const set = await page.evaluate(() => window.__historyCalls[0]);
    if (set.threadsKeep !== 1 || set.chatsKeep !== 60) throw new Error("limits sent: " + JSON.stringify(set));
    // Clearing asks first; cancelling deletes nothing.
    await card.getByRole("button", { name: "Clear saved chats" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByText(/permanently deletes the saved chat output/).waitFor();
    await dialog.getByRole("button", { name: "Cancel" }).click();
    if ((await page.evaluate(() => window.__historyCalls)).some((c) => c.cmd === "history_clear")) throw new Error("deleted without confirming");
    await card.getByRole("button", { name: "Clear saved chats" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete", exact: true }).click();
    await card.getByText("0 saved now").first().waitFor();
    const clearCall = await page.evaluate(() => window.__historyCalls.find((c) => c.cmd === "history_clear"));
    if (clearCall.what !== "chats") throw new Error("cleared " + clearCall.what);
    if (await card.getByRole("button", { name: "Clear saved chats" }).isEnabled()) throw new Error("nothing left to clear, yet the button is enabled");
    await page.evaluate(() => { setTab("workspace"); });
  });
  await step("Work on next task starts the chosen AI on the chosen task with nothing to pick", async () => {
    await page.evaluate(() => { window.__lastTermOpen = null; setTab("tasks"); });
    await page.getByRole("button", { name: "Work on next task", exact: true }).click();
    await page.waitForFunction(() => window.__lastTermOpen && window.__lastTermOpen.target === "codex" && window.__lastTermOpen.task === "T-9");
    await page.evaluate(() => {
      window.__terms = window.__terms.filter((t) => t.task !== "T-9");
      state.threads = state.threads.filter((t) => t.task !== "T-9");
    });
    await page.evaluate(() => loadThreads().then(() => render()));
  });
  await step("low usage prepares an unsent checkpoint message without replacing an existing draft", async () => {
    await page.evaluate(() => {
      const t = { id: "term_quota_probe", title: "Quota probe", kind: "agent", target: "claude-code", task: "T-1",
        project_id: state.project.id, started_at: 1, running: true };
      window.__terms.push(t);
      state.threads.push(t);
      setCtxOpen(false);
      setTab("workspace"); placeThread(t.id); render();
    });
    const pane = page.locator('.pane[data-id="term_quota_probe"]');
    await pane.locator(".xterm").waitFor();
    await page.fill(".ws-input", "My existing draft");
    await page.evaluate(() => {
      window.__writes = [];
      window.__quotaOverrides = { "claude-code": { state: "ready", windows: [{ remaining: 8 }] } };
      refreshQuota("claude-code", state.project.id, true);
    });
    await pane.getByText(/low on usage/).waitFor();
    if (await page.inputValue(".ws-input") !== "My existing draft") throw new Error("the checkpoint prompt replaced an existing draft");
    await page.fill(".ws-input", "");
    await pane.getByRole("button", { name: "Draft checkpoint prompt" }).click();
    await page.waitForFunction(() => /checkpoint for T-1/i.test(document.querySelector(".ws-input").value));
    await page.fill(".ws-input", "");
    await pane.locator(".pane-title").click();
    await page.evaluate(() => {
      window.__quotaOverrides["claude-code"].windows[0].remaining = 60;
      refreshQuota("claude-code", state.project.id, true);
    });
    await pane.getByText(/low on usage/).waitFor({ state: "detached" });
    await page.evaluate(() => {
      window.__quotaOverrides["claude-code"].windows[0].remaining = 8;
      refreshQuota("claude-code", state.project.id, true);
    });
    await page.waitForFunction(() => /checkpoint for T-1/i.test(document.querySelector(".ws-input").value)
      && workspaceDraft(state.project.id).recipients.join() === "term_quota_probe");
    if (await page.evaluate(() => window.__writes.some((w) => w.id === "term_quota_probe"))) throw new Error("the checkpoint prompt was typed into the terminal");
  });
  await step("task comparisons show both diffs and receipts and retain the preferred result", async () => {
    await page.evaluate(() => { setTab("tasks"); openTask("T-1"); });
    const drawer = page.locator("#drawer");
    await drawer.getByRole("button", { name: "Compare agents" }).click();
    const compare = page.getByRole("dialog", { name: "Compare agents on T-1" });
    await compare.getByLabel("First agent").selectOption("codex");
    await compare.getByLabel("Second agent").selectOption("claude-code");
    await compare.getByRole("button", { name: "Prepare comparison" }).click();
    await compare.getByText("+first candidate").waitFor();
    await compare.getByText("+second candidate").waitFor();
    await compare.getByText("Last check passed").waitFor();
    await compare.getByText("Last check failed").waitFor();
    await compare.getByText("Earlier snapshot").waitFor();
    const second = compare.locator("article").nth(1);
    await compare.getByText("+1 −0").first().waitFor();
    await compare.getByRole("button", { name: "Run tests in both" }).click();
    await page.waitForFunction(() => (window.__verified || []).join() === "T-2,T-3");
    await compare.getByRole("button", { name: "Run tests in both" }).waitFor();
    await second.getByRole("button", { name: "Prefer this result" }).click();
    await second.getByText("Preferred", { exact: true }).waitFor();
    if (process.env.EVYRO_SHOTS) {
      await page.evaluate(() => document.querySelectorAll(".toast").forEach((t) => t.remove()));
      await page.screenshot({ path: path.join(process.env.EVYRO_SHOTS, "comparison.png") });
    }
    await compare.getByRole("button", { name: "Close", exact: true }).click();
    await drawer.getByRole("button", { name: "Compare agents" }).click();
    await second.getByText("Preferred", { exact: true }).waitFor();
    const stagePreview = compare.getByRole("region", { name: "Staging preview" });
    const stageButton = second.getByRole("button", { name: "Preview staging" });
    await page.evaluate(() => { window.__blockStagePreview = true; });
    await stageButton.click();
    await stagePreview.getByRole("alert").getByText(/destination checkout/).waitFor();
    await page.evaluate(() => { window.__blockStagePreview = false; });
    await stageButton.click();
    await stagePreview.getByRole("heading", { name: "Stage changes from T-3?" }).waitFor();
    await stagePreview.getByText("/scratch/my-app").waitFor();
    await stagePreview.getByText("image.bin").waitFor();
    await stagePreview.getByRole("button", { name: "Cancel staging" }).click();
    if (await page.evaluate(() => window.__staged)) throw new Error("Staging happened without confirmation");
    await stageButton.click();
    if (process.env.EVYRO_SHOTS) {
      await stagePreview.getByText("image.bin").waitFor();
      await page.screenshot({ path: path.join(process.env.EVYRO_SHOTS, "comparison-stage.png") });
    }
    await stagePreview.getByRole("button", { name: "Stage changes", exact: true }).click();
    await stagePreview.getByRole("status").getByText("/scratch/my-app").waitFor();
    await page.waitForFunction(() => window.__staged === "T-3");
    await second.getByRole("button", { name: "Start agent" }).click();
    await page.waitForFunction(() => window.__lastTermOpen?.task === "T-3" && window.__lastTermOpen?.target === "claude-code");
    await compare.waitFor({ state: "detached" });
  });
  await step("first-run setup when there are no projects", async () => {
    await page.evaluate(() => { state.projects = []; state.project = null; setTab("overview"); });
    await page.waitForSelector("ol.setup");
    await page.getByRole("button", { name: "Try the sample project" }).waitFor();
  });
  await step("the guided tour ticks each step from what Evyro recorded, and can be dismissed", async () => {
    await page.evaluate((pid) => {
      window.__projectId = pid; window.__tour = null;
      try { localStorage.removeItem("evyro.tour.dismissed"); } catch {}
      state.projects = [{ id: pid, name: "Evyro sample", root_path: "/data/samples/evyro-sample", alias: null, favorite: false, archived: false }];
      state.project = state.projects[0];
      setTab("tasks");
    }, project.id);
    const card = page.getByRole("region", { name: "Guided tour" });
    await card.getByText("0 of 4").waitFor();
    await card.getByRole("button", { name: "Start with…" }).waitFor();
    // A recorded checkpoint ticks its step, nothing else.
    await page.evaluate(() => { window.__tour = { started: true, checkpoint: true, verified: false, handoff: false }; return loadThreads().then(() => setTab("tasks")); });
    await card.getByText("2 of 4").waitFor();
    if (await card.getByRole("button", { name: "Start with…" }).count()) throw new Error("a finished step still offers its button");
    await card.getByRole("button", { name: "Run tests" }).waitFor();
    // Everything done: the closing message and a way into a real project.
    await page.evaluate(() => { window.__tour = { started: true, checkpoint: true, verified: true, handoff: true }; return setTab("tasks"); });
    await card.getByText("You have done the whole loop").waitFor();
    await card.getByRole("button", { name: "Add your own project" }).waitFor();
    await card.getByRole("button", { name: "Dismiss" }).click();
    await card.waitFor({ state: "detached" });
    await page.evaluate(() => setTab("tasks"));
    await page.waitForTimeout(300);
    if (await page.getByRole("region", { name: "Guided tour" }).count()) throw new Error("the tour came back after it was dismissed");
    // A project that is not the sample never shows it.
    await page.evaluate(() => { try { localStorage.removeItem("evyro.tour.dismissed"); } catch {} state.project = { ...state.project, name: "My app", root_path: "/work/app" }; setTab("tasks"); });
    await page.waitForTimeout(300);
    if (await page.getByRole("region", { name: "Guided tour" }).count()) throw new Error("the tour showed on an ordinary project");
    // The button on the welcome screen creates the sample and opens it.
    await page.evaluate(() => { state.projects = []; state.project = null; window.__sampleCalls = 0; setTab("overview"); });
    await page.getByRole("button", { name: "Try the sample project" }).click();
    await page.waitForFunction(() => window.__sampleCalls === 1);
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
