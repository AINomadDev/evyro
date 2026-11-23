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
          if (cmd === "term_write") { setTimeout(() => window.__emit("term-output", { id: args.id, data: args.data, end: 0 }), 10); return null; }
          if (cmd === "term_close") { window.__terms = terms.filter((t) => t.id !== args.id); return null; }
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
    catch (e) { errors.push(`${name}: ${e.message.split("\n")[0]}`); console.log("FAIL  " + name); }
  };
  const tab = (v) => page.click(`.tab[data-view="${v}"]`);

  await step("overview shows the active task", async () => {
    await page.waitForSelector("text=Add rate limiting");
    await page.waitForSelector("text=Handoff waiting");
  });
  await step("tasks board", async () => { await tab("tasks"); await page.waitForSelector(".task-card"); });
  await step("changes view renders the diff", async () => {
    await tab("changes");
    await page.waitForSelector("pre.diff .add");
    if ((await page.textContent("pre.diff .add")).trim() !== "+two") throw new Error("diff line");
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
  await step("settings", async () => { await tab("settings"); await page.waitForSelector("text=Doctor"); });
  await step("command palette filters and runs", async () => {
    await page.keyboard.press("Control+k");
    await page.waitForSelector(".palette input");
    await page.keyboard.type("go to tasks");
    await page.keyboard.press("Enter");
    await page.waitForSelector('.tab.active[data-view="tasks"]');
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
