// Remote access card on the Settings screen: run `evyro serve` inside the app
// so a paired phone can follow the work and approve this window's chats,
// make pairing codes, change a device's access or expiry, read what devices
// did (the audit log), and revoke devices.

const scopeHelp = () => ({ read: t("remote.scope.read.help"), approve: t("remote.scope.approve.help"), control: t("remote.scope.control.help") });
const scopeName = (s) => ({ read: t("remote.scope.read"), approve: t("remote.scope.approve"), control: t("remote.scope.control") })[s] || s;
const deviceState = (s) => ({ active: t("remote.state.active"), revoked: t("remote.state.revoked"), waiting: t("remote.state.waiting"), expired: t("remote.state.expired") })[s] || s;
const auditAction = (a) => ({ pair_code: t("remote.audit.pairCode"), paired: t("remote.audit.paired"), revoked: t("remote.audit.revoked"), scope: t("remote.audit.scope"), expiry: t("remote.audit.expiry"), api: t("remote.audit.api"), refused: t("remote.audit.refused") })[a] || a;
const when = (iso) => new Date(iso).toLocaleString();
// Expiry choices in days; 0 is no expiry.
const EXPIRY_DAYS = [0, 1, 7, 30, 90, 365];
const expiryName = (days) => (days ? t("remote.expiry.days", { count: days }) : t("remote.expiry.never"));
const expirySelect = (current = 0) => h("select", { "aria-label": t("remote.expiry") }, EXPIRY_DAYS.map((d) => h("option", { value: String(d), selected: d === current }, expiryName(d))));
const expiryText = (d) => !d.expires_at ? t("remote.expiry.none") : d.state === "expired" ? t("remote.expiry.ended", { when: when(d.expires_at) }) : t("remote.expiry.until", { when: when(d.expires_at) });

// The audit log, newest first: one device's (id) or every device's.
async function auditDialog(id, name) {
  const log = await act(() => invoke("remote_audit", { id: id || null, limit: 200 }));
  modal(name ? t("remote.audit.titleFor", { device: name }) : t("remote.audit.title"), [
    h("p", { class: "small" }, t("remote.audit.help")),
    log.length ? h("ul", { class: "list audit-log" }, log.map((e) => h("li", null,
      h("span", { class: "pill " + (e.action === "refused" ? "bad" : e.action === "revoked" ? "warn" : "") }, auditAction(e.action)),
      name ? null : h("strong", null, e.device_name || ""),
      e.detail ? h("code", { class: "mono small" }, e.detail) : null,
      h("span", { class: "spacer" }),
      h("span", { class: "muted small" }, e.address ? t("remote.audit.byFrom", { actor: e.actor, address: e.address, when: when(e.at) }) : t("remote.audit.by", { actor: e.actor, when: when(e.at) })))))
      : h("div", { class: "empty" }, t("remote.audit.empty"))], []);
}

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
        h("span", { class: "spacer" }),
        h("button", { class: "btn small", onclick: () => auditDialog() }, t("remote.audit.all")),
        h("button", { class: "btn small", onclick: () => pairDialog(remoteAddress || st.url, !!remoteAddress) }, t("remote.pairBtn"))),
      devices.length ? h("ul", { class: "list" }, devices.slice().reverse().map((d) => {
        const live = d.state !== "revoked";
        return h("li", null,
          h("span", { class: "pill " + (d.state === "active" ? "ok" : d.state === "revoked" || d.state === "expired" ? "bad" : "") }, deviceState(d.state)),
          h("strong", null, d.name), live ? scopePicker(d) : h("span", { class: "pill" }, scopeName(d.scope)),
          h("span", { class: "muted small" }, d.last_seen_at ? (d.last_address ? t("remote.seenFrom", { ago: ago(d.last_seen_at), address: d.last_address }) : t("remote.seen", { ago: ago(d.last_seen_at) })) : t("remote.never")),
          live ? h("span", { class: "muted small" }, expiryText(d)) : null,
          h("span", { class: "spacer" }),
          h("button", { class: "btn small", "aria-label": t("remote.audit.forLabel", { device: d.name }), onclick: () => auditDialog(d.id, d.name) }, t("remote.audit.open")),
          live ? h("button", { class: "btn small", "aria-label": t("remote.expiry.changeLabel", { device: d.name }), onclick: () => expiryDialog(d) }, t("remote.expiry.change")) : null,
          live ? h("button", { class: "btn small danger", onclick: async () => { await act(() => invoke("remote_revoke", { id: d.id }), t("remote.revoked")); draw(); } }, t("remote.revoke")) : null);
      }))
        : h("div", { class: "empty" }, t("remote.noDevices")));
  };
  // Change a device's access in place; it applies to its next request.
  const scopePicker = (d) => {
    const sel = h("select", { class: "small", "aria-label": t("remote.scope.changeLabel", { device: d.name }) },
      ["read", "approve", "control"].map((s) => h("option", { value: s, selected: s === d.scope }, scopeName(s))));
    sel.addEventListener("change", async () => {
      try { await act(() => invoke("remote_set", { id: d.id, scope: sel.value, expiresDays: null }), t("remote.scope.changed", { device: d.name, scope: scopeName(sel.value) })); }
      catch { /* toast already shown */ }
      finally { draw(); }
    });
    return sel;
  };
  const expiryDialog = (d) => {
    const pick = expirySelect(0);
    modal(t("remote.expiry.title", { device: d.name }), [h("p", { class: "small" }, expiryText(d)), field(t("remote.expiry.new"), pick, t("remote.expiry.help"))], [{
      label: t("remote.expiry.save"), primary: true, run: async () => {
        await act(() => invoke("remote_set", { id: d.id, scope: null, expiresDays: Number(pick.value) }), t("remote.expiry.saved"));
        draw();
      },
    }]);
  };
  const pairDialog = (address, privateRoute) => {
    const name = h("input", { type: "text", placeholder: t("remote.pair.ph") });
    const scope = h("select", null, ["approve", "read", "control"].map((s) => h("option", { value: s }, scopeName(s))));
    const help = h("div", { class: "hint" }, scopeHelp().approve);
    scope.addEventListener("change", () => { help.textContent = scopeHelp()[scope.value]; });
    const expires = expirySelect(30);
    modal(t("remote.pair.title"), [address ? h("code", { class: "pair-address" }, address) : null, field(t("remote.pair.name"), name), field(t("remote.pair.access"), scope), help,
      field(t("remote.expiry"), expires, t("remote.expiry.pairHelp"))], [{
      label: t("remote.pair.make"), primary: true, run: async () => {
        if (!name.value.trim()) { toast(t("remote.pair.nameIt"), true); return false; }
        if (privateRoute) {
          const route = await act(() => invoke("remote_tailscale", { enable: false }));
          if (route.state !== "ready" || route.url !== address) {
            toast(t("remote.pair.changed"), true);
            return false;
          }
        }
        const p = await act(() => invoke("remote_pair", { name: name.value.trim(), scope: scope.value, minutes: 10, expiresDays: Number(expires.value) || null }));
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
