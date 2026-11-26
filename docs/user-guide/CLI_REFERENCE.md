# CLI reference

Generated from `evyro --help` (v0.1.0, 2026-11-23). For a guided tour see the [User guide](USER_GUIDE.md). Every command accepts `--json`, `--quiet`, `--debug`, `-C <dir>` and `--project <id|alias>`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | general error, failed verification |
| 2 | invalid input |
| 3 | not found |
| 4 | invalid transition or verification required |
| 5 | conflict or state drift |
| 6 | unsupported capability (fallback printed) |

## Commands

```text
Evyro: switch agents, keep building.

Usage: evyro [OPTIONS] <COMMAND>

Commands:
  capture-usage  Save reported limits from a Claude Code or Antigravity status-line hook
  init           Register this repository and create minimal .evyro scaffolding
  register       Register a repository without scaffolding
  projects       List registered projects (global tracker)
  project        Set project alias / archive / favorite
  status         Project status: active task, owner, git state, last checkpoint
  doctor         Diagnose installation, database, agents and projects
  tasks          List tasks
  task           Create, inspect and update tasks
  context        Print the budgeted context packet for agents
  heartbeat      Tell Evyro this session is still working. A session that has no process Evyro can watch counts as gone after 30 minutes without any checkpoint, event or heartbeat
  checkpoint     Record a checkpoint (git state captured automatically)
  handoff        Prepare a signed continuation contract for another agent
  work           Start the next ready task on a free AI. Nothing to name or set up: Evyro picks the most important task that can start, an installed AI that is not busy, and gives the task its own worktree when another AI is already working in the main checkout
  switch         Switch a task to another AI because the current one ran out of usage. You confirm the usage is used up; Evyro carries the latest checkpoint over, stops the current AI and starts the new one, so only one AI works on the task at a time. Use `handoff` to write your own notes or to prepare the contract without launching
  resume         Accept a pending handoff as the receiving agent (validates state first)
  run            Launch an agent with session tracking (pid, exit checkpoint)
  chat           Chat with a coding agent over ACP (Agent Client Protocol): its reply, plan and tool calls in the terminal, and you approve each tool call. `evyro chat --list` shows which agents can
  agents         Detected agents and their capabilities. `evyro agents check <name>` tests one agent definition, built-in or custom
  targets        Switch targets: every AI you can hand work to (agent CLIs, IDEs, chat apps, API models). `evyro targets` lists them with their setup status
  models         Model selection support per agent
  sessions       Agent sessions
  recover        Recover sessions whose process died (quota kill, crash)
  worktrees      Git worktrees
  sample         Create a small sample project to try Evyro on (inside Evyro's own data folder) with two ready tasks and a test command that works anywhere, and print the first steps
  pr             The pull request of a task: what is failing and what reviewers asked for, as a prompt for the AI that owns the branch. Needs the GitHub CLI (`gh`)
  plan           Plan first: an AI proposes a plan, you approve it, then it builds
  verify         Run a verification command and record a receipt: `evyro verify -- cargo test`
  decide         Record a decision. Record a decision: `evyro decide "<title>" "<decision>" [--why ...]`
  decisions      Decisions for the project
  search         Search the code: file names, text or regex, symbols, a file's outline
  serve          Let paired devices (a phone, another computer) follow and approve the work over HTTP: an API, an event stream, MCP over HTTP and a web companion
  remote         Devices that may use `evyro serve`: pair a new one, list, revoke
  skills         Skills: reusable instructions agents load when a task needs them
  review         Review comments on the working-tree changes: leave them on files and lines, send the open ones to an agent, resolve them when addressed
  memory         Project memory
  handoffs       Handoff history
  health         Evidence-linked project health
  export         Export this project's records (redacted, portable JSON)
  import         Import a project export into this machine's database
  backup         Snapshot the global database
  restore        Replace the global database with a backup (keeps the old one)
  logs           Event log (timeline)
  mcp            Run the MCP server on stdio for this project
  integrate      Integration helpers (MCP registration, hooks)
  completions    Shell completions
  version        Version information
  bug-report     Write a bug report: versions, system, doctor checks and recent crash logs, with secrets redacted. Read it before you share it
  help           Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
  -V, --version            Print version
```

