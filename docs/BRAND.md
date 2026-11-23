# Evyro brand guide

Evyro is the relay between AI coding tools. The brand says one thing: **work keeps moving when the AI changes.** Everything below serves that idea.

## Name and tagline

| Item | Use |
|---|---|
| Name | **Evyro**, one word, capital E. The CLI and package are `evyro`, in code style. |
| Tagline | **Switch agents. Keep building.** |
| One-line description | The local-first continuity layer for AI coding. |
| Longer description | Tasks, decisions, checkpoints and test proof travel with your repo, so the next AI picks up exactly where the last one stopped. |

## Logo

The mark is a **relay**: three nodes on one line. The outer nodes are the agent you leave and the agent you join. The middle node, ringed and filled, is the Evyro handoff that carries the work across.

| File | Use |
|---|---|
| [`docs/brand/logo.svg`](brand/logo.svg) | App icon and avatar: the mark on the indigo-to-cyan tile. |
| [`docs/brand/mark.svg`](brand/mark.svg) | The mark alone, in `currentColor`, for small sizes and one-colour use. |
| [`crates/evyro-desktop/icons/`](../crates/evyro-desktop/icons/) | Rendered app icons (.png, .icns, .ico). |

Rules:

- Keep clear space around the tile of at least a quarter of its width.
- Do not recolour the tile, rotate the mark, add shadows or outlines, or put the tile on a busy photo.
- At 24 px or smaller, use the mark alone.

## Colour

The palette comes from the logo's gradient and the desktop app's tokens in [`ui/style.css`](../crates/evyro-desktop/ui/style.css).

### Brand

| Name | Hex | Use |
|---|---|---|
| Relay Indigo | `#4F46E5` | Primary actions, links, focus, the top of the gradient |
| Signal Cyan | `#22D3EE` | Gradient end, live and moving states, highlights |
| Soft Violet | `#8B85FF` | Accent on dark backgrounds |
| Relay gradient | `#4F46E5 → #22D3EE` | Logo tile, hero headings, one key element per screen at most |

### Neutrals

| Name | Light | Dark |
|---|---|---|
| Background | `#F7F7F9` | `#111318` |
| Panel | `#FFFFFF` | `#181B22` |
| Border | `#E3E4EA` | `#2B303B` |
| Text | `#1B1D24` | `#E7E9EE` |
| Muted text | `#6B6F7D` | `#9298A7` |

### Status

Status colours carry meaning, so they are never used for decoration.

| Meaning | Light | Dark |
|---|---|---|
| Verified, ready, passing | `#16803C` | `#4CC47A` |
| Needs setup, drift, warning | `#A15C00` | `#E5A33B` |
| Failed, blocked | `#C2261D` | `#F07167` |

## Typography

| Role | Typeface | Notes |
|---|---|---|
| Headlines | **Inter Display**, 600 to 700 | Tight tracking (about −2% to −3%). |
| Interface and body | **Inter**, 400 to 600 | The desktop app uses the system UI font first (San Francisco, Segoe UI) and falls back to Inter. |
| Code, commands, ids | **JetBrains Mono**, 400 to 500 | Task keys (`T-1`), commands, hashes. |

All three are free under the SIL Open Font License.

## Voice

- **Plain and exact.** "Codex starts where Claude stopped", not "seamless AI orchestration".
- **Proof over promise.** Say what Evyro checked: Git state, test receipts, signatures. Say what it did not check.
- **Local and calm.** No cloud account, no hype, no exclamation marks.
- **Name the tools.** Claude Code, Codex, Cursor, ChatGPT. People switch between real tools, so say which.

## Assets

| Asset | File | How it is made |
|---|---|---|
| README banner (dark, light) | `docs/assets/banner-dark.png`, `banner-light.png` | `node docs/brand/render.cjs` from `docs/brand/banner.html` |
| GitHub social preview (1280×640) | `docs/assets/social-preview.png` | Same script |
| App screenshots (light, dark) | `docs/assets/screen-*.png` | `EVYRO_SHOTS=docs/assets node crates/evyro-desktop/tests/ui-smoke.cjs` |
| Landing page | `docs/site/index.html` | Hand-written, no build step |

Screenshots come from the real UI driven by the smoke test, so they always match the shipped app.
