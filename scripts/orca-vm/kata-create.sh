#!/usr/bin/env bash
# Orca per-workspace environment recipe: claim a Kata sandbox on the intranet
# k3s cluster and hand Orca an SSH target that tunnels through
# `kubectl exec ... sshd -i` (no Service, no open port).
#
# Runs on the desktop from the repo root. stdout carries only the recipe JSON;
# progress goes to stderr.
#
# Managed by KantoRegion/forca (recipe/kata-create.sh). Edit it there and run
# `forca sync`; local edits in a consumer repo are overwritten.
#
# Desktop prerequisites: kubectl and a kubeconfig for the intranet cluster.
#   ORCA_KATA_KUBECONFIG  kubeconfig path (default ~/.config/orca-kata/kubeconfig,
#                         then $KUBECONFIG, then ~/.kube/config)
#   ORCA_KATA_KUBECTL     kubectl binary (default: PATH, then Homebrew paths)
#   ORCA_KATA_POOL        SandboxWarmPool to claim from (default below)
#   ORCA_KATA_SSH_KEY     desktop key Orca dials with (default ~/.ssh/orca-kata_ed25519)
set -euo pipefail

DEFAULT_POOL=omp-kata-rust
NAMESPACE=agent-sandbox
SSHD_DIR=/home/dev/.orca-sshd

log() { printf '[kata-create] %s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

kubectl_bin="${ORCA_KATA_KUBECTL:-$(command -v kubectl || true)}"
for c in /opt/homebrew/bin/kubectl /usr/local/bin/kubectl /usr/bin/kubectl; do
  [ -n "$kubectl_bin" ] && break
  [ -x "$c" ] && kubectl_bin="$c"
done
[ -n "$kubectl_bin" ] || die "kubectl not found; set ORCA_KATA_KUBECTL"

kubeconfig="${ORCA_KATA_KUBECONFIG:-}"
for c in "$HOME/.config/orca-kata/kubeconfig" "${KUBECONFIG:-}" "$HOME/.kube/config"; do
  [ -n "$kubeconfig" ] && break
  [ -n "$c" ] && [ -f "$c" ] && kubeconfig="$c"
done
[ -n "$kubeconfig" ] || die "no kubeconfig; set ORCA_KATA_KUBECONFIG"

pool="${ORCA_KATA_POOL:-$DEFAULT_POOL}"
ssh_key="${ORCA_KATA_SSH_KEY:-$HOME/.ssh/orca-kata_ed25519}"
k() { "$kubectl_bin" --kubeconfig "$kubeconfig" -n "$NAMESPACE" "$@"; }

# Repo to check out in the sandbox: this checkout's origin, over HTTPS so the
# sandbox's GH_TOKEN can authenticate.
origin_url="$(git remote get-url origin)"
case "$origin_url" in
  git@github.com:*) repo_url="https://github.com/${origin_url#git@github.com:}" ;;
  ssh://git@github.com/*) repo_url="https://github.com/${origin_url#ssh://git@github.com/}" ;;
  *) repo_url="$origin_url" ;;
esac
repo_name="$(basename "${repo_url%.git}")"
repo_ref="$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || echo origin/main)"
repo_ref="${repo_ref#origin/}"
project_root="/workspace/$repo_name"
git_name="$(git config user.name || true)"
git_email="$(git config user.email || true)"

instance="${ORCA_VM_INSTANCE_ID:-$(date +%s)}"
instance="${instance#orca-}"
claim="$(printf 'orca-%s-%s' "$repo_name" "$instance" | tr 'A-Z_.' 'a-z--' | tr -cd 'a-z0-9-' | cut -c1-55)"
claim="${claim%-}"

cleanup_on_error() {
  log "create failed; releasing claim $claim"
  k delete sandboxclaim "$claim" --ignore-not-found --wait=false >/dev/null 2>&1 || true
}
trap cleanup_on_error ERR

log "claiming $claim from pool $pool"
k apply -f - >&2 <<EOF
apiVersion: extensions.agents.x-k8s.io/v1beta1
kind: SandboxClaim
metadata:
  name: $claim
  namespace: $NAMESPACE
  labels:
    app.kubernetes.io/managed-by: orca-recipe
spec:
  warmPoolRef:
    name: $pool
EOF

sandbox=""
for _ in $(seq 1 90); do
  sandbox="$(k get sandboxclaim "$claim" -o jsonpath='{.status.sandbox.name}' 2>/dev/null || true)"
  [ -n "$sandbox" ] && break
  sleep 2