### evyro capture-usage

Read the CLI's documented status-line JSON from standard input and save the published quota fields for the desktop Usage panel. See [Desktop usage](DESKTOP.md#usage-and-continuation) for setup.

```text
Save reported limits from a Claude Code or Antigravity status-line hook

Usage: evyro capture-usage [OPTIONS] <HARNESS>

Arguments:
  <HARNESS>  [possible values: claude-code, antigravity-cli]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro init

```text
Register this repository and create minimal .evyro scaffolding

Usage: evyro init [OPTIONS]

Options:
      --json                Machine-readable JSON output
      --name <NAME>         
      --no-agent-files      Do not create AGENTS.md / CLAUDE.md / GEMINI.md entry files
  -q, --quiet               Suppress non-essential output
      --append-agent-files  Append the Evyro section to existing agent files that lack it
      --debug               Print debug details on errors
  -C, --dir <DIR>           Run as if started in this directory
      --project <PROJECT>   Target a registered project by id or alias instead of the current directory
  -h, --help                Print help
```

### evyro task add

```text
Add a task

Usage: evyro task add [OPTIONS] <TITLE>

Arguments:
  <TITLE>  

Options:
      --desc <DESC>                  [default: ""]
      --json                         Machine-readable JSON output
      --kind <KIND>                  epic, feature, task, subtask, bug, tech_debt, spike, docs, maintenance, integration, security or release
  -q, --quiet                        Suppress non-essential output
      --debug                        Print debug details on errors
      --priority <PRIORITY>          0 (critical) to 4 (trivial); default 2
      --accept <ACCEPTANCE>          An acceptance criterion; repeat for several
  -C, --dir <DIR>                    Run as if started in this directory
      --constraint <CONSTRAINTS>     A constraint the AI must respect; repeat for several
      --project <PROJECT>            Target a registered project by id or alias instead of the current directory
      --file <FILES>                 A file or folder the task changes. Tasks with overlapping files never run side by side
      --needs-person <NEEDS_PERSON>  Mark the task as needing a person (a phone, a certificate, a sign-in) and say why. `evyro work` and the desktop never hand such a task to an AI
      --parent <PARENT>              The epic or task this one belongs to
      --ready                        Start in the ready state instead of the backlog
  -h, --help                         Print help
```

### evyro task approve-pieces

```text
Approve the pieces an AI wrote for a task, so workers may start them. A person has to run it at a terminal and type the task key

Usage: evyro task approve-pieces [OPTIONS] <TASK>

Arguments:
  <TASK>  The task whose pieces to approve

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro task split

```text
Split a task into pieces other AIs can take. Reads a JSON list of pieces (`title`, optional `description`, `files`, `acceptance`, `after`) from a file or `-`. Pieces that share files run one after the other. Nothing is started

Usage: evyro task split [OPTIONS] --file <FILE> <TASK>

Arguments:
  <TASK>  The task to split

Options:
      --file <FILE>        The pieces as JSON, from a file or `-` for standard input
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro task start

```text
Claim a task for an agent (starts a session, takes the lease)

Usage: evyro task start [OPTIONS] --agent <AGENT> <TASK>

Arguments:
  <TASK>  

Options:
      --agent <AGENT>        
      --json                 Machine-readable JSON output
      --model <MODEL>        
  -q, --quiet                Suppress non-essential output
      --debug                Print debug details on errors
      --provider <PROVIDER>  
      --accept-drift         If the task has an open handoff with drift, continue anyway
  -C, --dir <DIR>            Run as if started in this directory
      --project <PROJECT>    Target a registered project by id or alias instead of the current directory
  -h, --help                 Print help
```

### evyro task check

```text
Mark acceptance criterion <n> (1-based) met. `verified` needs all of them

Usage: evyro task check [OPTIONS] <TASK> <INDEX>

Arguments:
  <TASK>   
  <INDEX>  

Options:
      --json               Machine-readable JSON output
      --note <NOTE>        [default: ""]
  -q, --quiet              Suppress non-essential output
      --receipt <RECEIPT>  Link a passing verification receipt as evidence
      --debug              Print debug details on errors
      --unmet              Mark it not met instead
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro task update

