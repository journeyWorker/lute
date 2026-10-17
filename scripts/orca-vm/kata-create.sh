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
#
# Features (repos.tsv third column):
#   mirror    clone from the node-local forca cache (/forca in the sandbox:
#             git mirrors, LFS subset, warm pub/Gradle caches), then point
#             every remote back at GitHub so the sandbox owns its objects
#   emulator  claim a paired Android emulator (android-emu pool), connect the
#             sandbox's adb to it, and forward it to this desktop's adb so
#             Orca's Mobile Emulator panel (scrcpy) can show it
set -euo pipefail

DEFAULT_POOL=omp-kata-rust
FEATURES=""
NAMESPACE=agent-sandbox
SSHD_DIR=/home/dev/.orca-sshd
EMU_POOL=android-emu
STATE_DIR="$HOME/.cache/orca-kata"

log() { printf '[kata-create] %s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }
has() { case ",$FEATURES," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }

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
emu_claim=""
has emulator && emu_claim="$claim-emu"
pf_pid_file="$STATE_DIR/$claim.port-forward.pid"

cleanup_on_error() {
  log "create failed; releasing $claim${emu_claim:+ and $emu_claim}"
  k delete sandboxclaim "$claim" ${emu_claim:+"$emu_claim"} --ignore-not-found --wait=false >/dev/null 2>&1 || true
  if [ -f "$pf_pid_file" ]; then
    pid="$(cat "$pf_pid_file")"; pkill -P "$pid" 2>/dev/null || true; kill "$pid" 2>/dev/null || true
    rm -f "$pf_pid_file"
  fi
}
trap cleanup_on_error ERR

claim_from() {
  k apply -f - >&2 <<EOF
apiVersion: extensions.agents.x-k8s.io/v1beta1
kind: SandboxClaim
metadata:
  name: $1
  namespace: $NAMESPACE
  labels:
    app.kubernetes.io/managed-by: orca-recipe
spec:
  warmPoolRef:
    name: $2
EOF
}

# wait_claim <claim> <seconds>: prints the sandbox (pod) name once Ready
wait_claim() {
  local name="" _
  for _ in $(seq 1 90); do
    name="$(k get sandboxclaim "$1" -o jsonpath='{.status.sandbox.name}' 2>/dev/null || true)"
    [ -n "$name" ] && break
    sleep 2
  done
  [ -n "$name" ] || die "claim $1 got no sandbox within 180s"
  k wait pod "$name" --for=condition=Ready --timeout="${2}s" >&2
  echo "$name"
}

log "claiming $claim from pool $pool"
claim_from "$claim" "$pool"
if [ -n "$emu_claim" ]; then
  log "claiming $emu_claim from pool $EMU_POOL"
  claim_from "$emu_claim" "$EMU_POOL"
fi

sandbox="$(wait_claim "$claim" 300)"
log "sandbox $sandbox ready"
emu_pod=""; emu_addr=""
if [ -n "$emu_claim" ]; then
  # A warm emulator is already booted; a cold one needs ~2 minutes.
  emu_pod="$(wait_claim "$emu_claim" 600)"
  emu_addr="$(k get pod "$emu_pod" -o jsonpath='{.status.podIP}'):5555"
  log "emulator $emu_pod ready at $emu_addr"
fi

if [ ! -f "$ssh_key" ]; then
  log "creating desktop key $ssh_key"
  mkdir -p "$(dirname "$ssh_key")"
  ssh-keygen -q -t ed25519 -N '' -C "orca-kata@$(hostname -s)" -f "$ssh_key"
fi

# Prepare a user-mode sshd (the pod runs as dev, so no system sshd), the SSH
# session environment, the checkout, caches and the emulator connection.
# Inputs arrive on stdin, one per line. The last stdout line is the sandbox
# host key, read over the authenticated Kubernetes API rather than an
# unverified ssh-keyscan.
host_key="$(
  printf '%s\n%s\n%s\n%s\n%s\n%s\n%s\n%s\n' "$(cat "$ssh_key.pub")" "$repo_url" "$repo_ref" \
    "$project_root" "$git_name" "$git_email" "$FEATURES" "$emu_addr" |
    k exec -i "$sandbox" -- bash -c '
set -euo pipefail
read -r pub; read -r repo_url; read -r repo_ref; read -r project_root
IFS= read -r git_name || true; IFS= read -r git_email || true
IFS= read -r features || true; IFS= read -r emu_addr || true
has() { case ",$features," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
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
umask 022
gh auth setup-git >/dev/null 2>&1 || true
git config --global url."https://github.com/".insteadOf git@github.com:
[ -n "$git_name" ] && git config --global user.name "$git_name"
[ -n "$git_email" ] && git config --global user.email "$git_email"
export GIT_TERMINAL_PROMPT=0
extra_env=""

# forca cache key for a GitHub URL or slug: owner/repo -> owner__repo
key() { local s="${1#https://github.com/}"; s="${s#git@github.com:}"; s="${s%.git}"; echo "${s%%/*}__${s#*/}"; }

# lfs_from_mirror <checkout> <mirror>: copy the LFS objects this checkout needs
# (the subset the mirror tracks) from the read-only cache, then fill any gap
# from GitHub.
lfs_from_mirror() {
  local co="$1" m="$2" inc dst list
  [ -f "$m/forca-lfs-include" ] || return 0
  inc="$(cat "$m/forca-lfs-include")"
  dst="$(git -C "$co" rev-parse --git-common-dir)/lfs/objects"
  case "$dst" in /*) ;; *) dst="$co/$dst" ;; esac
  mkdir -p "$dst"; list="$(mktemp)"
  git -C "$co" lfs ls-files -l -I "$inc" | awk "{o=\$1; print substr(o,1,2)\"/\"substr(o,3,2)\"/\"o}" > "$list"
  rsync -a --ignore-missing-args --files-from="$list" "$m/lfs/objects/" "$dst/" >&2 || true
  rm -f "$list"
  git -C "$co" lfs checkout >&2 2>/dev/null || true
  git -C "$co" lfs pull --include "$inc" >&2
}

if [ ! -d "$project_root/.git" ]; then
  mirror="/forca/git/$(key "$repo_url").git"
  if has mirror && [ -d "$mirror" ]; then
    # Copy (never borrow) objects from the cache, then switch every remote to
    # GitHub: the checkout must not depend on the cache after creation.
    export GIT_LFS_SKIP_SMUDGE=1
    git clone --quiet --no-hardlinks --branch "$repo_ref" "$mirror" "$project_root" >&2
    git -C "$project_root" remote set-url origin "$repo_url"
    git -C "$project_root" fetch --quiet origin >&2
    git -C "$project_root" reset --quiet --hard "origin/$repo_ref"
    git -C "$project_root" submodule init --quiet
    git -C "$project_root" config --get-regexp "^submodule\..*\.url$" | while read -r name url; do
      sm="/forca/git/$(key "$url").git"
      if [ -d "$sm" ]; then git -C "$project_root" config "$name" "$sm"; fi
    done
    # From the cache first (git blocks file-transport submodules unless
    # allowed); commits newer than the last mirror sync come from GitHub on
    # the second pass.
    git -c protocol.file.allow=always -C "$project_root" submodule update --quiet --recursive >&2 2>/dev/null || true
    git -C "$project_root" submodule sync --quiet --recursive
    git -C "$project_root" submodule update --quiet --init --recursive >&2
    lfs_from_mirror "$project_root" "$mirror"
    git -C "$project_root" submodule foreach --quiet --recursive "echo \$displaypath \$(git remote get-url origin)" |
      while read -r path url; do
        lfs_from_mirror "$project_root/$path" "/forca/git/$(key "$url").git"
      done
    unset GIT_LFS_SKIP_SMUDGE
  else
    git clone --quiet --filter=blob:none --branch "$repo_ref" "$repo_url" "$project_root" >&2
    git -C "$project_root" submodule update --init --recursive --filter=blob:none --quiet >&2 || \
      echo "warning: submodule init failed" >&2
  fi
fi

# Warm Flutter caches built by the forca-mirror job: Gradle reads the shared
# dependency cache in place (GRADLE_RO_DEP_CACHE); pub and the Gradle wrapper
# are copied because they need a writable home. The seed (one build made at
# these same paths) adds Gradle transforms, Kotlin DSL and build cache plus
# .dart_tool, so the first build skips configuration, build hooks and the
# kernel when the sources match (eevee: 444 s -> about 210 s).
warm="/forca/warm/$(key "$repo_url")/current"
if has mirror && [ -d "$warm" ]; then
  warm="$(readlink -f "$warm")"
  [ -d "$warm/pub" ] && rsync -a "$warm/pub/" "$HOME/.pub-cache/" >&2
  [ -d "$warm/gradle/wrapper" ] && mkdir -p "$HOME/.gradle" && rsync -a "$warm/gradle/wrapper" "$HOME/.gradle/" >&2
  [ -d "$warm/gradle/caches/modules-2" ] && extra_env="$extra_env GRADLE_RO_DEP_CACHE=$warm/gradle/caches"
  if [ -f "$warm/seed/.complete" ]; then
    app="$project_root/flutter"; [ -f "$app/pubspec.yaml" ] || app="$project_root"
    [ -d "$warm/seed/gradle-caches" ] && mkdir -p "$HOME/.gradle/caches" &&
      rsync -a "$warm/seed/gradle-caches/" "$HOME/.gradle/caches/" >&2
    [ -d "$warm/seed/dart_tool" ] && [ ! -e "$app/.dart_tool" ] &&
      rsync -a "$warm/seed/dart_tool/" "$app/.dart_tool/" >&2
  fi
fi

if [ -n "$emu_addr" ]; then
  adb connect "$emu_addr" >&2
  extra_env="$extra_env FORCA_EMULATOR=$emu_addr ANDROID_SERIAL=$emu_addr"
fi

# SSH sessions do not inherit the container env (toolchain PATH, omp broker,
# git token, SDK homes); sshd reads it from here for every session.
umask 077
{
  env | grep -E "^(PATH|OMP_AUTH_BROKER_[A-Z_]*|GH_TOKEN|TOKEN_GITHUB|BUN_INSTALL[A-Z_]*|CARGO_HOME|RUSTUP_HOME|UV_[A-Z_]*|ANDROID_[A-Z_]*|JAVA_HOME|FLUTTER_[A-Z_]*|LANG|LC_ALL)="
  for kv in $extra_env; do echo "$kv"; done
} > "$HOME/.ssh/environment"
cut -d" " -f1,2 "$D/host_ed25519.pub"
' | tail -n 1
)"
case "$host_key" in ssh-ed25519\ *) ;; *) die "could not read sandbox host key" ;; esac

# omp defaults (models, roles, compaction, ...) and custom agents so the first
# launch in the workspace is not a fresh install.
omp_config="$(dirname "$0")/omp-config.yml"
omp_agents="$(dirname "$0")/omp-agents"
[ -f "$omp_config" ] && [ -d "$omp_agents" ] || die "missing $omp_config or $omp_agents (run forca sync)"
k exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.omp/agent" && cat > "$HOME/.omp/agent/config.yml"' < "$omp_config" >&2
# bsdtar (macOS) would add AppleDouble files and provenance xattr headers that
# GNU tar in the sandbox warns about.
tar_flags=""
tar --version 2>/dev/null | grep -q bsdtar && tar_flags="--no-xattrs --no-mac-metadata"
COPYFILE_DISABLE=1 tar $tar_flags -C "$omp_agents" -cf - . |
  k exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.omp/agent/agents" && tar -C "$HOME/.omp/agent/agents" -xf -' >&2

# "Kata Sandbox + Backend" (kata-create-backend.sh): the repo's own setup flow
# (eevee development-workflow guide): DOPPLER_TOKEN in the root .env, then
# `bun run setup` (dependencies, Doppler dev_local env, local DB migrate). The
# token is a read-only service token in Secret forca-backend-<repo>; it only
# lands in this workspace's .env. `adb reverse` makes the emulator's
# localhost:3001 the sandbox API, so `run:local` works unchanged; Marionette
# lets omp drive the running app (flutter-dev skill).
if [ "${ORCA_KATA_BACKEND:-0}" = 1 ]; then
  has backend || die "$repo_name has no backend feature in forca repos.tsv"
  backend_secret="forca-backend-$repo_name"
  doppler_token="$(k get secret "$backend_secret" -o jsonpath='{.data.DOPPLER_TOKEN}' | base64 -d)"
  [ -n "$doppler_token" ] || die "secret $backend_secret has no DOPPLER_TOKEN"
  log "backend: bun run setup (several minutes), adb reverse :3001, Marionette"
  printf '%s\n%s\n%s\n' "$doppler_token" "$project_root" "$emu_addr" |
    k exec -i "$sandbox" -- bash -c '
set -euo pipefail
read -r token; read -r project_root; IFS= read -r emu_addr || true
cd "$project_root"
if [ ! -f .env ]; then (umask 077; printf "DOPPLER_TOKEN=%s\n" "$token" > .env); fi
bun run setup >&2
if [ -n "$emu_addr" ]; then adb -s "$emu_addr" reverse tcp:3001 tcp:3001 >&2; fi
bun run flutter:marionette setup --providers oh-my-pi >&2 ||
  echo "warning: Marionette setup failed; see the flutter-dev skill" >&2
' >&2
  unset doppler_token
fi

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

# Desktop side of the emulator: a self-restarting port-forward to the
# emulator's adbd plus `adb connect`, so the device shows up in Orca's Mobile
# Emulator panel. The emulator itself runs on the cluster, never here.
adb_port=""
if [ -n "$emu_pod" ]; then
  adb_bin="$(command -v adb || true)"
  [ -n "$adb_bin" ] || [ ! -x "$HOME/Library/Android/sdk/platform-tools/adb" ] ||
    adb_bin="$HOME/Library/Android/sdk/platform-tools/adb"
  if [ -z "$adb_bin" ]; then
    log "no adb on this desktop; skipping the local emulator connection"
  else
    mkdir -p "$STATE_DIR"
    adb_port=15555
    while nc -z 127.0.0.1 "$adb_port" >/dev/null 2>&1; do adb_port=$((adb_port + 1)); done
    nohup bash -c "while :; do '$kubectl_bin' --kubeconfig '$kubeconfig' -n $NAMESPACE port-forward --address 127.0.0.1 pod/$emu_pod $adb_port:5555 >/dev/null 2>&1; sleep 2; done" \
      >/dev/null 2>&1 &
    echo $! > "$pf_pid_file"
    for _ in $(seq 1 30); do nc -z 127.0.0.1 "$adb_port" >/dev/null 2>&1 && break; sleep 1; done
    "$adb_bin" connect "127.0.0.1:$adb_port" >&2 || log "adb connect failed; run it by hand: adb connect 127.0.0.1:$adb_port"
  fi
fi

trap - ERR
log "ready: $sandbox ($project_root)${emu_pod:+, emulator $emu_pod (desktop adb 127.0.0.1:$adb_port)}"
printf '{"schemaVersion":1,"connection":{"type":"ssh","projectRoot":"%s","target":{"label":"kata-%s","host":"%s","port":22,"username":"dev","identityFile":"%s","identitiesOnly":true,"proxyCommand":"%s"}},"userData":{"provider":"agent-sandbox","resourceId":"%s","sandbox":"%s","namespace":"%s","kubeconfig":"%s","emuClaim":"%s","adbPort":"%s"}}\n' \
  "$project_root" "$repo_name" "$sandbox" "$ssh_key" "$proxy_command" "$claim" "$sandbox" "$NAMESPACE" "$kubeconfig" "$emu_claim" "$adb_port"
