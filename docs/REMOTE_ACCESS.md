# Remote access

Follow your coding agents from a phone or another computer, and answer their permission requests there. Remote access is off until you start it, and only devices you pair can sign in.

## What you get

- **A phone companion.** Open the address on your phone. You see your projects, tasks, recent sessions and handoffs, the changes on disk (with secrets hidden), and every chat with an agent. When an agent asks before it edits a file or runs a command, the request shows on the phone with the exact action, and you can allow or reject it.
- **A JSON API** under `/api/v1` for scripts and other tools.
- **An event feed** so clients see chat replies and permission requests as they happen.
- **MCP over HTTP** at `/mcp/<project>`, so an agent on another machine can read Evyro's context and checkpoint.

## Start it

There are two ways. Use one, not both, on the same port.

| Where | How | Chats it can see |
|---|---|---|
| Desktop app | Settings → Remote access → **Start serving** | The chats open in the app window |
| Terminal | `evyro serve` | Chats started from a paired device |

Both use the same address rules, the same paired devices, and the same audit log.

```bash
# This computer only (the default)
evyro serve

# Reach your phone through Tailscale (encrypted)
evyro serve --host 100.101.102.103

# Any network, plain HTTP (not encrypted; avoid)
evyro serve --host 0.0.0.0 --allow-lan
```

`evyro serve` runs until you press Ctrl-C. With `--json` it prints the address, how far it reaches, and the API version first.

## Pair a device

1. On the computer, make a code. In the app: Settings → Remote access → **+ Pair a device**. In a terminal:

   ```bash
   evyro remote pair --name "My phone" --scope approve --expires 30d
   ```

2. On the device, open the address and type the code. Case, spaces and dashes don't matter.
3. The device keeps a long random token and signs in with it from then on.

A code works once and expires after 10 minutes (`--minutes` sets 1 to 60). Make a new code if it expires.

A device can also be given an expiry (`--expires 12h`, `30d`, `2w`; `never` is the default; the app offers a choice when pairing). After it, the device is refused and listed as **expired** until you extend it or pair it again.

```bash
evyro remote devices                          # every device, its access, expiry, and when it was last seen
evyro remote set "My phone" --scope read      # change its access without pairing again
evyro remote set "My phone" --expires 7d      # a new expiry from now (or never); also extends an expired device
evyro remote audit "My phone"                 # what it did and what was done to it, newest first
evyro remote revoke "My phone"                # by name, id, or the first 6+ characters of the id
```

A revoked device's token stops working on its next request. A scope or expiry change applies to the device's next request too; an MCP session it started with write tools ends when it loses `control`. In the app, each device under Settings → Remote access has an access menu, **Expiry** and **Activity**, and **Activity log** shows every device.

## Access levels

Pick the lowest level that does the job.

| Scope | Can do |
|---|---|
| `read` | See projects, tasks, sessions, handoffs, the changes on disk, review comments, chats and the feed. Use MCP with read tools only. |
| `approve` | Everything in `read`, plus answer an agent's permission requests, stop a running turn, and leave review comments. This is the default. |
| `control` | Everything in `approve`, plus start, message and close chats, and use MCP with write tools (checkpoint, record decisions, and so on). |

## Security model

- **Off by default.** Nothing listens until you start it.
- **Safe addresses only.** Evyro binds to `127.0.0.1` unless you choose otherwise. A Tailscale address (`100.64.0.0/10` or `fd7a:115c:a1e0::/48`) is allowed because Tailscale encrypts the traffic. Any other address is refused unless you pass `--allow-lan` (or tick **Allow other networks** in the app), because plain HTTP is not encrypted.
- **Pairing, not passwords.** Codes are 8 characters from an alphabet without look-alike characters. Tokens are `evr_` followed by 64 hex characters. Evyro stores only SHA-256 hashes of both, so a copy of the database can't be used to sign in.
- **Exact approvals.** An approval must name the waiting request and its tool call, and pick one of the options the agent offered. A stale or mismatched approval is refused (409), and an option the agent didn't offer is refused (400). Who answered is recorded as `remote:<device name>`.
- **Rate limits.** After 10 failed sign-ins or pairing attempts from one address within 10 minutes, that address is refused until the oldest failure is 10 minutes old. After 60 failures from anywhere within a minute, every sign-in is refused for up to a minute, paired devices included. Refused requests get 429.
- **Audit log.** Every request is written to `logs/remote-audit.log` in Evyro's data folder: time, address, device, method, path and status. Each device also has an audit log in the database (`evyro remote audit`, or **Activity** in the app): pairing, revocation, scope and expiry changes with who made them (`cli:<user>`, `desktop:<user>` or `device:<id>`), every change it made through the API, and every call or sign-in it was refused. Query strings, request bodies, codes and tokens are never written to either.
- **Expiry.** A device past its expiry is refused like a revoked one until someone on the computer extends it.
- **Secrets stay hidden.** Diffs pass through the same redaction as handoffs and exports.
- **Locked-down pages.** Every response sets a strict Content-Security-Policy and `Cache-Control: no-store`. Requests larger than 1 MB are refused.

