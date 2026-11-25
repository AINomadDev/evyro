#!/usr/bin/env bash
# Refresh the read-only copies of the reference projects listed in
# docs/research/REFERENCE_PROJECTS.md and print what changed since the last look.
#
#   scripts/reference-projects.sh <empty-or-existing scratch dir>
#
# The copies are shallow clones for reading. Never build or run anything in them.
set -euo pipefail
dir="${1:?usage: reference-projects.sh <scratch dir outside the repository>}"
case "$(cd "$dir" 2>/dev/null && pwd || echo "$dir")" in
  "$(git rev-parse --show-toplevel 2>/dev/null || echo /nonexistent)"*) echo "pick a folder outside the repository" >&2; exit 2 ;;
esac
mkdir -p "$dir"
repos=(
  pingdotgg/t3code maria-rcks/t1code MiaAI-Lab/m3code stablyai/orca getpaseo/paseo
  slopus/happy OrchestratorInc/agent-orchestrator ParadevAI/Paradev get-bb/bb
  generalaction/emdash Emanuele-web04/synara coollabsio/jean
)
for r in "${repos[@]}"; do
  name="${r#*/}"
  if [ -d "$dir/$name/.git" ]; then
    old=$(git -C "$dir/$name" rev-parse --short HEAD)
    git -C "$dir/$name" fetch -q --depth 1 origin && git -C "$dir/$name" reset -q --hard FETCH_HEAD
  else
    git clone -q --depth 1 "https://github.com/$r.git" "$dir/$name"
    old="new"
  fi
  new=$(git -C "$dir/$name" rev-parse --short HEAD)
  printf '%-22s %s -> %s  %s\n' "$name" "$old" "$new" "$(git -C "$dir/$name" log -1 --format=%cs)"
done
