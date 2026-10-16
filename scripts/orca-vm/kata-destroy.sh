#!/usr/bin/env bash
# Orca per-workspace environment recipe: release the Kata sandbox claimed by
# kata-create.sh. Orca passes the lifecycle payload (with recipeResult.userData)
# on stdin.
#
# Managed by KantoRegion/forca (recipe/kata-destroy.sh). Edit it there and run
# `forca sync`; local edits in a consumer repo are overwritten.
set -euo pipefail

log() { printf '[kata-destroy] %s\n' "$*" >&2; }

payload="$(cat)"
# Values are written by kata-create.sh ([a-z0-9-] names, plain paths), so a
# field match is enough and avoids requiring jq/node on the desktop.
field() { printf '%s' "$payload" | tr -d '\n' | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p"; }
claim="$(field resourceId)"
sandbox="$(field sandbox)"
namespace="$(field namespace)"
kubeconfig="$(field kubeconfig)"
emu_claim="$(field emuClaim)"
adb_port="$(field adbPort)"
[ -n "$claim" ] || { log "no resourceId in payload; nothing to release"; exit 1; }

kubectl_bin="${ORCA_KATA_KUBECTL:-$(command -v kubectl || true)}"
for c in /opt/homebrew/bin/kubectl /usr/local/bin/kubectl /usr/bin/kubectl; do
  [ -n "$kubectl_bin" ] && break
  [ -x "$c" ] && kubectl_bin="$c"
done
[ -n "$kubectl_bin" ] || { log "kubectl not found; set ORCA_KATA_KUBECTL"; exit 1; }

log "releasing claim $claim${emu_claim:+ and $emu_claim}"
"$kubectl_bin" ${kubeconfig:+--kubeconfig "$kubeconfig"} -n "${namespace:-agent-sandbox}" \
  delete sandboxclaim "$claim" ${emu_claim:+"$emu_claim"} --ignore-not-found --wait=false >&2
[ -n "$sandbox" ] && ssh-keygen -R "$sandbox" -f "$HOME/.ssh/known_hosts" >/dev/null 2>&1 || true

# Desktop side of the paired emulator (see kata-create.sh).
pf_pid_file="$HOME/.cache/orca-kata/$claim.port-forward.pid"
if [ -f "$pf_pid_file" ]; then
  pid="$(cat "$pf_pid_file")"
  pkill -P "$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  rm -f "$pf_pid_file"
fi
if [ -n "$adb_port" ]; then
  adb_bin="$(command -v adb || true)"
  [ -n "$adb_bin" ] || adb_bin="$HOME/Library/Android/sdk/platform-tools/adb"
  [ -x "$adb_bin" ] && "$adb_bin" disconnect "127.0.0.1:$adb_port" >/dev/null 2>&1 || true
fi
log "done"