## Use it from your phone away from home

Tailscale lets your phone reach the Mac over mobile data or another Wi-Fi network. Evyro keeps running on the Mac and uses its own device pairing in addition to Tailscale access.

1. Connect the Mac and phone to the same Tailscale network. Keep the Mac powered on, awake, and online while you are away.
2. In the desktop app, open **Settings → Remote access**, leave the address at `127.0.0.1`, and select **Start serving**.
3. Under **Away from home**, choose **Check private address**. Select **Enable private address** if needed. If Tailscale requires HTTPS setup in your browser, use **Copy setup command** and run it in Terminal. For example, when port 8443 is free, run `tailscale serve --bg --https=8443 http://127.0.0.1:7420`.
4. Select **Pair over Tailscale**, name the phone, and choose its access. Open the private `https://…ts.net` address shown with the code, including its port, on your phone. Enter the one-time code within 10 minutes and bookmark the address. Test on mobile data before leaving. Use **approve** to answer permission requests or **control** to start and message ACP chats.

Use Tailscale Serve for a private listener. Evyro flags a matching public Funnel route and looks for a free HTTPS port when other services already use Tailscale. The phone companion offers ACP chat control, approvals, project status, and diffs. Native terminal panes continue running on the Mac.

On a phone you can add the page to your home screen. It opens like an app.

## API

All endpoints except `health` and `pair` need `Authorization: Bearer evr_...`. Replies are JSON. Errors look like `{"error": {"code": 409, "message": "..."}}`, with the same status code on the response.

| Method and path | Scope | What it does |
|---|---|---|
| `GET /api/v1/health` | none | Is the server up, Evyro's version and the API version |
| `POST /api/v1/pair` | none | `{"code": "..."}` → `{"token", "device"}` |
| `GET /api/v1/me` | read | This device, the API version and Evyro's version |
| `GET /api/v1/projects` | read | Registered projects |
| `GET /api/v1/projects/<p>/overview` | read | Active task and who holds it, git state, health, pending handoff, active sessions, recent events |
| `GET /api/v1/projects/<p>/tasks` | read | Tasks |
| `GET /api/v1/projects/<p>/tasks/<key>` | read | One task |
| `GET /api/v1/projects/<p>/sessions` | read | Recent sessions |
| `GET /api/v1/projects/<p>/handoffs` | read | Handoffs |
| `GET /api/v1/projects/<p>/context` | read | The context pack an agent would get |
| `GET /api/v1/projects/<p>/changes` | read | Changes on disk, secrets hidden |
| `GET /api/v1/projects/<p>/review` | read | Open and sent review comments |
| `POST /api/v1/projects/<p>/review` | approve | `{"path", "line", "body"}` adds a review comment |
| `GET /api/v1/agents` | read | Agents that can chat, and how to install them |
| `GET /api/v1/chats` | read | Chats and their state |
| `POST /api/v1/chats` | control | `{"project", "target", "task"?, "handoff"?}` starts a chat |
| `GET /api/v1/chats/<id>?after=<seq>` | read | A chat's log and its waiting permission requests |
| `POST /api/v1/chats/<id>/messages` | control | `{"text"}` sends the next message |
| `POST /api/v1/chats/<id>/permission` | approve | `{"request", "tool_call_id", "option_id"}` answers a request; leave out `option_id` to reject |
| `POST /api/v1/chats/<id>/cancel` | approve | Stops the current turn |
| `POST /api/v1/chats/<id>/close` | control | Ends the chat and checkpoints its session |
| `GET /api/v1/feed?since=<n>&wait=<s>` | read | Changes after `since`; waits up to 25 seconds for new ones |
| `GET /api/v1/stream?since=<n>` | read | The same changes as Server-Sent Events |

`<p>` is a project id or alias. The stream also accepts `?token=` because browsers can't set headers on an `EventSource`; prefer the header where you can.

## MCP over HTTP

Point an MCP client at `http://<address>:7420/mcp/<project>` with the device token as a bearer token. Evyro speaks Streamable HTTP with JSON replies:

- `initialize` returns an `Mcp-Session-Id` header. Send it with every later request.
- A session belongs to the device that started it.
- `DELETE` with the session id ends it.
- Add `?agent=<name>` to say which agent is connecting.

Devices without `control` get a read-only server: write tools are left out of `tools/list` and refused if called.

## What it doesn't do yet

- No HTTPS of its own. Use Tailscale, or `tailscale serve` for a certificate.
- Chats started in the app and chats started by `evyro serve` in a terminal are separate. Run one or the other.
- No push notifications. The phone page updates while it is open.
- Tested in automated runs (a scenario test with a stand-in agent, and the desktop UI smoke test). Not yet tested on a real phone over Tailscale.
