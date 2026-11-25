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
  modal(t("update.installTitle", { version: u.version }), [
    h("p", null, t("update.installBody", { current: u.current })),
    u.notes ? h("pre", { class: "small" }, u.notes) : null,
  ], [{
    label: t("update.install"), primary: true, run: async () => {
      toast(t("update.downloading"));
      await act(() => invoke("install_update"));
    },
  }]);
}

function updateFooterButton(u) {
  const foot = $("#side-foot");
  const old = foot.querySelector(".update-btn");
  if (old) old.remove();
  if (!u || !u.available) return;
  foot.append(h("button", { class: "btn small primary update-btn", onclick: () => installUpdateDialog(u) }, t("update.footer", { version: u.version })));
}

function updatesCard() {
  const card = h("div", { class: "card" }, h("h2", null, t("update.title")), h("div", { class: "muted" }, t("update.checking")));
  const draw = (u, err) => {
    const auto = h("input", { type: "checkbox", checked: autoCheck(), onchange: (e) => setAutoCheck(e.target.checked) });
    let status;
    if (err) status = h("div", { class: "banner bad" }, t("update.checkFailed", { error: errMsg(err) }));
    else if (!u.configured) status = h("p", { class: "small" }, t("update.cannotSelf", { current: u.current }));
    else if (u.available) status = h("div", { class: "stack" },
      h("p", null, h("span", { class: "pill ok" }, t("update.new")), " " + t("update.available", { version: u.version, current: u.current })),
      h("div", { class: "row" }, h("button", { class: "btn primary small", onclick: () => installUpdateDialog(u) }, t("update.install"))));
    else status = h("p", { class: "small" }, t("update.latest", { current: u.current }));
    clear(card).append(h("h2", null, t("update.title")), status,
      h("label", { class: "check small" }, auto, " " + t("update.auto")),
      h("div", { class: "row", style: "margin-top:12px" }, h("button", { class: "btn small", onclick: run }, t("update.checkNow"))));
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
      if (u.available) toast(t("update.toast", { version: u.version }));
    }).catch(() => { /* offline or no release yet: say nothing */ });
  }, 4000);
});