```text
Update a task

Usage: evyro task update [OPTIONS] <TASK>

Arguments:
  <TASK>  

Options:
      --json
          Machine-readable JSON output
      --status <STATUS>
          
  -q, --quiet
          Suppress non-essential output
      --title <TITLE>
          
      --debug
          Print debug details on errors
      --desc <DESC>
          
  -C, --dir <DIR>
          Run as if started in this directory
      --priority <PRIORITY>
          
      --accept <ACCEPTANCE>
          
      --project <PROJECT>
          Target a registered project by id or alias instead of the current directory
      --file <FILES>
          
      --needs-person <NEEDS_PERSON>
          Mark the task as needing a person (a phone, a certificate, a sign-in) and say why
      --comment <COMMENT>
          
      --agent <AGENT>
          Acting agent (for provenance). Omit when a human runs the command
      --human-override <HUMAN_OVERRIDE>
          Human-only: mark verified without a receipt, recording the reason
  -h, --help
          Print help
```

### evyro handoff

```text
Prepare a signed continuation contract for another agent

Usage: evyro handoff [OPTIONS] --to <TO>

Options:
      --json                   Machine-readable JSON output
      --to <TO>                
  -q, --quiet                  Suppress non-essential output
      --task <TASK>            
      --debug                  Print debug details on errors
      --summary <SUMMARY>      [default: ""]
  -C, --dir <DIR>              Run as if started in this directory
      --done <DONE>            
      --project <PROJECT>      Target a registered project by id or alias instead of the current directory
      --remaining <REMAINING>  
      --next <NEXT>            [default: ""]
      --model <MODEL>          
      --launch                 Launch the target agent after preparing the handoff
      --headless               Launch in headless (non-interactive) mode
      --from <FROM>            Who is preparing the handoff (default: the owning session's agent, else human)
      --no-open                IDE/chat targets: prepare the hand-over without opening apps or the clipboard
  -h, --help                   Print help
```

### evyro switch

```text
Switch a task to another AI because the current one ran out of usage. You confirm the usage is used up; Evyro carries the latest checkpoint over, stops the current AI and starts the new one, so only one AI works on the task at a time. Use `handoff` to write your own notes or to prepare the contract without launching

Usage: evyro switch [OPTIONS] --to <TO>

Options:
      --json               Machine-readable JSON output
      --to <TO>            The AI to continue with (a harness, or a model on the same harness with --model)
  -q, --quiet              Suppress non-essential output
      --task <TASK>        The task to switch (default: the active task)
      --debug              Print debug details on errors
      --model <MODEL>      The model to run on the new AI (default: its own)
  -C, --dir <DIR>          Run as if started in this directory
  -y, --yes                Confirm that the current AI's usage has run out or is about to
      --headless           Launch in headless (non-interactive) mode
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
      --no-launch          Prepare the switch and stop the old AI, but do not start the new one
      --no-open            IDE/chat targets: prepare the hand-over without opening apps or the clipboard
  -h, --help               Print help
```

### evyro resume

```text
Accept a pending handoff as the receiving agent (validates state first)

Usage: evyro resume [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>      
      --json               Machine-readable JSON output
      --handoff <HANDOFF>  
  -q, --quiet              Suppress non-essential output
      --accept-drift       
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --model <MODEL>      
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro run

```text
Launch an agent with session tracking (pid, exit checkpoint)

Usage: evyro run [OPTIONS] --agent <AGENT>

Options:
      --agent <AGENT>      Switch target: a harness (claude, codex…), an IDE (cursor-ide, vscode…), a chat app (chatgpt, claude-app…) or one of your own targets. See `evyro targets`
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --task <TASK>        
      --debug              Print debug details on errors
      --headless           
  -C, --dir <DIR>          Run as if started in this directory
      --prompt <PROMPT>    
      --model <MODEL>      
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
      --accept-drift       
      --no-open            IDE/chat targets: prepare the hand-over without opening apps or the clipboard
      --handoff <HANDOFF>  Continue this handoff, even one prepared for a different AI
      --plan-first         Plan first: the AI proposes a plan for the task and waits for you to approve it (`evyro plan approve`). Claude Code and Codex start read-only; other AIs are asked
  -h, --help               Print help