done
[ -n "$sandbox" ] || die "claim $claim got no sandbox within 180s"
k wait pod "$sandbox" --for=condition=Ready --timeout=300s >&2
log "sandbox $sandbox ready"

if [ ! -f "$ssh_key" ]; then
  log "creating desktop key $ssh_key"
  mkdir -p "$(dirname "$ssh_key")"
  ssh-keygen -q -t ed25519 -N '' -C "orca-kata@$(hostname -s)" -f "$ssh_key"
fi

# Prepare a user-mode sshd (the pod runs as dev, so no system sshd), the SSH
# session environment and the checkout. Inputs arrive on stdin, one per line.
# The last stdout line is the sandbox host key, read over the authenticated
# Kubernetes API rather than an unverified ssh-keyscan.
host_key="$(
  printf '%s\n%s\n%s\n%s\n%s\n%s\n' "$(cat "$ssh_key.pub")" "$repo_url" "$repo_ref" \
    "$project_root" "$git_name" "$git_email" |
    k exec -i "$sandbox" -- bash -c '
set -euo pipefail
read -r pub; read -r repo_url; read -r repo_ref; read -r project_root
IFS= read -r git_name || true; IFS= read -r git_email || true
D='"$SSHD_DIR"'
umask 077
mkdir -p "$D" "$HOME/.ssh"
[ -f "$D/host_ed25519" ] || ssh-keygen -q -t ed25519 -N "" -f "$D/host_ed25519"
printf "%s\n" "$pub" > "$HOME/.ssh/authorized_keys"
cat > "$D/sshd_config" <<CFG
HostKey $D/host_ed25519
AuthorizedKeysFile $HOME/.ssh/authorized_keys
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
PidFile none
PermitUserEnvironment yes
AllowTcpForwarding yes
Subsystem sftp internal-sftp
CFG
# SSH sessions do not inherit the container env (toolchain PATH, omp broker,
# git token); sshd reads it from here for every session.
env | grep -E "^(PATH|OMP_AUTH_BROKER_[A-Z_]*|GH_TOKEN|TOKEN_GITHUB|BUN_INSTALL[A-Z_]*|CARGO_HOME|RUSTUP_HOME|UV_[A-Z_]*|LANG|LC_ALL)=" > "$HOME/.ssh/environment"
umask 022
gh auth setup-git >/dev/null 2>&1 || true
[ -n "$git_name" ] && git config --global user.name "$git_name"
[ -n "$git_email" ] && git config --global user.email "$git_email"
export GIT_TERMINAL_PROMPT=0
if [ ! -d "$project_root/.git" ]; then
  git clone --quiet --filter=blob:none --branch "$repo_ref" "$repo_url" "$project_root" >&2
  git -C "$project_root" submodule update --init --recursive --filter=blob:none --quiet >&2 || \
    echo "warning: submodule init failed" >&2
fi
cut -d" " -f1,2 "$D/host_ed25519.pub"
' | tail -n 1
)"
case "$host_key" in ssh-ed25519\ *) ;; *) die "could not read sandbox host key" ;; esac

# Orca dials with BatchMode and StrictHostKeyChecking: trust this sandbox's key
# under its own alias (removed again by kata-destroy.sh).
known_hosts="$HOME/.ssh/known_hosts"
touch "$known_hosts"
ssh-keygen -R "$sandbox" -f "$known_hosts" >/dev/null 2>&1 || true
printf '%s %s\n' "$sandbox" "$host_key" >> "$known_hosts"

proxy_command="'$kubectl_bin' --kubeconfig '$kubeconfig' -n $NAMESPACE exec -i $sandbox -- /usr/sbin/sshd -i -e -o LogLevel=QUIET -f $SSHD_DIR/sshd_config"

log "smoke-testing SSH"
ssh -o BatchMode=yes -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -i "$ssh_key" \
  -o "ProxyCommand=$proxy_command" "dev@$sandbox" \
  "test -d '$project_root/.git' && command -v omp >/dev/null" >&2 ||
  die "SSH smoke test failed"

trap - ERR
log "ready: $sandbox ($project_root)"
printf '{"schemaVersion":1,"connection":{"type":"ssh","projectRoot":"%s","target":{"label":"kata-%s","host":"%s","port":22,"username":"dev","identityFile":"%s","identitiesOnly":true,"proxyCommand":"%s"}},"userData":{"provider":"agent-sandbox","resourceId":"%s","sandbox":"%s","namespace":"%s","kubeconfig":"%s"}}\n' \
  "$project_root" "$repo_name" "$sandbox" "$ssh_key" "$proxy_command" "$claim" "$sandbox" "$NAMESPACE" "$kubeconfig"
