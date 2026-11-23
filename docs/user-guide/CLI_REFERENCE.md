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
  agents       Detected agents and their capabilities
  targets      Switch targets: every AI you can hand work to (agent CLIs, IDEs, chat apps, API models). `evyro targets` lists them with their setup status
  models       Model selection support per agent
  sessions     Agent sessions
  recover      Recover sessions whose process died (quota kill, crash)
  worktrees    Git worktrees
  verify       Run a verification command and record a receipt: `evyro verify -- cargo test`
  decide       Record a decision. Record a decision: `evyro decide "<title>" "<decision>" [--why ...]`
  decisions    Decisions for the project
  memory       Project memory
  handoffs     Handoff history
  health       Evidence-linked project health
  export       Export this project's records (redacted, portable JSON)
  import       Import a project export into this machine's database
  backup       Snapshot the global database
  restore      Replace the global database with a backup (keeps the old one)
  logs         Event log (timeline)
  mcp          Run the MCP server on stdio for this project
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
