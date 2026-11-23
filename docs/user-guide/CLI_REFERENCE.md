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
  init         Register this repository and create minimal .evyro scaffolding
  register     Register a repository without scaffolding
  projects     List registered projects (global tracker)
  project      Set project alias / archive / favorite
  status       Project status: active task, owner, git state, last checkpoint
  doctor       Diagnose installation, database, agents and projects
  tasks        List tasks
  task         Create, inspect and update tasks
  context      Print the budgeted context packet for agents
  checkpoint   Record a checkpoint (git state captured automatically)
  handoff      Prepare a signed continuation contract for another agent
  switch       Handoff and launch the target agent now (same as `handoff --launch`). Use `handoff` to prepare the contract without launching anything
  resume       Accept a pending handoff as the receiving agent (validates state first)
  run          Launch an agent with session tracking (pid, exit checkpoint)
  chat         Chat with a coding agent over ACP (Agent Client Protocol): its reply, plan and tool calls in the terminal, and you approve each tool call. `evyro chat --list` shows which agents can
  agents       Detected agents and their capabilities
  targets      Switch targets: every AI you can hand work to (agent CLIs, IDEs, chat apps, API models). `evyro targets` lists them with their setup status
  models       Model selection support per agent
  sessions     Agent sessions
  recover      Recover sessions whose process died (quota kill, crash)
  worktrees    Git worktrees
  verify       Run a verification command and record a receipt: `evyro verify -- cargo test`
  decide       Record a decision. Record a decision: `evyro decide "<title>" "<decision>" [--why ...]`
  decisions    Decisions for the project
  search       Search the code: file names, text or regex, symbols, a file's outline
  skills       Skills: reusable instructions agents load when a task needs them
  review       Review comments on the working-tree changes: leave them on files and lines, send the open ones to an agent, resolve them when addressed
  memory       Project memory
  handoffs     Handoff history
  health       Evidence-linked project health
  export       Export this project's records (redacted, portable JSON)
  import       Import a project export into this machine's database
  backup       Snapshot the global database
  restore      Replace the global database with a backup (keeps the old one)
  logs         Event log (timeline)
  mcp          Run the MCP server on stdio for this project
  serve        Let paired devices (a phone, another computer) follow and approve the work over HTTP: an API, an event stream, MCP over HTTP and a web companion
  remote       Devices that may use `evyro serve`: pair a new one, list, revoke
  integrate    Integration helpers (MCP registration, hooks)
  completions  Shell completions
  version      Version information
  help         Print this message or the help of the given subcommand(s)

Options:
      --json               Machine-readable JSON output
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
  -C, --dir <DIR>          Run as if started in this directory
      --project <PROJECT>  Target a registered project by id or alias instead of the current directory
  -h, --help               Print help
  -V, --version            Print version
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
      --desc <DESC>               [default: ""]
      --json                      Machine-readable JSON output
      --kind <KIND>               
  -q, --quiet                     Suppress non-essential output
      --debug                     Print debug details on errors
      --priority <PRIORITY>       
      --accept <ACCEPTANCE>       
  -C, --dir <DIR>                 Run as if started in this directory
      --constraint <CONSTRAINTS>  
      --project <PROJECT>         Target a registered project by id or alias instead of the current directory
      --file <FILES>              
      --parent <PARENT>           
      --ready                     
  -h, --help                      Print help
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
Handoff and launch the target agent now (same as `handoff --launch`). Use `handoff` to prepare the contract without launching anything

Usage: evyro switch [OPTIONS] --to <TO>

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
  [PROMPT]  The first message (after Evyro's task or handoff context)

Options:
      --json                  Machine-readable JSON output
      --to <TO>               The coding agent or API target to chat with (see `evyro chat --list`)
  -q, --quiet                 Suppress non-essential output
      --task <TASK>           The task to work on (default: a handoff waiting for this agent, or the active task)
      --debug                 Print debug details on errors
      --handoff <HANDOFF>     Continue this handoff, even one prepared for a different AI
      --accept-drift          Continue a handoff even though the repository changed since it was written
  -C, --dir <DIR>             Run as if started in this directory
      --model <MODEL>         Model for API targets (passed to the agent's environment as `evyro run` does)
      --project <PROJECT>     Target a registered project by id or alias instead of the current directory
      --approve <APPROVE>     Which tool calls to allow without asking [default: ask] [possible values: ask, reads, all, none]
      --no-mcp                Do not offer Evyro's MCP server to the agent
      --command <COMMAND>...  Start this ACP command instead of the agent's usual one (program and arguments)
      --list                  List the agents that can chat over ACP and how to install the missing ones
  -h, --help                  Print help (see more with '--help')
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
Find text in files (plain text by default, `--regex` for a pattern)

Usage: evyro search text [OPTIONS] <PATTERN>

Arguments:
  <PATTERN>  What to look for

Options:
      --json               Machine-readable JSON output
      --regex              Treat the pattern as a regular expression
      --case               Match case exactly
  -q, --quiet              Suppress non-essential output
      --debug              Print debug details on errors
      --path <PATH>        Only files whose path contains this
  -C, --dir <DIR>          Run as if started in this directory
      --limit <LIMIT>      Stop after this many matching lines [default: 100]
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
Options:
      --host <HOST>        Address to listen on. 127.0.0.1 is this computer only; a Tailscale IP reaches your tailnet [default: 127.0.0.1]
      --port <PORT>        [default: 7420]
      --allow-lan          Allow an address that is neither this computer nor Tailscale (plain HTTP, not encrypted)
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
