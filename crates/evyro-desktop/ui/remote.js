// Remote access card on the Settings screen: run `evyro serve` inside the app
// so a paired phone can follow the work and approve this window's chats,
// make pairing codes, and revoke devices.

const SCOPE_HELP = {
  read: "Look only: projects, tasks, changes, chats.",
  approve: "Also answer an agent's permission requests and leave review comments.",
  control: "Also start, message and end chats, and use MCP write tools.",
};

function remoteCard() {
  const card = h("div", { class: "card wide" }, h("h2", null, "Remote access"), h("div", { class: "muted" }, "Loading…"));
  const draw = async () => {
    const [st, devices] = await Promise.all([invoke("remote_status"), invoke("remote_devices")]).catch((e) => { toast(errMsg(e), true); return [{ running: false }, []]; });
    const host = h("input", { type: "text", value: "127.0.0.1" });
    const port = h("input", { type: "number", value: "7420", min: "1", max: "65535" });
    const lan = h("input", { type: "checkbox" });
    clear(card).append(h("h2", null, "Remote access"),
      h("p", { class: "small" }, "Follow the work from your phone or another computer, and approve an agent's requests there. Off until you start it. Only devices you pair can sign in."),
      st.running
        ? h("div", { class: "row" }, h("span", { class: "pill ok" }, "Serving"), h("code", { class: "mono" }, st.url),
          h("span", { class: "muted small" }, st.reach === "loopback" ? "this computer only" : st.reach === "tailscale" ? "your Tailscale network" : "any device on the network (not encrypted)"),
          h("span", { class: "spacer" }),
          h("button", { class: "btn small", onclick: () => copy(st.url, "Address copied") }, "Copy address"),
          h("button", { class: "btn small danger", onclick: async () => { await act(() => invoke("remote_stop"), "Stopped"); draw(); } }, "Stop"))
        : h("div", { class: "stack" },
          h("div", { class: "row" }, field("Address", host, "127.0.0.1 is this computer only. Use your Tailscale IP (100.x.y.z) to reach your phone through Tailscale."), field("Port", port)),
          h("label", { class: "check small" }, lan, " Allow other networks (plain HTTP, not encrypted)"),
          h("div", { class: "row" }, h("button", {
            class: "btn primary small", onclick: async () => {
              await act(() => invoke("remote_start", { host: host.value.trim(), port: Number(port.value) || 7420, allowLan: lan.checked }), "Serving");
              draw();
            },
          }, "Start serving"))),
      h("div", { class: "card-head", style: "margin-top:14px" }, h("h3", null, "Devices"), h("button", { class: "btn small", onclick: pairDialog }, "+ Pair a device")),
      devices.length ? h("ul", { class: "list" }, devices.slice().reverse().map((d) => h("li", null,
        h("span", { class: "pill " + (d.state === "active" ? "ok" : d.state === "revoked" ? "bad" : "") }, d.state),
        h("strong", null, d.name), h("span", { class: "pill" }, d.scope),
        h("span", { class: "muted small" }, d.last_seen_at ? "seen " + ago(d.last_seen_at) + (d.last_address ? " from " + d.last_address : "") : "never used"),
        h("span", { class: "spacer" }),
        d.state === "active" || d.state === "waiting" ? h("button", { class: "btn small danger", onclick: async () => { await act(() => invoke("remote_revoke", { id: d.id }), "Revoked"); draw(); } }, "Revoke") : null)))
        : h("div", { class: "empty" }, "No devices yet."));
  };
  const pairDialog = () => {
    const name = h("input", { type: "text", placeholder: "My phone" });
    const scope = h("select", null, ["approve", "read", "control"].map((s) => h("option", { value: s }, s)));
    const help = h("div", { class: "hint" }, SCOPE_HELP.approve);
    scope.addEventListener("change", () => { help.textContent = SCOPE_HELP[scope.value]; });
    modal("Pair a device", [field("Name", name), field("Access", scope), help], [{
      label: "Make a code", primary: true, run: async () => {
        if (!name.value.trim()) { toast("Name the device", true); return false; }
        const p = await act(() => invoke("remote_pair", { name: name.value.trim(), scope: scope.value, minutes: 10 }));
        modal("Pairing code", [
          h("div", { class: "pair-code" }, p.code),
          h("p", { class: "small" }, `On "${p.device.name}", open the address shown under Remote access and enter this code. It works once and expires in 10 minutes.`)], []);
        draw();
        return false; // keep the code dialog open
      },
    }]);
  };
  draw();
  return card;
}
