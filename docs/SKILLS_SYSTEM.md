# Skills

A **skill** is a short set of instructions for one kind of work, such as debugging, a security review or a handoff. Evyro keeps a library of skills and tells each agent which ones fit its task. The agent loads a skill's full text only when it needs it, so its context is not filled with instructions it does not use.

## What a skill looks like

A skill is a folder with a `SKILL.md` file. The file starts with a few settings between `---` lines, followed by the instructions in Markdown.

```markdown
---
name: api-conventions
description: Use when adding or changing an HTTP endpoint in this service.
version: 1.2.0
tags: [api, endpoint, http, route]
harnesses: [claude-code, codex]   # optional: only these agents; leave out for all
requires: ">=0.1.0"               # optional: the Evyro version it needs
---
# API conventions

1. Every endpoint has a request and response type in `src/api/types.rs`.
2. ...
```

| Setting | Required | What it does |
|---|---|---|
| `name` | Yes | 1 to 64 lowercase letters, digits or hyphens. Must match the folder name. |
| `description` | Yes | One line that says **when** to use the skill. Agents decide from this line alone whether to load it. |
| `version` | No | Like `1.2.0`. Used when importing, so an older copy does not replace a newer one. |
| `tags` | No | Words that link the skill to tasks. A task titled "Fix the crash on save" matches tags such as `crash` and `bug`. |
| `harnesses` | No | Limit the skill to some agents, for example `[claude-code]`. |
| `requires` | No | The Evyro version the skill needs, such as `>=0.1.0` or `>=0.1, <2`. |

This is the same layout Claude Code uses for its skills, so a skill written for one usually works in the other.

## Where skills come from

| Source | Folder | Who it is for |
|---|---|---|
| Built in | inside Evyro | Everyone. 15 skills, listed below. |
| Yours | `<Evyro config folder>/skills/<name>/` | All your projects. |
| This project | `<repo>/.evyro/skills/<name>/` | Everyone who works on this repository. Commit it with the code. |

A project skill replaces a skill of yours with the same name, and a skill of yours replaces a built-in one. `evyro skills list` shows which one is used and what it replaces.

**Built-in skills:** architecture-design, requirements-analysis, implementation-planning, test-driven-development, debugging, refactoring, security-review, performance-optimization, code-review, documentation, dependency-updates, context-checkpointing, agent-handoff, recovery, release-preparation.

## How agents get skills

Agents get skills in three ways, and each one shows only names and one-line descriptions until the agent asks for more:

1. **In the task context.** When an agent starts a task, the context Evyro gives it lists up to four skills that fit the task under **Skills that fit this task**. The skills' text is not included.
2. **Over MCP.** `list_skills` returns the skills that are on, with names and descriptions only. Pass `for` with some task text to get only the ones that fit. `get_skill` returns one skill's full text. Both are read-only, so they also work with `evyro mcp --read-only`.
3. **In the agent's own files**, with `evyro skills sync` (see below).

A skill from the repository (`.evyro/skills`) comes with a note when an agent loads it. The note says that the skill cannot change Evyro's verification rules. A skill can tell an agent how to work. It cannot mark a task verified, because only test receipts do that.

## Turning skills on and off

Every skill is on until you turn it off.

```bash
evyro skills disable performance-optimization          # this project only
evyro skills disable performance-optimization --global # every project
evyro skills enable performance-optimization           # back on here, even if off globally
evyro skills reset performance-optimization            # this project follows the global choice again
```

A skill that is off is not listed to agents, is left out of the task context, and cannot be loaded.

## Writing your own

```bash
evyro skills new api-conventions --description "Use when adding or changing an HTTP endpoint"
evyro skills new my-review-style --global
evyro skills check        # problems that stop a skill from being used
```

`new` creates a template to fill in. `check` lists every skill with a problem and exits with code 1 if it finds any. Evyro will not use a skill that:

- has no name or description, or a name that does not match its folder;
- is larger than 64 KB;
- contains what looks like a secret, such as an API key, a token or a password;
- needs a newer Evyro than the one installed.

## Importing and exporting

```bash
evyro skills import ./team-skills              # a folder of skill folders, into this project
evyro skills import ~/Downloads/ship-it --global
evyro skills import ./ship-it --force          # replace even if the installed version is newer
evyro skills export debugging ship-it --out ./shared   # writes ./shared/<name>/SKILL.md
```

Import copies only `SKILL.md`, never scripts or other files. It skips a skill that has any problem, including a secret. It also keeps an installed copy when that copy's version is the same or newer, unless you pass `--force`.

## Adding skills to an agent's own files

Some agents read instructions from files in the repository. `evyro skills sync` writes skill references there.

| `--for` | What it writes |
|---|---|
| `claude-code` | `.claude/skills/<name>/SKILL.md`, one per skill that is on. Claude Code then loads them as its own skills. |
| `codex`, `opencode`, `goose`, `cursor`, `amp`, `aider` | A list of skills in `AGENTS.md` |
| `gemini` | A list of skills in `GEMINI.md` |
| `copilot` | A list of skills in `.github/copilot-instructions.md` |

```bash
evyro skills sync --for claude-code
evyro skills sync --for codex --append    # AGENTS.md already has your own instructions
```

**Evyro never overwrites instructions that someone wrote:**

- A `.claude/skills/<name>/SKILL.md` that Evyro did not write is kept. The sync reports it as skipped. `--force` replaces it.
- Files Evyro wrote end with a marker comment. A later sync updates them. When a skill is turned off, the next full sync removes the file Evyro wrote for it.
- In `AGENTS.md`, `GEMINI.md` and the Copilot file, Evyro writes only between `<!-- evyro:skills:start -->` and `<!-- evyro:skills:end -->`. If the file exists and has no such section, Evyro does not change it unless you pass `--append`. That adds the section at the end and leaves the rest of the file as it was.

In the desktop app, the **Skills** tab does the same through **Add to agent files…**. It never uses `--force`.

## Desktop app

The **Skills** tab lists every skill with its source, version and state (on, off or problem). From there you can:

- click a skill to read it;
- use **Turn on** and **Turn off** for this project or for every project;
- type a task in the filter box to see which skills fit;
- create a skill with **+ New skill**;
- bring skills in with **Import…**;
- write them into an agent's files with **Add to agent files…**.

## Limits

- Matching skills to a task uses the skill's name and tags, and the task's kind, title and description. It is word matching, not a model, so a skill without good tags may not be suggested. Agents can still find it with `list_skills`.
- On/off choices are stored in Evyro's database. A full `evyro backup` keeps them, but a single-project `evyro export` does not.
