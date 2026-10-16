#!/usr/bin/env bash
# Orca per-workspace environment recipe: release the Kata sandbox claimed by
# kata-create.sh. Orca passes the lifecycle payload (with recipeResult.userData)
# on stdin. Keep identical across bard, lute and canon.
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
[ -n "$claim" ] || { log "no resourceId in payload; nothing to release"; exit 1; }

kubectl_bin="${ORCA_KATA_KUBECTL:-$(command -v kubectl || true)}"
for c in /opt/homebrew/bin/kubectl /usr/local/bin/kubectl /usr/bin/kubectl; do
  [ -n "$kubectl_bin" ] && break
  [ -x "$c" ] && kubectl_bin="$c"
done
[ -n "$kubectl_bin" ] || { log "kubectl not found; set ORCA_KATA_KUBECTL"; exit 1; }

log "releasing claim $claim"
"$kubectl_bin" ${kubeconfig:+--kubeconfig "$kubeconfig"} -n "${namespace:-agent-sandbox}" \
  delete sandboxclaim "$claim" --ignore-not-found --wait=false >&2
[ -n "$sandbox" ] && ssh-keygen -R "$sandbox" -f "$HOME/.ssh/known_hosts" >/dev/null 2>&1 || true
log "done"
