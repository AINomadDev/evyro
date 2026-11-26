// Remote access card on the Settings screen: run `evyro serve` inside the app
// so a paired phone can follow the work and approve this window's chats,
// make pairing codes, and revoke devices.

const scopeHelp = () => ({ read: t("remote.scope.read.help"), approve: t("remote.scope.approve.help"), control: t("remote.scope.control.help") });
const scopeName = (s) => ({ read: t("remote.scope.read"), approve: t("remote.scope.approve"), control: t("remote.scope.control") })[s] || s;
const deviceState = (s) => ({ active: t("remote.state.active"), revoked: t("remote.state.revoked"), waiting: t("remote.state.waiting") })[s] || s;

function remoteCard() {
  const card = h("div", { class: "card wide" }, h("h2", null, t("remote.title")), h("div", { class: "muted" }, t("remote.loading")));
  let remoteAddress = null;
  const away = () => {
    const result = h("div", { class: "small", role: "status" });
    const detect = async (enable = false) => {
      check.disabled = true;
      result.textContent = t("remote.ts.checking");
      remoteAddress = null;
      try {
        const r = await invoke("remote_tailscale", { enable });
        const command = `tailscale serve --bg --https=${r.https_port} http://127.0.0.1:${r.port}`;
        remoteAddress = r.state === "ready" ? r.url : null;
        clear(result).append(remoteAddress
          ? h("div", { class: "row" }, h("code", { class: "pair-address" }, r.url),
              h("button", { class: "btn small", onclick: () => copy(r.url, t("remote.ts.copied")) }, t("remote.ts.copy")),
              h("button", { class: "btn small primary", onclick: () => pairDialog(r.url, true) }, t("remote.ts.pair")))
          : r.state === "setup" ? h("div", { class: "stack" },
              h("p", null, r.feedback || t("remote.ts.setup")),
              h("div", { class: "row" }, h("button", { class: "btn small primary", onclick: () => detect(true) }, t("remote.ts.enable")), h("button", { class: "btn small", onclick: () => copy(command, t("remote.ts.cmdCopied")) }, t("remote.ts.copyCmd"))))
            : ({ start_loopback: t("remote.ts.state.loopback"), missing: t("remote.ts.state.missing"), disconnected: t("remote.ts.state.disconnected"), unavailable: t("remote.ts.state.unavailable"), public: t("remote.ts.state.public"), ports_busy: t("remote.ts.state.ports") })[r.state]);
      } catch (e) { result.textContent = errMsg(e); }
      finally { check.disabled = false; }
    };
    const check = h("button", { class: "btn small", onclick: () => detect() }, t("remote.ts.check"));
    return h("div", { class: "stack" }, h("h3", null, t("remote.away")),
      h("p", { class: "small" }, t("remote.away.help")),
      h("div", null, check), result);
  };
  const draw = async () => {
    const [st, devices] = await Promise.all([invoke("remote_status"), invoke("remote_devices")]).catch((e) => { toast(errMsg(e), true); return [{ running: false }, []]; });
    const host = h("input", { type: "text", value: "127.0.0.1" });
    const port = h("input", { type: "number", value: "7420", min: "1", max: "65535" });
    const lan = h("input", { type: "checkbox" });
    clear(card).append(h("h2", null, t("remote.title")),
      h("p", { class: "small" }, t("remote.intro")),
      st.running
        ? h("div", { class: "row" }, h("span", { class: "pill ok" }, t("remote.serving")), h("code", { class: "mono" }, st.url),
          h("span", { class: "muted small" }, st.reach === "loopback" ? t("remote.reach.loopback") : st.reach === "tailscale" ? t("remote.reach.tailscale") : t("remote.reach.lan")),
          h("span", { class: "spacer" }),
          h("button", { class: "btn small", onclick: () => copy(st.url, t("remote.addrCopied")) }, t("remote.copyAddr")),
          h("button", { class: "btn small danger", onclick: async () => { await act(() => invoke("remote_stop"), t("remote.stopped")); remoteAddress = null; draw(); } }, t("remote.stop")))
        : h("div", { class: "stack" },
          h("div", { class: "row" }, field(t("remote.address"), host, t("remote.address.help")), field(t("remote.port"), port)),
          h("label", { class: "check small" }, lan, " " + t("remote.lan")),
          h("div", { class: "row" }, h("button", {
            class: "btn primary small", onclick: async () => {
              await act(() => invoke("remote_start", { host: host.value.trim(), port: Number(port.value) || 7420, allowLan: lan.checked }), t("remote.serving"));
              draw();
            },
          }, t("remote.start")))),
      away(),
      h("div", { class: "card-head", style: "margin-top:14px" }, h("h3", null, t("remote.devices")),
        h("button", { class: "btn small", onclick: () => pairDialog(remoteAddress || st.url, !!remoteAddress) }, t("remote.pairBtn"))),
      devices.length ? h("ul", { class: "list" }, devices.slice().reverse().map((d) => h("li", null,
        h("span", { class: "pill " + (d.state === "active" ? "ok" : d.state === "revoked" ? "bad" : "") }, deviceState(d.state)),
        h("strong", null, d.name), h("span", { class: "pill" }, scopeName(d.scope)),
        h("span", { class: "muted small" }, d.last_seen_at ? (d.last_address ? t("remote.seenFrom", { ago: ago(d.last_seen_at), address: d.last_address }) : t("remote.seen", { ago: ago(d.last_seen_at) })) : t("remote.never")),
        h("span", { class: "spacer" }),
        d.state === "active" || d.state === "waiting" ? h("button", { class: "btn small danger", onclick: async () => { await act(() => invoke("remote_revoke", { id: d.id }), t("remote.revoked")); draw(); } }, t("remote.revoke")) : null)))
        : h("div", { class: "empty" }, t("remote.noDevices")));
  };
  const pairDialog = (address, privateRoute) => {
    const name = h("input", { type: "text", placeholder: t("remote.pair.ph") });
    const scope = h("select", null, ["approve", "read", "control"].map((s) => h("option", { value: s }, scopeName(s))));
    const help = h("div", { class: "hint" }, scopeHelp().approve);
    scope.addEventListener("change", () => { help.textContent = scopeHelp()[scope.value]; });
    modal(t("remote.pair.title"), [address ? h("code", { class: "pair-address" }, address) : null, field(t("remote.pair.name"), name), field(t("remote.pair.access"), scope), help], [{
      label: t("remote.pair.make"), primary: true, run: async () => {
        if (!name.value.trim()) { toast(t("remote.pair.nameIt"), true); return false; }
        if (privateRoute) {
          const route = await act(() => invoke("remote_tailscale", { enable: false }));
          if (route.state !== "ready" || route.url !== address) {
            toast(t("remote.pair.changed"), true);
            return false;
          }
        }
        const p = await act(() => invoke("remote_pair", { name: name.value.trim(), scope: scope.value, minutes: 10 }));
        modal(t("remote.code.title"), [
          h("div", { class: "pair-code" }, p.code),
          h("p", { class: "small" }, address ? t("remote.code.help", { device: p.device.name }) : t("remote.code.helpNoAddr", { device: p.device.name })),
          address ? h("code", { class: "pair-address" }, address) : null,
          h("div", { class: "row" }, address ? h("button", { class: "btn small", onclick: () => copy(address, t("remote.addrCopied")) }, t("remote.copyAddr")) : null,
            h("button", { class: "btn small", onclick: () => copy(p.code, t("remote.code.copied")) }, t("remote.code.copy")))], []);
        draw();
        return false; // keep the code dialog open
      },
    }]);
  };
  draw();
  return card;
}
