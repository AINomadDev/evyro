# Multi-AI workspace: status and handoff

Last updated: 2026-11-23 IST. The Mac workspace is on `main` with uncommitted changes. The original `feature/multi-ai-workspace` branch remains local to the cloud checkout.

## Goal

A better desktop layout for working with several AIs that share context. It should feel like T3 Code and be customisable (themes, colours, fonts, layout). Pratik chose the hybrid **A + B + C now, E and D later**.

## Completed

| Commit | What |
|---|---|
| 4a5b40f | Workspace with 1, 2 or 4 panes; sidebar grouped Needs you / Working / Your turn / Ended; shared-context panel (task, criteria, handoff, decisions, review notes, memory, activity, changed files); composer that messages several AIs; terminal drawer; status bar; live task board; appearance settings (5 themes, accents, fonts, sizes, density, radius, layout) |
| 23d4d3e | No "null" in a pane's shared-context strip |
| a42e45d | Header names each AI once; panes show the tasks an AI claimed itself; `evyro` on PATH inside agent threads |
| bbb2ec4 | A task an AI starts inside `evyro run` belongs to that run and is freed when it exits (fixes "owned by session" every time); `evyro run` with no task skips a task another AI holds; threads, output and pane layout are kept after a restart, with **Resume**; **Free T-n and retry** for tasks held by AIs that are gone; **Usage, by AI** panel (time, runs, tasks held) |
| 1198d27 | Copilot CLI (`copilot -i`) and Cursor (`cursor-agent <prompt>`) run in terminal threads; grids of six and nine; a warning before starting a second copy of an AI that is already running |

| 4de6529 | Agents Amp, Qwen Code, Kiro CLI, Factory Droid, Cline CLI, Crush; local servers MLX, llama.cpp (Unsloth GGUF), vLLM; `integrate cursor --write` fix |

- Mac workspace changes: optional names for terminal agent threads, names preserved by Resume, a focused pane with one-click return to the grid, direct access to code review, and more room for the composer and pane content.
- Shared messages can reach running AI threads beyond the visible grid. The composer keeps a draft on failure and selects the failed recipients. Cursor sign-in and Copilot folder-trust prompts surface in delivery feedback. Terminal snapshots are loaded before sending so hidden recipients can use their current paste mode.
- The Mac Tauri smoke used a fixture agent in a scratch checkout to check named threads, Focus pane, Usage, duplicate warnings, composer delivery and Resume after restart. An earlier build reproduced the duplicate sidebar row after Resume.
- Resume now reuses the terminal thread's ID, name, pane assignment and saved scrollback. Copilot threads created with a credit budget also reuse their assigned Copilot session ID.
- Usage shows named threads and Codex/Copilot account quota using their installed CLIs. Copilot threads can set a session budget and show the remaining credits from a complete usage report. The panel labels cached Copilot account readings and separates them from the session budget.
- Opening a desktop terminal, desktop ACP chat, or paired web chat now leaves the first message as a draft. Desktop ACP chat uses Cursor's `cursor_login` method and offers `antigravity-cli` for individual Gemini Code Assist accounts.
- Pane count and columns are adjustable and saved by project. New threads use an empty pane or expand the grid. The Focus picker can show an off-grid thread without changing the saved grid.
- **Columns → Side by side** keeps the panes in one row as threads are added, scrolling the focused pane into view when space runs out. The older Side by side preference maps to this mode.
- Pane count and Columns share a compact toolbar. The pane stepper supports direct entry, add and remove buttons, and preserves keyboard focus when the layout changes.
- Stopped terminals are marked read-only. **Restart shell** starts a fresh shell in the existing pane or drawer and keeps its saved output.
- The desktop Remote access card can check and enable private Tailscale Serve HTTPS for the loopback listener. It checks for public Funnel on the matching route and avoids occupied ports when proposing setup.
- Final Mac checks passed for the Rust workspace, Clippy with warnings denied, formatting, and the Chromium UI smoke and accessibility suite. Evyro `T-4` holds the receipts.

- Usage refreshes periodically and retains the last successful Codex, Copilot, and Antigravity quota readings. Claude Code can capture documented status-line limits with opt-in setup. Cursor shows dashboard links. OpenCode shows seven-day project token totals and recorded cost.
- ACP chats expose the current model and switch in place when the agent advertises the standard model option. **Continue with…** preloads the task checkpoint for a cross-agent handoff and opens the receiving terminal with an editable continuation draft.
- Mac verification covered native Antigravity quota capture and saving, Codex and Copilot account responses, OpenCode project stats, and same-session ACP model changes in Antigravity and OpenCode. The desktop smoke suite checked the model picker, a short chat pane, saved readings, and an unsent continuation draft.

## In progress

- Cursor CLI 2026.11.22-e9042b2 reached its sign-in screen through Evyro. Copilot CLI 1.0.114 answered a diagnostic prompt, reached its folder-trust prompt, and displayed a pasted interactive follow-up in disposable checkouts. The installed versions accept the current launch flags.
- Evyro task `T-4` tracks workspace checks. Native macOS verified same-thread Resume and a real Codex quota reading. A paired local client opened a real Antigravity ACP session with a draft and zero prompts. Full provider-session continuation for other terminal agents and ACP transcript persistence remain open.
- Tailscale is connected on the Mac. The route selection and setup UI are tested with fixtures. Pairing through a private HTTPS route from a phone on mobile data remains to be checked with the phone available.

## Pending (code)

0. Try the new agents for real on the Mac (all are "documented" only). Amp and Crush start without a first prompt; check whether they accept one.

1. **Option E, context ledger:** which AI read which files, file claims, token and cost usage (needs MCP read tracking and parsing each agent's logs).
2. **Option D, compare:** the same task to several AIs, each in its own git worktree, then compare and pick.
3. Save chat (ACP, "+ Chat") threads across restarts, like terminal threads. Persistent transcripts and searchable thread history are useful follow-ups for the T3 Code and px0 workflows.
4. Optional: clear stuck sessions automatically when the app starts.
5. In-app **Install** button for missing agent CLIs (Copilot, Cursor, Gemini...), using the install hints `evyro doctor` already knows.
6. Optional: claude-code target that defaults to the app sign-in (subscription) instead of an API key.

## Needs Pratik

- Clear the stuck sessions from before the fix, once: **History → Recover stale sessions**, or `./target/debug/evyro recover --include-unknown`.
- Claude Code on the Mac uses Console API billing, which is limited until 1 Dec, 5:30 AM IST. Either switch to the subscription login (`/logout`, `/login`, then pick the Claude account) or add an `--auth app_login` target.
- Try **Connect usage** after signing in to an eligible Claude subscription and getting a normal Claude Code reply. Verify the 5-hour and weekly windows in Evyro.
- Sign in to the installed Cursor CLI via `cursor-agent login`, then verify a reply and follow-up message. On first use of a checkout, review any Copilot folder-trust prompt in its pane. The repo's `.cursor/mcp.json` and the existing Copilot MCP setup were already present.
- Install and sign in to Amp, Qwen Code, Kiro CLI, Factory Droid, Cline CLI and Crush before checking their launch flags on this Mac.
- Say when it's good to push. Then push the branch, and open the PR from the Mac (no Claude footer).

## How to continue (for any AI)

1. Run `evyro context`, then read this file.
2. Work on `feature/multi-ai-workspace` in the cloud clone, or on the Mac copy.
3. Run `node crates/evyro-desktop/tests/ui-smoke.cjs` and `cargo test --workspace` before handing back.
