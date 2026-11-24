// Run after cargo build -p evyro-cli. Requires Playwright and Chromium.
//   node crates/evyro-remote/tests/companion-smoke.cjs
"use strict";
const assert = require("node:assert/strict");
const { spawn, execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { chromium, devices } = require("playwright");
const root = path.resolve(__dirname, "../../..");
const bin = process.env.EVYRO_BIN || path.join(root, "target/debug", process.platform === "win32" ? "evyro.exe" : "evyro");
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "evyro-companion-"));
const repo = path.join(dir, "Companion audit");
fs.mkdirSync(repo);
const env = { ...process.env, EVYRO_HOME: path.join(dir, "home"), EVYRO_NO_APP_LOOKUP: "1",
  EVYRO_ACP_BIN_CODEX: path.join(root, "crates/evyro-acp/tests/fake_acp_agent.py"), FAKE_ACP_LOG: path.join(dir, "acp.log") };
const run = (cmd, ...args) => execFileSync(cmd, args, { env, cwd: repo, encoding: "utf8" });
const ev = (...args) => run(bin, ...args);
if (process.platform === "win32") {
  const script = env.EVYRO_ACP_BIN_CODEX;
  env.EVYRO_ACP_BIN_CODEX = path.join(dir, "fake-acp.cmd");
  fs.writeFileSync(env.EVYRO_ACP_BIN_CODEX, `@python "${script}" %*\r\n@exit /b %ERRORLEVEL%\r\n`);
}
fs.writeFileSync(path.join(repo, "README.md"), "# Companion audit\n");
fs.writeFileSync(path.join(repo, "notes.txt"), "first line\n");
run("git", "init", "-q", "-b", "main");
run("git", "add", ".");
run("git", "-c", "user.email=audit@example.test", "-c", "user.name=Audit", "commit", "-qm", "base");
fs.appendFileSync(path.join(repo, "notes.txt"), "second line\n");
ev("init");
ev("task", "add", "Verify companion browser", "--ready", "--accept", "Can browse on a narrow screen");
ev("task", "start", "T-1", "--agent", "audit");
ev("checkpoint", "--task", "T-1", "--done", "Prepared scratch checkout");
ev("handoff", "--task", "T-1", "--to", "codex", "--next", "Review audit project");
const serverFor = (host) => {
  const child = spawn(bin, ["--json", "serve", "--host", host, "--port", "0"], { env, cwd: repo });
  const address = new Promise((res, rej) => {
    let line = "";
    const timer = setTimeout(() => rej(new Error("Server startup timed out")), 5000);
    child.once("error", rej);
    child.once("exit", (code) => { clearTimeout(timer); rej(new Error("Server exited " + code)); });
    child.stdout.on("data", (buf) => {
      line += buf;
      if (!line.includes("\n")) return;
      clearTimeout(timer);
      try { res(JSON.parse(line.split("\n")[0])); } catch (e) { rej(e); }
    });
  });
  return [child, address];
};
(async () => {
  const [server, address] = serverFor("127.0.0.1");
  let browser;
  const errors = [];
  try {
    const {url} = await address;
    browser = await chromium.launch();
    const context = await browser.newContext({ ...devices["iPhone SE"], viewport: { width: 360, height: 740 } });
    const page = await context.newPage();
    page.setDefaultTimeout(6500);
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(url);
    const pair = JSON.parse(ev("--json", "remote", "pair", "--name", "Scratch phone", "--scope", "control"));
    const input = page.getByLabel("Pairing code");
    await input.fill(pair.code);
    await page.getByRole("button", {name:"Pair", exact:true}).click();
    await page.getByText("Signed in as Scratch phone (control access).").waitFor();
    console.log("PASS browser pairing");
    const project = page.getByRole("button", {name:/Companion audit/});
    await project.press("Enter");
    await page.getByRole("button", {name:"Tasks", exact:true}).click();
    let releaseTask;
    let taskRequested;
    const requested = new Promise(resolve => { taskRequested = resolve; });
    const blocked = new Promise(resolve => { releaseTask = resolve; });
    const endpoint = "**/api/v1/projects/*/tasks/T-1";
    await page.route(endpoint, async (request) => {
      taskRequested();
      await blocked;
      await request.continue();
    }, { times: 1 });
    const task = page.getByRole("button", { name: /T-1 Verify companion browser/ });
    await task.press("Enter");
    await requested;
    await page.getByRole("button", { name: "Back" }).click();
    await task.waitFor();
    const slowResponse = page.waitForResponse(r => r.url().endsWith("/tasks/T-1"));
    releaseTask();
    await (await slowResponse).finished();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await page.getByRole("heading", { name: "Acceptance criteria" }).count(), 0);
    await task.press("Enter");
    await page.getByText("Can browse on a narrow screen").waitFor();
    await page.getByText("Prepared scratch checkout", {exact:false}).last().waitFor();
    await page.getByRole("button", {name:"Back"}).click();
    await page.getByRole("button", {name:"Sessions", exact:true}).click();
    await page.getByText("audit", {exact:true}).waitFor();
    await page.getByRole("button", {name:"Handoffs", exact:true}).click();
    await page.getByText("audit → codex", {exact:true}).waitFor();
    await page.getByRole("button", {name:"Changes", exact:true}).click();
    await page.getByText("+second line", {exact:true}).waitFor();
    const review = page.getByRole("button", {name:/Comment on notes\.txt.*2/});
    await review.press("Enter");
    await page.getByRole("textbox", {name: "Review comment"}).fill("Checked on the companion.");
    await page.getByRole("button", {name:"Add comment", exact:true}).click();
    await page.getByText(/Comment saved/).waitFor();
    assert(ev("review").includes("Checked on the companion."));
    assert(await page.evaluate(() => document.documentElement.scrollWidth) <= page.viewportSize().width);
    console.log("PASS project, tasks, checkpoint, sessions, handoff and diff comment at phone width");
    await page.getByRole("button", {name:"Back"}).click();
    await page.getByRole("button", {name:"+ New chat", exact:true}).click();
    await page.getByLabel("Agent").selectOption("codex");
    await page.getByRole("button", {name:"Start chat", exact:true}).click();
    await page.getByRole("textbox", {name: "Message the agent"}).fill("Please write the notes");
    await page.getByRole("button", {name:"Send", exact:true}).click();
    await page.getByRole("button", {name:"Allow", exact:true}).waitFor();
    assert(await page.evaluate(() => document.documentElement.scrollWidth) <= page.viewportSize().width, "Approval view has horizontal overflow");
    const sendLabel = await page.getByRole("button", {name:"Send", exact:true}).boundingBox();
    assert(sendLabel.height <= 46, "Send button wraps onto two lines");
    await page.waitForTimeout(300);
    const reject = await page.getByRole("button", {name:"Reject", exact:true}).boundingBox();
    const composerTop = (await page.getByRole("textbox", {name: "Message the agent"}).boundingBox()).y;
    assert(reject && reject.y >= 0 && reject.y + reject.height <= composerTop, "Reject is hidden behind the composer or off screen");
    if (process.env.EVYRO_SHOTS) {
      await page.locator("#toast").waitFor({ state: "hidden" });
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.screenshot({ path: path.join(process.env.EVYRO_SHOTS, "remote-approval.png"), fullPage: true });
    }
    await page.getByRole("button", {name:"Allow", exact:true}).click();
    await page.getByText(/Answered:.*Scratch phone/).waitFor();
    await page.locator(".msg.agent").filter({ hasText: /Wrote notes\.txt\./ }).waitFor();
    assert.equal(fs.readFileSync(path.join(repo, "notes.txt"), "utf8"), "written by the fake agent\n");
    assert(await page.evaluate(() => document.documentElement.scrollWidth) <= page.viewportSize().width);
    page.once("dialog", (d) => d.accept());
    await page.getByRole("button", {name:"End chat", exact:true}).click();
    await page.getByText(/Signed in as Scratch phone/).waitFor();
    console.log("PASS chat launch, live reply/permission, approval and close");
    await page.reload();
    await page.getByText(/Signed in as Scratch phone/).waitFor();
    ev("remote", "revoke", "Scratch phone");
    await page.reload();
    await page.getByLabel("Pairing code").waitFor();
    console.log("PASS reload preserves pairing and revocation returns to pairing");
    assert.deepEqual(errors, []);
    if (process.env.EVYRO_CHECK_TAILSCALE === "1") {
      const host = run("tailscale", "ip", "-4").trim();
      const [tail, ready] = serverFor(host);
      try {
        const {url: privateUrl} = await ready;
        assert.equal((await (await fetch(privateUrl + "/api/v1/health")).json()).ok, true);
        console.log("PASS real Tailscale-address listener and health response from this Mac");
      } finally { tail.kill("SIGTERM"); }
    }
  } finally {
    await browser?.close();
    server.kill("SIGTERM");
    fs.rmSync(dir, { recursive: true, force: true });
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
