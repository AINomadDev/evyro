// Updates: the Settings card, and one quiet check a few seconds after launch.
// Nothing is downloaded until the person clicks "Install and restart"; the
// app checks the download's signature against its built-in key first.

const AUTO_KEY = "evyro.update.auto";
function autoCheck() {
  try { return localStorage.getItem(AUTO_KEY) !== "off"; } catch { return true; }
}
function setAutoCheck(on) {
  try { localStorage.setItem(AUTO_KEY, on ? "on" : "off"); } catch { /* not kept */ }
}

let lastUpdate = null;

function installUpdateDialog(u) {
  modal(`Install Evyro ${u.version}?`, [
    h("p", null, `You have ${u.current}. Evyro downloads the new version, checks its signature, installs it and restarts. Agents running in this window are stopped; their progress is checkpointed.`),
    u.notes ? h("pre", { class: "small" }, u.notes) : null,
  ], [{
    label: "Install and restart", primary: true, run: async () => {
      toast("Downloading the update…");
      await act(() => invoke("install_update"));
    },
  }]);
}

function updateFooterButton(u) {
  const foot = $("#side-foot");
  const old = foot.querySelector(".update-btn");
  if (old) old.remove();
  if (!u || !u.available) return;
  foot.append(h("button", { class: "btn small primary update-btn", onclick: () => installUpdateDialog(u) }, `Update to ${u.version}`));
}

function updatesCard() {
  const card = h("div", { class: "card" }, h("h2", null, "Updates"), h("div", { class: "muted" }, "Checking…"));
  const draw = (u, err) => {
    const auto = h("input", { type: "checkbox", checked: autoCheck(), onchange: (e) => setAutoCheck(e.target.checked) });
    let status;
    if (err) status = h("div", { class: "banner bad" }, "Could not check for updates: " + errMsg(err));
    else if (!u.configured) status = h("p", { class: "small" }, `This build (${u.current}) cannot update itself. Download new versions from the releases page.`);
    else if (u.available) status = h("div", { class: "stack" },
      h("p", null, h("span", { class: "pill ok" }, "New"), ` Evyro ${u.version} is available. You have ${u.current}.`),
      h("div", { class: "row" }, h("button", { class: "btn primary small", onclick: () => installUpdateDialog(u) }, "Install and restart")));
    else status = h("p", { class: "small" }, `You have the latest version (${u.current}).`);
    clear(card).append(h("h2", null, "Updates"), status,
      h("label", { class: "check small" }, auto, " Check for updates when Evyro starts"),
      h("div", { class: "row", style: "margin-top:12px" }, h("button", { class: "btn small", onclick: run }, "Check now")));
  };
  const run = () => invoke("check_update").then((u) => { lastUpdate = u; updateFooterButton(u); draw(u); }).catch((e) => draw(null, e));
  if (lastUpdate) draw(lastUpdate); else run();
  return card;
}

document.addEventListener("DOMContentLoaded", () => {
  if (!autoCheck()) return;
  setTimeout(() => {
    invoke("check_update").then((u) => {
      lastUpdate = u;
      updateFooterButton(u);
      if (u.available) toast(`Evyro ${u.version} is available. Use the button at the bottom left to install it.`);
    }).catch(() => { /* offline or no release yet: say nothing */ });
  }, 4000);
});