```

### evyro chat

Chat with a coding agent over ACP (Agent Client Protocol). The agent's reply, plan and tool calls stream into the terminal, and you approve each tool call. The chat runs under an Evyro session, like `evyro run`: it claims the task or a waiting handoff, checkpoints after each turn, and gives the agent Evyro's MCP server. Each line you type is the next message; an empty line or `/exit` ends the chat. Details: [ACP](../protocols/ACP.md).

```bash
evyro chat --list                                  # which agents can chat here
evyro chat --to codex --task T-1                   # ask before every tool call
evyro chat --to claude-code --approve reads "Start with the failing test"
echo "now add the docs" | evyro chat --to codex --approve all "fix the bug" --json
```

```text
Chat with a coding agent over ACP (Agent Client Protocol): its reply, plan and tool calls in the terminal, and you approve each tool call. `evyro chat --list` shows which agents can

Usage: evyro chat [OPTIONS] [PROMPT]

Arguments:
  [PROMPT]
          The first message (after Evyro's task or handoff context)

Options:
      --json
          Machine-readable JSON output

      --to <TO>
          The coding agent or API target to chat with (see `evyro chat --list`)

  -q, --quiet
          Suppress non-essential output

      --task <TASK>
          The task to work on (default: a handoff waiting for this agent, or the active task)

      --debug
          Print debug details on errors

      --handoff <HANDOFF>
          Continue this handoff, even one prepared for a different AI

      --accept-drift
          Continue a handoff even though the repository changed since it was written

  -C, --dir <DIR>
          Run as if started in this directory

      --model <MODEL>
          Model for API targets (passed to the agent's environment as `evyro run` does)

      --project <PROJECT>
          Target a registered project by id or alias instead of the current directory

      --approve <APPROVE>
          Which tool calls to allow without asking

          Possible values:
          - ask:   Ask in the terminal before each tool call (refuse when there is no terminal)
          - reads: Allow reading, searching and thinking; ask about everything else
          - all:   Allow every tool call
          - none:  Refuse every tool call
          
          [default: ask]

      --no-mcp
          Do not offer Evyro's MCP server to the agent

      --command <COMMAND>...
          Start this ACP command instead of the agent's usual one (program and arguments)

      --list
          List the agents that can chat over ACP and how to install the missing ones

  -h, --help
          Print help (see a summary with '-h')
```

### evyro targets

```text
Switch targets: every AI you can hand work to (agent CLIs, IDEs, chat apps, API models). `evyro targets` lists them with their setup status

Usage: evyro targets [OPTIONS] [COMMAND]

Commands:
  list       List every target with its setup status (the default)
  check      Show one target's checks and next step
  add        Add or replace your own target. Keys are never stored: give the environment variable name (--key-env) or a macOS Keychain service (--key-keychain)
  remove     Remove one of your own targets
  providers  Providers you can use with API targets
  help       Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro targets add

```text
Add or replace your own target. Keys are never stored: give the environment variable name (--key-env) or a macOS Keychain service (--key-keychain)

Usage: evyro targets add [OPTIONS] --kind <KIND> <NAME>

Arguments:
  <NAME>  

Options:
      --json                         Machine-readable JSON output
      --kind <KIND>                  harness | api | ide | chat
      --harness <HARNESS>            Agent CLI for harness/api targets (api defaults to opencode)
  -q, --quiet                        Suppress non-essential output
      --app <APP>                    IDE id (ide targets) or chat app id (chat targets)
      --debug                        Print debug details on errors
      --auth <AUTH>                  auto | app_login | api_key | local [default: auto]
  -C, --dir <DIR>                    Run as if started in this directory
      --project <PROJECT>            Target a registered project by id or alias instead of the current directory
      --provider <PROVIDER>          
      --model <MODEL>                
      --key-env <KEY_ENV>            
      --key-keychain <KEY_KEYCHAIN>  
      --base-url <BASE_URL>          
      --description <DESCRIPTION>    [default: ""]
  -h, --help                         Print help
```

### evyro targets check

```text
Show one target's checks and next step

Usage: evyro targets check [OPTIONS] <NAME>

Arguments:
  <NAME>  

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro targets providers

```text
Providers you can use with API targets

Usage: evyro targets providers [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro integrate

```text
Integration helpers (MCP registration, hooks)

Usage: evyro integrate [OPTIONS] <AGENT>

Arguments:
  <AGENT>  

Options:
      --hooks              
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --write              IDEs: write the project MCP config (.cursor/mcp.json, .vscode/mcp.json) if it does not exist
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro verify

```text
Run a verification command and record a receipt: `evyro verify -- cargo test`

Usage: evyro verify [OPTIONS] [COMMAND]...

Arguments:
  [COMMAND]...  

Options:
      --json               Machine-readable JSON output
      --task <TASK>        
      --kind <KIND>        
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
      --index <INDEX>      [default: 0]
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro checkpoint

```text
Record a checkpoint (git state captured automatically)

Usage: evyro checkpoint [OPTIONS]

Options:
      --json                   Machine-readable JSON output
      --task <TASK>            
  -q, --quiet                  Suppress non-essential output
      --summary <SUMMARY>      [default: ""]
      --debug                  Print debug details on errors
      --done <DONE>            
  -C, --dir <DIR>              Run as if started in this directory
      --remaining <REMAINING>  
      --blocker <BLOCKERS>     
      --project <PROJECT>      Target a registered project by id or alias instead of the current directory
      --next <NEXT>            [default: ""]
      --trigger <TRIGGER>      
      --auto                   Hook mode: skip when nothing changed, never fail when no task is active
  -h, --help                   Print help
```

### evyro context

```text
Print the budgeted context packet for agents

Usage: evyro context [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --task <TASK>        
      --budget <BUDGET>    [default: small]
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
      --quiet-errors       
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro decide

```text
Record a decision. Record a decision: `evyro decide "<title>" "<decision>" [--why ...]`

Usage: evyro decide [OPTIONS] <TITLE> [DECISION_TEXT]

Arguments:
  <TITLE>          
  [DECISION_TEXT]  The decision itself (or pass it with --decision)

Options:
      --decision <DECISION>  
      --json                 Machine-readable JSON output
  -q, --quiet                Suppress non-essential output
      --why <WHY>            [default: ""]
      --debug                Print debug details on errors
      --task <TASK>          
  -C, --dir <DIR>            Run as if started in this directory
      --project <PROJECT>    Target a registered project by id or alias instead of the current directory
  -h, --help                 Print help
```

### evyro review

```text
Review comments on the working-tree changes: leave them on files and lines, send the open ones to an agent, resolve them when addressed

Usage: evyro review [OPTIONS] [COMMAND]

Commands:
  list     Comments still waiting for an agent (open or sent). The default
  comment  Comment on a file or a line: `evyro review comment src/lib.rs:42 "Handle the empty case"`
  resolve  Mark comments as addressed
  reopen   Open a resolved or sent comment again
  delete   Remove a comment
  prompt   Print the open comments as one prompt to paste into any AI
  help     Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro review comment

```text
Comment on a file or a line: `evyro review comment src/lib.rs:42 "Handle the empty case"`

Usage: evyro review comment [OPTIONS] <LOCATION> <TEXT>

Arguments:
  <LOCATION>  `path` or `path:line`, relative to the repository root
  <TEXT>      

Options:
      --json               Machine-readable JSON output
      --task <TASK>        
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro review resolve

```text
Mark comments as addressed

Usage: evyro review resolve [OPTIONS] [IDS]...

Arguments:
  [IDS]...  

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro review prompt

```text
Print the open comments as one prompt to paste into any AI

Usage: evyro review prompt [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --mark-sent          Mark the printed comments as sent
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro memory add

```text
Save a memory: `evyro memory add "<text>" [--category fact]`. The older form `memory add <category> "<text>"` still works

Usage: evyro memory add [OPTIONS] <TEXT>...

Arguments:
  <TEXT>...  

Options:
      --category <CATEGORY>  
      --json                 Machine-readable JSON output
      --file <FILES>         
  -q, --quiet                Suppress non-essential output
      --debug                Print debug details on errors
      --global               
  -C, --dir <DIR>            Run as if started in this directory
      --task <TASK>          
      --project <PROJECT>    Target a registered project by id or alias instead of the current directory
  -h, --help                 Print help
```

### evyro heartbeat

```text
Tell Evyro this session is still working. A session that has no process Evyro can watch counts as gone after 30 minutes without any checkpoint, event or heartbeat

Usage: evyro heartbeat [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --task <TASK>        The task the session holds (default: the session this command runs in, or the active task)
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro recover

```text
Recover sessions whose process died (quota kill, crash)

Usage: evyro recover [OPTIONS]

Options:
      --include-unknown    
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro search

Search the repository: fuzzy file names, text or regex, symbols, and one file's outline. Only files Git tracks or would track are searched (`.gitignore` is honoured). Binary files, files over 1 MB, `.evyro/` and sensitive files (`.env`, keys) are skipped, and secrets in matched lines are redacted. `--json` gives the raw results.

```bash
evyro search files ratelim
evyro search text "allow_request" --path src/
evyro search text --regex 'fn \w+_request'
evyro search symbols RateLimiter
evyro search outline src/net/rate_limiter.rs
```

```text
Search the code: file names, text or regex, symbols, a file's outline

Usage: evyro search [OPTIONS] <COMMAND>

Commands:
  files    Fuzzy-find files by name: `evyro search files hndrs` finds handoff.rs
  text     Find text in files (plain text by default, `--regex` for a pattern)
  symbols  Find functions, types and other symbols by name across the repository
  outline  List the symbols in one file
  help     Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro skills

Skills are reusable instructions that agents load when a task needs them: 15 built in, yours (`--global`), and the project's own in `.evyro/skills`. `list` is the default. `--for` shows the skills that fit some task text. `check` exits 1 if any skill has a problem. `import` skips skills with secrets or a version older than or the same as the one installed (`--force` replaces it). `sync` never replaces a file someone wrote without `--force`, or adds to an instruction file without `--append`. Details: [Skills](../SKILLS_SYSTEM.md).

```bash
evyro skills
evyro skills list --for "fix the crash on save"
evyro skills show debugging
evyro skills disable performance-optimization --global
evyro skills new api-conventions --description "Use when adding an HTTP endpoint"
evyro skills import ./team-skills
evyro skills export debugging --out ./shared
evyro skills sync --for claude-code
evyro skills sync --for codex --append
```

```text
Skills: reusable instructions agents load when a task needs them

Usage: evyro skills [OPTIONS] [COMMAND]

Commands:
  list     Every skill here, with its source, version and whether it is on. The default
  show     Print one skill's full text
  new      Start a new skill from a template (in this project, or --global)
  enable   Turn skills on (for this project, or --global for every project)
  disable  Turn skills off (for this project, or --global for every project)
  reset    Forget this project's on/off choice, so the global one applies
  check    List problems that stop skills from being used
  import   Copy skills in from a SKILL.md, a skill folder or a folder of skills
  export   Write skills out as <dir>/<name>/SKILL.md (all usable skills if none named)
  sync     Write skill references where an agent reads its instructions
  help     Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

```text
Write skill references where an agent reads its instructions

Usage: evyro skills sync [OPTIONS] --for <HARNESS> [NAMES]...

Arguments:
  [NAMES]...  Only these skills (default: every skill that is on)

Options:
      --for <HARNESS>      claude-code, codex, gemini, copilot, opencode, goose, cursor, amp or aider
      --json               Machine-readable JSON output
      --append             Add the skills list to an instruction file that has no Evyro section yet
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
      --force              Replace skill files that someone else wrote
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro serve

Serves the phone companion, the `/api/v1` API, the event feed and MCP over HTTP to devices you paired. It runs until Ctrl-C. Only `127.0.0.1` and Tailscale addresses are allowed unless you pass `--allow-lan`, because plain HTTP on other networks is not encrypted. With `--json` the first line is `{"url", "reach", "api"}`. Details: [Remote access](../REMOTE_ACCESS.md).

```bash
evyro serve
evyro serve --host 100.101.102.103 --port 7420
evyro serve --host 0.0.0.0 --allow-lan
```

```text
Let paired devices (a phone, another computer) follow and approve the work over HTTP: an API, an event stream, MCP over HTTP and a web companion

Usage: evyro serve [OPTIONS]

Options:
      --host <HOST>        Address to listen on. 127.0.0.1 is this computer only; a Tailscale IP reaches your tailnet [default: 127.0.0.1]
      --json               Machine-readable JSON output
      --port <PORT>        [default: 7420]
  -q, --quiet              Suppress non-essential output
      --allow-lan          Allow an address that is neither this computer nor Tailscale (plain HTTP, not encrypted)
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro remote

Devices that may sign in to `evyro serve`. `pair` makes a one-time code (it works once, for `--minutes`, 1 to 60, default 10). `devices` lists every device with its access, state and when and from where it was last seen. `revoke` takes an id, the first 6 or more characters of one, or an exact name; the device's token stops working at once.

```bash
evyro remote pair --name "My phone" --scope approve
evyro remote devices
evyro remote revoke "My phone"
```

Scopes: `read` (look only), `approve` (also answer permission requests, stop a turn and comment), `control` (also start, message and close chats, and MCP write tools).

### evyro export

```text
Export this project's records (redacted, portable JSON)

Usage: evyro export [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --out <OUT>          
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro import

```text
Import a project export into this machine's database

Usage: evyro import [OPTIONS] <FILE>

Arguments:
  <FILE>  

Options:
      --json               Machine-readable JSON output
      --root <ROOT>        
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

## More commands

### evyro register

```text
Register a repository without scaffolding

Usage: evyro register [OPTIONS] [PATH]

Arguments:
  [PATH]  

Options:
      --json               Machine-readable JSON output
      --name <NAME>        
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro projects

```text
List registered projects (global tracker)

Usage: evyro projects [OPTIONS]

Options:
      --all                
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --search <SEARCH>    
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro project

```text
Set project alias / archive / favorite

Usage: evyro project [OPTIONS]

Options:
      --alias <ALIAS>        
      --json                 Machine-readable JSON output
      --archive <ARCHIVE>    [possible values: true, false]
  -q, --quiet                Suppress non-essential output
      --debug                Print debug details on errors
      --favorite <FAVORITE>  [possible values: true, false]
  -C, --dir <DIR>            Run as if started in this directory
      --project <PROJECT>    Target a registered project by id or alias instead of the current directory
  -h, --help                 Print help
```

### evyro status

```text
Project status: active task, owner, git state, last checkpoint

Usage: evyro status [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro doctor

```text
Diagnose installation, database, agents and projects

Usage: evyro doctor [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro tasks

```text
List tasks

Usage: evyro tasks [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --ready              
  -q, --quiet              Suppress non-essential output
      --status <STATUS>    
      --all                
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro agents

```text
Detected agents and their capabilities. `evyro agents check <name>` tests one agent definition, built-in or custom

Usage: evyro agents [OPTIONS] [COMMAND]

Commands:
  list   List detected agents and their capabilities (the default)
  check  Test one agent definition: it parses, the program is found, the login check, and the argv Evyro builds. Exits 1 if a check fails
  help   Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro agents check

```text
Test one agent definition: it parses, the program is found, the login check, and the argv Evyro builds. Exits 1 if a check fails

Usage: evyro agents check [OPTIONS] <NAME>

Arguments:
  <NAME>  A built-in agent, a custom agent's name, or a path to a .toml file (checked as your own file, so its login check runs)

Options:
      --json               Machine-readable JSON output
      --smoke              Also start the agent once in an empty scratch folder with a short prompt and check that it exits 0. Uses the agent's account
      --flow               Also run the Evyro workflow with the agent in a scratch project (your own projects are not touched): start a task, have the agent record a checkpoint with `evyro checkpoint`, hand off to it and resume. Uses the agent's account
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
      --timeout <TIMEOUT>  Seconds to wait for each agent run [default: 180]
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro models

```text
Model selection support per agent

Usage: evyro models [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro sessions

```text
Agent sessions

Usage: evyro sessions [OPTIONS]

Options:
      --active             
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro sample

```text
Create a small sample project to try Evyro on (inside Evyro's own data folder) with two ready tasks and a test command that works anywhere, and print the first steps

Usage: evyro sample [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro pr

```text
The pull request of a task: what is failing and what reviewers asked for, as a prompt for the AI that owns the branch. Needs the GitHub CLI (`gh`)

Usage: evyro pr [OPTIONS] <COMMAND>

Commands:
  feedback  Failing checks (with the end of their log) and unresolved review comments on the task's pull request, written as one prompt. Quoted GitHub text is fenced as data
  help      Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro pr feedback

```text
Failing checks (with the end of their log) and unresolved review comments on the task's pull request, written as one prompt. Quoted GitHub text is fenced as data

Usage: evyro pr feedback [OPTIONS] [TASK]

Arguments:
  [TASK]  The task (default: the active task)

Options:
      --json               Machine-readable JSON output
      --links-only         Name the failing checks and comments with links, but quote no log or comment text
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro plan

```text
Plan first: an AI proposes a plan, you approve it, then it builds

Usage: evyro plan [OPTIONS] <COMMAND>

Commands:
  show     Show the plan for a task and whether it is approved
  set      Propose a plan for a task. An AI in plan mode does this; the plan then waits for you
  files    Show the files the plan names that exist in the checkout: what approving it would declare for the task. Changes nothing
  approve  Approve the plan that is waiting. A person has to run it at a terminal and type the task key; it refuses when run by an AI or from a script
  reject   Reject the plan that is waiting, with the reason the AI should read
  help     Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro work

```text
Start the next ready task on a free AI. Nothing to name or set up: Evyro picks the most important task that can start, an installed AI that is not busy, and gives the task its own worktree when another AI is already working in the main checkout

Usage: evyro work [OPTIONS]

Options:
      --agent <AGENT>      The AI to use (default: an installed AI that is not working on anything)
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --task <TASK>        The task to start (default: the most important ready task)
      --debug              Print debug details on errors
      --under <UNDER>      Pick only among the ready pieces of this task (see `evyro task split`)
  -C, --dir <DIR>          Run as if started in this directory
      --headless           Launch in headless (non-interactive) mode
      --dry-run            Show what would start without starting it
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro worktrees

```text
Git worktrees

Usage: evyro worktrees [OPTIONS] <COMMAND>

Commands:
  list    
  add     Create a worktree + branch for a task
  prune   Tidy finished work: remove the worktrees of finished tasks that are clean and merged, and keep any with uncommitted or unmerged work
  remove  Remove a worktree (refuses with uncommitted changes unless --force)
  help    Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro decisions

```text
Decisions for the project

Usage: evyro decisions [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro handoffs

```text
Handoff history

Usage: evyro handoffs [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro health

```text
Evidence-linked project health

Usage: evyro health [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro backup

```text
Snapshot the global database

Usage: evyro backup [OPTIONS] <FILE>

Arguments:
  <FILE>  

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro restore

```text
Replace the global database with a backup (keeps the old one)

Usage: evyro restore [OPTIONS] <FILE>

Arguments:
  <FILE>  

Options:
      --json               Machine-readable JSON output
      --yes                
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro logs

```text
Event log (timeline)

Usage: evyro logs [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --task <TASK>        
      --limit <LIMIT>      [default: 50]
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro mcp

```text
Run the MCP server on stdio for this project

Usage: evyro mcp [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --read-only          
      --agent <AGENT>      
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro completions

```text
Shell completions

Usage: evyro completions [OPTIONS] <SHELL>

Arguments:
  <SHELL>  [possible values: bash, elvish, fish, powershell, zsh]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro version

```text
Version information

Usage: evyro version [OPTIONS]

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```

### evyro bug-report

```text
Write a bug report: versions, system, doctor checks and recent crash logs, with secrets redacted. Read it before you share it

Usage: evyro bug-report [OPTIONS]

Options:
      --json               Machine-readable JSON output
      --out <OUT>          Write to this file instead of printing
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
```
