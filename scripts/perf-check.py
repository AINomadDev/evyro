#!/usr/bin/env python3
"""Check CLI startup and large-repository timings against docs/PERFORMANCE.md.

Usage: python3 scripts/perf-check.py [path/to/evyro]   (default: target/release/evyro)

Generates a throwaway Git repository with many files and an Evyro store with many
tasks (nothing is committed), runs each budgeted command several times as a fresh
process and fails when a median exceeds its budget. Budgets live in the table
between the perf-budgets markers in docs/PERFORMANCE.md.
"""
import os, re, shlex, statistics, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOC = os.path.join(ROOT, "docs", "PERFORMANCE.md")
FILES = int(os.environ.get("PERF_FILES", 5000))
TASKS = int(os.environ.get("PERF_TASKS", 2000))
RUNS = int(os.environ.get("PERF_RUNS", 11))


def budgets():
    text = open(DOC, encoding="utf-8").read()
    m = re.search(r"<!-- perf-budgets:start -->(.*?)<!-- perf-budgets:end -->", text, re.S)
    if not m:
        sys.exit(f"no perf-budgets table in {DOC}")
    rows = []
    for line in m.group(1).splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 2 or not cells[0].startswith("`"):
            continue
        args = shlex.split(cells[0].strip("`"))
        if args[0] != "evyro":
            sys.exit(f"budget command must start with evyro: {cells[0]}")
        rows.append((cells[0].strip("`"), args[1:], float(cells[1])))
    if not rows:
        sys.exit("perf-budgets table is empty")
    return rows


def run(cmd, cwd, env):
    r = subprocess.run(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if r.returncode != 0:
        sys.exit(f"`{' '.join(cmd)}` failed ({r.returncode}):\n{r.stdout}{r.stderr}")


def generate(repo, evyro, env):
    """A repository of FILES small Rust files in 100 folders, and TASKS tasks."""
    for i in range(FILES):
        d = os.path.join(repo, "src", f"mod_{i % 100}")
        os.makedirs(d, exist_ok=True)
        body = "".join(
            f"/// Handler {i}.{j}.\npub fn handler_{i}_{j}(x: u64) -> u64 {{\n    x.wrapping_mul({j + 1})\n}}\n\n"
            for j in range(8)
        )
        if i == FILES - 1:
            body += "// perf_needle: the only line the text search finds\n"
        with open(os.path.join(d, f"file_{i}.rs"), "w") as f:
            f.write(body)
    git = ["git", "-c", "user.email=perf@example.com", "-c", "user.name=perf"]
    run(git + ["init", "-q", "-b", "main"], repo, env)
    run(git + ["add", "-A"], repo, env)
    run(git + ["commit", "-q", "-m", "generated"], repo, env)
    run([evyro, "init", "-q", "--no-agent-files"], repo, env)
    for i in range(1, TASKS + 1):
        run([evyro, "task", "add", f"generated task {i}", "-q"], repo, env)


def measure(evyro, args, cwd, env):
    cmd = [evyro] + args
    run(cmd, cwd, env)  # warm-up, also checks the command works
    ts = []
    for _ in range(RUNS):
        t = time.perf_counter()
        subprocess.run(cmd, cwd=cwd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        ts.append((time.perf_counter() - t) * 1000)
    return statistics.median(ts), max(ts)


def main():
    evyro = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "evyro"))
    if not os.path.isfile(evyro):
        sys.exit(f"{evyro} not found; run `cargo build --release -p evyro-cli` first")
    rows = budgets()
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, EVYRO_HOME=os.path.join(tmp, "home"))
        repo = os.path.join(tmp, "repo")
        os.makedirs(repo)
        t = time.perf_counter()
        generate(repo, evyro, env)
        print(f"generated {FILES} files and {TASKS} tasks in {time.perf_counter() - t:.1f} s; {RUNS} runs each\n")
        lines = ["| Command | Median | Max | Budget | |", "|---|---:|---:|---:|---|"]
        failed = []
        for label, args, budget in rows:
            med, mx = measure(evyro, args, repo, env)
            ok = med <= budget
            if not ok:
                failed.append(label)
            print(f"{'ok  ' if ok else 'OVER'} {label:45} median {med:7.1f} ms  max {mx:7.1f} ms  budget {budget:6.0f} ms")
            lines.append(f"| `{label}` | {med:.1f} ms | {mx:.1f} ms | {budget:.0f} ms | {'ok' if ok else '**over budget**'} |")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as f:
            f.write("## Performance budgets\n\n" + "\n".join(lines) + "\n")
    if failed:
        sys.exit(f"\nover budget: {', '.join(failed)}. See docs/PERFORMANCE.md before raising a budget.")
    print("\nall within budget")


if __name__ == "__main__":
    main()
