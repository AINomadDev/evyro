#!/bin/sh
# Reproduce docs/PERFORMANCE.md: median/max wall time of common commands.
set -e
cargo build --release -q
E="$(pwd)/target/release/evyro"
export EVYRO_HOME="$(mktemp -d)"
R="$(mktemp -d)"; cd "$R"
git init -q -b main; git -c user.email=b@b -c user.name=b commit -q --allow-empty -m init
"$E" init -q
i=1; while [ $i -le 20 ]; do "$E" task add "task $i" -q; i=$((i+1)); done
python3 - "$E" <<'PY'
import subprocess, time, sys, statistics, resource
E = sys.argv[1]
for a in (["version"], ["tasks"], ["status"], ["context"], ["checkpoint", "--auto", "--task", "T-1"]):
    ts = []
    for _ in range(30):
        t = time.perf_counter(); subprocess.run([E] + a, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); ts.append((time.perf_counter() - t) * 1000)
    print(f"{' '.join(a):35} median {statistics.median(ts):6.1f} ms  max {max(ts):6.1f} ms")
print("peak RSS (KB):", resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
PY
ls -l "$E" | awk '{print "binary bytes:", $5}'
