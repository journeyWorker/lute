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
#   ORCA_KATA_RETRY_BUDGET  seconds a single cluster call may keep retrying
#                         transient network errors (default 300)
#
# Network outages: every cluster call goes through `kr`, which retries
# transient transport errors (no route, timeouts, resets, EOF, 5xx) with
# backoff for up to the retry budget and fails at once on deterministic ones.
# The long setup (clone, LFS, caches, backend) runs detached inside the
# sandbox and is polled with short execs, so a dropped connection never
# kills it: the poll just resumes where it left off.
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
RETRY_BUDGET="${ORCA_KATA_RETRY_BUDGET:-300}"
CLEANUP_BUDGET=90

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

# Provisioned root: the workspace IS this sandbox checkout (orca.yaml sets
# checkoutMode: provisioned-root). Without it Orca adds a bare linked worktree
# next to it, missing submodules, LFS assets, .env and the Flutter seed that
# this script prepares. Orca passes the branch to create at the exact commit
# it resolved; `orca vm recipe doctor` passes neither, and then the checkout
# stays on the default branch.
result_schema="${ORCA_RECIPE_RESULT_SCHEMA_VERSION:-1}"
checkout_branch="" checkout_head="" checkout_ref=""
if [ "$result_schema" = 2 ] && [ -n "${ORCA_REPO_REF_HEAD:-}" ]; then
  [ -n "${ORCA_REPO_BRANCH:-}" ] || die "ORCA_REPO_REF_HEAD set without ORCA_REPO_BRANCH"
  checkout_branch="$ORCA_REPO_BRANCH" checkout_head="$ORCA_REPO_REF_HEAD"
  # Orca resolves the base ref on the desktop remote (e.g. origin/main); the
  # sandbox fetches the same branch from GitHub.
  checkout_ref="${ORCA_REPO_REF:-$repo_ref}"
  checkout_ref="${checkout_ref#refs/remotes/}"; checkout_ref="${checkout_ref#refs/heads/}"
  checkout_ref="${checkout_ref#origin/}"
fi

instance="${ORCA_VM_INSTANCE_ID:-$(date +%s)}"
instance="${instance#orca-}"
claim="$(printf 'orca-%s-%s' "$repo_name" "$instance" | tr 'A-Z_.' 'a-z--' | tr -cd 'a-z0-9-' | cut -c1-55)"
claim="${claim%-}"
emu_claim=""
has emulator && emu_claim="$claim-emu"

# Scratch space for kubectl output and uploads (0700: it can hold the backend
# token). Removed on exit.
mkdir -p "$STATE_DIR"
work="$STATE_DIR/$claim.d"
rm -rf "$work"
mkdir -m 700 "$work"

# kr: the only way this script talks to the cluster after the claim.
#   kr [-f FILE] <kubectl-verb> <args...>
# -f feeds FILE to stdin (for `exec -i`); stdout lands in $KR_OUT, stderr is
# shown (unless KR_QUIET is set and the call succeeded). Transient transport
# errors are retried with backoff until RETRY_BUDGET seconds have passed since
# the first failure of this call; anything else fails at once with kubectl's
# exit code. kubectl runs in the background and is awaited with `wait`, so a
# TERM to this script is handled immediately instead of after the call.
kr_child=""
KR_OUT=""

nap() { sleep "$1" & kr_child=$!; wait "$kr_child" || true; kr_child=""; }

# transient <errfile>: 0 when the error is a network/server blip worth retrying
transient() {
  grep -Eiq 'not found|does not exist|forbidden|unauthorized|is invalid|invalid value|already exists|bad request|executable file|permission denied|command terminated with exit code' "$1" && return 1
  grep -Eiq 'connection refused|connection reset|no route to host|network is unreachable|host is down|i/o timeout|connection timed out|broken pipe|tls handshake timeout|\bEOF\b|unable to upgrade connection|websocket|context deadline exceeded|client\.timeout|timeout occurred|request did not complete|etcdserver: (request timed out|leader changed)|serviceunavailable|internal ?error|currently unable to handle|status code 50[0-9]|too ?many ?requests|error dialing backend|lost connection|stream error|http2:|connection error|use of closed network' "$1"
}

kr() {
  local infile=/dev/null rc first=-1 delay=2 verb extra=""
  if [ "${1:-}" = -f ]; then infile="$2"; shift 2; fi
  verb="$1"; shift
  case "$verb" in get|apply|delete) extra=--request-timeout=30s ;; esac
  KR_OUT=""
  while :; do
    "$kubectl_bin" --kubeconfig "$kubeconfig" -n "$NAMESPACE" "$verb" ${extra:+"$extra"} "$@" \
      <"$infile" >"$work/out" 2>"$work/err" &
    kr_child=$!
    rc=0; wait "$kr_child" || rc=$?
    kr_child=""
    if [ "$rc" = 0 ]; then
      [ -n "${KR_QUIET:-}" ] || cat "$work/err" >&2
      KR_OUT="$(cat "$work/out"; printf x)"; KR_OUT="${KR_OUT%x}"
      return 0
    fi
    if ! transient "$work/err"; then cat "$work/err" >&2; return "$rc"; fi
    [ "$first" -ge 0 ] || first=$SECONDS
    if [ $((SECONDS - first)) -ge "$RETRY_BUDGET" ]; then
      log "giving up on kubectl $verb after ${RETRY_BUDGET}s of network errors"
      cat "$work/err" >&2; return "$rc"
    fi
    log "kubectl $verb: $(sed -n '1p' "$work/err" | cut -c1-160); retrying in ${delay}s ($((RETRY_BUDGET - SECONDS + first))s left)"
    nap "$delay"
    delay=$((delay * 2)); [ "$delay" -le 15 ] || delay=15
  done
}

# Any exit before the result JSON is printed releases the claims: `die`
# (exit 1, which an ERR trap misses), a failed command under set -e, and
# Orca stopping the recipe (TERM/INT/HUP). Without this a failed create left
# a claimed VM and emulator running with no Orca workspace to delete them.
# The delete itself retries through an outage; signals are ignored meanwhile
# so a second TERM cannot abort the release.
created=0
cleanup_on_exit() {
  trap '' TERM INT HUP
  [ -z "$kr_child" ] || kill "$kr_child" 2>/dev/null || true
  if [ "$created" != 1 ]; then
    log "create failed; releasing $claim${emu_claim:+ and $emu_claim}"
    KR_QUIET=1 RETRY_BUDGET="$CLEANUP_BUDGET" kr delete sandboxclaim "$claim" ${emu_claim:+"$emu_claim"} --ignore-not-found --wait=false ||
      log "could not release the claim; run: kubectl --kubeconfig '$kubeconfig' -n $NAMESPACE delete sandboxclaim $claim${emu_claim:+ $emu_claim}"
    if [ -x "$(dirname "$0")/forca" ]; then
      ORCA_KATA_KUBECTL="$kubectl_bin" "$(dirname "$0")/forca" emulator disconnect --sandbox "$claim" >&2 </dev/null || true
    fi
  fi
  rm -rf "$work"
}
trap cleanup_on_exit EXIT
trap 'exit 1' TERM INT HUP

# claim_from <claim> <pool> [<owner claim uid>]: idempotent (apply), so a retry
# after a lost response is harmless. With an owner uid the claim is an
# emulator of the workspace claim $claim (label, ownerReference).
claim_from() {
  local f="$work/claim.yaml"
  cat > "$f" <<EOF
apiVersion: extensions.agents.x-k8s.io/v1beta1
kind: SandboxClaim
metadata:
  name: $1
  namespace: $NAMESPACE
  labels:
    app.kubernetes.io/managed-by: orca-recipe
EOF
  if [ -n "${3:-}" ]; then
    cat >> "$f" <<EOF
    vivident.dev/workspace: $claim
    vivident.dev/emulator-profile: phone
  ownerReferences:
    - apiVersion: extensions.agents.x-k8s.io/v1beta1
      kind: SandboxClaim
      name: $claim
      uid: $3
EOF
  fi
  cat >> "$f" <<EOF
spec:
  warmPoolRef:
    name: $2
EOF
  kr -f "$f" apply -f -
  printf '%s' "$KR_OUT" >&2
}

# wait_claim <claim> <seconds>: sets WAIT_NAME to the sandbox (pod) name once Ready
WAIT_NAME=""
wait_claim() {
  local name="" _
  for _ in $(seq 1 90); do
    name=""
    if KR_QUIET=1 kr get sandboxclaim "$1" -o jsonpath='{.status.sandbox.name}'; then name="$KR_OUT"; fi
    [ -n "$name" ] && break
    nap 2
  done
  [ -n "$name" ] || die "claim $1 got no sandbox within 180s"
  kr wait pod "$name" --for=condition=Ready --timeout="${2}s"
  printf '%s' "$KR_OUT" >&2
  WAIT_NAME="$name"
}

# make_payload <file> <script> <arg>...: the upload format of remote_run. Line 1
# is the argument count, then one line per argument, then the script, then an
# end marker the sandbox checks so a cut-off upload is never started.
make_payload() {
  local f="$1" script="$2"; shift 2
  {
    printf '%s\n' "$#"
    if [ "$#" -gt 0 ]; then printf '%s\n' "$@"; fi
    printf '%s\n' "$script"
    printf '#@@END@@\n'
  } > "$f"
}

# Guest side of remote_run, run with `sh -c`. $1 is the run name. Starts the
# uploaded script detached (own session, no stream attached) unless this run
# was already started, so repeating the call after a network error never
# starts a second copy. State lives in ~/.orca-setup/<name>/: args, run.sh,
# log, pid, rc (written last, atomically).
read -r -d '' launcher <<'LAUNCHER' || true
D="$HOME/.orca-setup/$1"
mkdir -p "$D"
if [ -e "$D/started" ]; then cat > /dev/null; echo "remote $1 already started" >&2; exit 0; fi
umask 077
cat > "$D/payload.part"
[ "$(tail -n 1 "$D/payload.part")" = "#@@END@@" ] || { echo "upload of $1 was cut off" >&2; exit 3; }
mv "$D/payload.part" "$D/payload"
n="$(head -n 1 "$D/payload")"
sed -n "2,$((n + 1))p" "$D/payload" > "$D/args"
tail -n +"$((n + 2))" "$D/payload" > "$D/run.sh"
rm -f "$D/payload"
: > "$D/log"
touch "$D/started"
RUNNER='echo $$ > "$1/pid"; export ORCA_RUN_DIR="$1"; cd "$HOME"
bash "$1/run.sh" "$1/args" < /dev/null > "$1/log" 2>&1
echo $? > "$1/rc.tmp"; mv "$1/rc.tmp" "$1/rc"; rm -f "$1/args"'
if command -v setsid > /dev/null 2>&1; then
  setsid nohup sh -c "$RUNNER" sh "$D" > /dev/null 2>&1 < /dev/null &
else
  nohup sh -c "$RUNNER" sh "$D" > /dev/null 2>&1 < /dev/null &
fi
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$D/pid" ] && exit 0; sleep 0.5; done
rm -f "$D/started"
echo "remote $1 did not start" >&2
exit 4
LAUNCHER

# Guest side of the poll: "<new log bytes> <exit code or -> <runner alive>",
# then exactly that many log bytes from offset $2. The exit code is read
# before the log size, so once it is set the log is complete.
read -r -d '' poller <<'POLLER' || true
D="$HOME/.orca-setup/$1"; off=$2
rc=-; [ -f "$D/rc" ] && rc="$(cat "$D/rc")"; [ -n "$rc" ] || rc=-
pid="$(cat "$D/pid" 2>/dev/null || true)"; alive=0
[ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && alive=1
size=$(( $(wc -c < "$D/log" 2>/dev/null || echo 0) ))
n=$((size - off)); [ "$n" -ge 0 ] || n=0
echo "$n $rc $alive"
if [ "$n" -gt 0 ]; then tail -c +"$((off + 1))" "$D/log" | head -c "$n"; fi
POLLER

# remote_run <name> <payload-file>: start (idempotent) and follow a detached
# script in the sandbox. The sandbox log is streamed to stderr (Orca's
# provisioning log). Poll errors go through kr, so an outage up to the retry
# budget only pauses the output; the offset makes the stream resume exactly.
remote_run() {
  local name="$1" payload="$2" off=0 bad=0 dead=0 raw hdr body n rc alive got
  log "remote $name: starting in $sandbox"
  kr -f "$payload" exec -i "$sandbox" -- sh -c "$launcher" sh "$name"
  while :; do
    KR_QUIET=1 kr exec "$sandbox" -- sh -c "$poller" sh "$name" "$off" ||
      die "lost contact with $sandbox while running remote $name"
    raw="$KR_OUT"
    hdr="${raw%%$'\n'*}"; body="${raw#*$'\n'}"
    case "$raw" in *$'\n'*) ;; *) hdr="" ;; esac
    n="${hdr%% *}"; rc="${hdr#* }"; alive="${rc##* }"; rc="${rc%% *}"
    case "$n$alive" in ''|*[!0-9]*) hdr="" ;; esac
    if [ -z "$hdr" ]; then
      bad=$((bad + 1)); [ "$bad" -lt 5 ] || die "unreadable poll reply for remote $name"
      nap 2; continue
    fi
    if [ "$n" -gt 0 ]; then
      got="$(printf '%s' "$body" | wc -c | tr -d ' ')"
      if [ "$got" != "$n" ] && [ "$bad" -lt 3 ]; then bad=$((bad + 1)); nap 2; continue; fi
      printf '%s' "$body" >&2
      off=$((off + n))
    fi
    bad=0
    if [ "$rc" != "-" ]; then
      [ "$rc" = 0 ] && return 0
      KR_QUIET=1 kr exec "$sandbox" -- sh -c 'tail -n 40 "$HOME/.orca-setup/$1/log"' sh "$name" || true
      log "remote $name failed with exit code $rc; last log lines:"
      printf '%s' "$KR_OUT" >&2
      die "remote $name failed (exit $rc)"
    fi
    if [ "$alive" = 0 ]; then
      dead=$((dead + 1)); [ "$dead" -lt 3 ] || die "remote $name stopped without a result"
    else
      dead=0
    fi
    nap 5
  done
}

log "claiming $claim from pool $pool"
claim_from "$claim" "$pool"
if [ -n "$emu_claim" ]; then
  # The emulator claim is owned by the workspace claim (deleting the workspace
  # deletes it) and labelled so `forca emulator` finds it.
  kr get sandboxclaim "$claim" -o jsonpath='{.metadata.uid}'
  [ -n "$KR_OUT" ] || die "claim $claim has no uid"
  log "claiming $emu_claim from pool $EMU_POOL"
  claim_from "$emu_claim" "$EMU_POOL" "$KR_OUT"
fi

wait_claim "$claim" 300; sandbox="$WAIT_NAME"
log "sandbox $sandbox ready"
emu_pod=""; emu_addr=""
if [ -n "$emu_claim" ]; then
  # A warm emulator is already booted; a cold one needs ~2 minutes.
  wait_claim "$emu_claim" 600; emu_pod="$WAIT_NAME"
  # The sandbox may stop this emulator itself (forca emulator stop); the
  # admission policy lets it delete only claims labelled with its own pod.
  kr label sandboxclaim "$emu_claim" "vivident.dev/workspace-pod=$sandbox" --overwrite
  kr get pod "$emu_pod" -o jsonpath='{.status.podIP}'
  emu_addr="$KR_OUT:5555"
  log "emulator $emu_pod ready at $emu_addr"
fi

if [ ! -f "$ssh_key" ]; then
  log "creating desktop key $ssh_key"
  mkdir -p "$(dirname "$ssh_key")"
  ssh-keygen -q -t ed25519 -N '' -C "orca-kata@$(hostname -s)" -f "$ssh_key"
fi

# Prepare a user-mode sshd (the pod runs as dev, so no system sshd), the SSH
# session environment, the checkout, caches and the emulator connection. Runs
# detached in the sandbox; arguments are read from the file given as $1. The
# last step writes the sandbox host key to $ORCA_RUN_DIR/host_key, read below
# over the authenticated Kubernetes API rather than an unverified ssh-keyscan.
read -r -d '' setup_script <<'REMOTE' || true
set -euo pipefail
{
  read -r pub; read -r repo_url; read -r repo_ref; read -r project_root
  IFS= read -r git_name || true; IFS= read -r git_email || true
  IFS= read -r features || true; IFS= read -r emu_addr || true
  IFS= read -r checkout_branch || true; IFS= read -r checkout_head || true
  IFS= read -r checkout_ref || true; IFS= read -r D || true
  IFS= read -r ws_claim || true; IFS= read -r ws_pod || true
} < "$1"
has() { case ",$features," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
umask 077
mkdir -p "$D" "$HOME/.ssh" "$HOME/.config/rclone"
# The nas remote comes from the environment; an empty config file keeps rclone
# from printing "config file not found" on every run (images before 2026-10-20
# do not ship one in /etc/skel).
[ -e "$HOME/.config/rclone/rclone.conf" ] || : > "$HOME/.config/rclone/rclone.conf"
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

# pin_checkout <checkout>: provisioned root; put the workspace branch at the
# exact commit Orca resolved (fetching it only if the clone lacks it).
pin_checkout() {
  [ -n "$checkout_head" ] || return 0
  git -C "$1" cat-file -e "$checkout_head^{commit}" 2>/dev/null ||
    git -C "$1" fetch --quiet origin "$checkout_ref" >&2
  git -C "$1" cat-file -e "$checkout_head^{commit}"
  git -C "$1" checkout --quiet -B "$checkout_branch" "$checkout_head" >&2
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
    pin_checkout "$project_root"
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
    pin_checkout "$project_root"
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
# The sandbox's own claim and pod name: `forca emulator` finds its workspace
# with them (claims it starts are owned by FORCA_SANDBOX_CLAIM and labelled
# with FORCA_SANDBOX_POD, which the admission policy checks).
[ -z "$ws_claim" ] || extra_env="$extra_env FORCA_SANDBOX_CLAIM=$ws_claim"
[ -z "$ws_pod" ] || extra_env="$extra_env FORCA_SANDBOX_POD=$ws_pod"

# SSH sessions do not inherit the container env (toolchain PATH, omp broker,
# git token, SDK homes, DOCKER_HOST of the dev-flutter podman engine); sshd
# reads it from here for every session (UsePAM no, so /etc/environment is not
# applied).
umask 077
{
  env | grep -E "^(PATH|OMP_AUTH_BROKER_[A-Z_]*|GH_TOKEN|TOKEN_GITHUB|BUN_INSTALL[A-Z_]*|CARGO_HOME|RUSTUP_HOME|UV_[A-Z_]*|ANDROID_[A-Z_]*|JAVA_HOME|FLUTTER_[A-Z_]*|RCLONE_CONFIG_NAS_[A-Z_]*|DOCKER_[A-Z_]*|COMPOSE_[A-Z_]*|LANG|LC_ALL)="
  for kv in $extra_env; do echo "$kv"; done
} > "$HOME/.ssh/environment"
cut -d" " -f1,2 "$D/host_ed25519.pub" > "$ORCA_RUN_DIR/host_key"
REMOTE

make_payload "$work/setup.payload" "$setup_script" \
  "$(cat "$ssh_key.pub")" "$repo_url" "$repo_ref" "$project_root" "$git_name" "$git_email" \
  "$FEATURES" "$emu_addr" "$checkout_branch" "$checkout_head" "$checkout_ref" "$SSHD_DIR" "$claim" "$sandbox"
remote_run setup "$work/setup.payload"
kr exec "$sandbox" -- sh -c 'cat "$HOME/.orca-setup/setup/host_key"'
host_key="${KR_OUT%%$'\n'*}"
case "$host_key" in ssh-ed25519\ *) ;; *) die "could not read sandbox host key" ;; esac

# omp defaults (models, roles, compaction, ...), user-level context and custom
# agents so the first launch in the workspace is not a fresh install.
omp_config="$(dirname "$0")/omp-config.yml"
omp_agents="$(dirname "$0")/omp-agents"
omp_context="$(dirname "$0")/omp-AGENTS.md"
[ -f "$omp_config" ] && [ -f "$omp_context" ] && [ -d "$omp_agents" ] || die "missing $omp_config, $omp_context or $omp_agents (run forca sync)"
kr -f "$omp_config" exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.omp/agent" && cat > "$HOME/.omp/agent/config.yml"'
# User-level context for every omp session (NAS artifact area, ...).
kr -f "$omp_context" exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.omp/agent" && cat > "$HOME/.omp/agent/AGENTS.md"'
# bsdtar (macOS) would add AppleDouble files and provenance xattr headers that
# GNU tar in the sandbox warns about.
tar_flags=""
tar --version 2>/dev/null | grep -q bsdtar && tar_flags="--no-xattrs --no-mac-metadata"
COPYFILE_DISABLE=1 tar $tar_flags -C "$omp_agents" -cf "$work/agents.tar" .
kr -f "$work/agents.tar" exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.omp/agent/agents" && tar -C "$HOME/.omp/agent/agents" -xf -'

# The forca CLI inside the sandbox (`forca emulator start|list|stop|posture`).
# It ships with the recipe, so a workspace always has the CLI version that
# matches the recipe's FORCA_SANDBOX_* environment, and `forca sync` is the
# only update path. It needs bash, kubectl and adb, all in the dev-flutter
# image.
if has emulator; then
  forca_cli="$(dirname "$0")/forca"
  [ -f "$forca_cli" ] || die "missing $forca_cli (run forca sync)"
  kr -f "$forca_cli" exec -i "$sandbox" -- sh -c 'mkdir -p "$HOME/.local/bin" && cat > "$HOME/.local/bin/forca.tmp" && chmod 0755 "$HOME/.local/bin/forca.tmp" && mv "$HOME/.local/bin/forca.tmp" "$HOME/.local/bin/forca"'
fi

# "Kata Sandbox + Backend" (kata-create-backend.sh): the repo's own setup flow
# (eevee development-workflow guide): DOPPLER_TOKEN in the root .env, then
# `bun run setup` (dependencies, Doppler dev_local env, local DB migrate). The
# token is a read-only service token in Secret forca-backend-<repo>; it only
# lands in this workspace's .env. `adb reverse` makes the emulator's
# localhost:3001 the sandbox API, so `run:local` works unchanged; Marionette
# lets omp drive the running app (flutter-dev skill). Detached like the main
# setup: `bun run setup` takes minutes.
if [ "${ORCA_KATA_BACKEND:-0}" = 1 ]; then
  has backend || die "$repo_name has no backend feature in forca repos.tsv"
  backend_secret="forca-backend-$repo_name"
  kr get secret "$backend_secret" -o jsonpath='{.data.DOPPLER_TOKEN}'
  doppler_token="$(printf '%s' "$KR_OUT" | base64 -d)"
  [ -n "$doppler_token" ] || die "secret $backend_secret has no DOPPLER_TOKEN"
  log "backend: bun run setup (several minutes), adb reverse :3001, Marionette"
  read -r -d '' backend_script <<'REMOTE' || true
set -euo pipefail
{ read -r token; read -r project_root; IFS= read -r emu_addr || true; } < "$1"
cd "$project_root"
if [ ! -f .env ]; then (umask 077; printf "DOPPLER_TOKEN=%s\n" "$token" > .env); fi
bun run setup >&2
if [ -n "$emu_addr" ]; then adb -s "$emu_addr" reverse tcp:3001 tcp:3001 >&2; fi
bun run flutter:marionette setup --providers oh-my-pi >&2 ||
  echo "warning: Marionette setup failed; see the flutter-dev skill" >&2
REMOTE
  make_payload "$work/backend.payload" "$backend_script" "$doppler_token" "$project_root" "$emu_addr"
  unset doppler_token
  remote_run backend "$work/backend.payload"
  rm -f "$work/backend.payload"
fi

# Orca dials with BatchMode and StrictHostKeyChecking: trust this sandbox's key
# under its own alias (removed again by kata-destroy.sh).
known_hosts="$HOME/.ssh/known_hosts"
touch "$known_hosts"
ssh-keygen -R "$sandbox" -f "$known_hosts" >/dev/null 2>&1 || true
printf '%s %s\n' "$sandbox" "$host_key" >> "$known_hosts"

proxy_command="'$kubectl_bin' --kubeconfig '$kubeconfig' -n $NAMESPACE exec -i $sandbox -- /usr/sbin/sshd -i -e -o LogLevel=QUIET -f $SSHD_DIR/sshd_config"

# The tunnel goes through the API server like everything else, so a network
# blip fails an attempt without meaning the sandbox is broken: retry a few times.
log "smoke-testing SSH"
smoke=0
for attempt in 1 2 3 4 5 6; do
  if ssh -o BatchMode=yes -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -i "$ssh_key" \
    -o "ProxyCommand=$proxy_command" "dev@$sandbox" \
    "test -d '$project_root/.git' && command -v omp >/dev/null" </dev/null >&2; then
    smoke=1; break
  fi
  log "SSH smoke test attempt $attempt failed"
  nap 5
done
[ "$smoke" = 1 ] || die "SSH smoke test failed"

# Desktop side of the emulator: `forca emulator connect` (the forca CLI synced
# next to this script) keeps a self-restarting port-forward to the emulator's
# adbd and `adb connect`s it, so the device shows up in Orca's Mobile Emulator
# panel. The emulator itself runs on the cluster, never here. The same command
# re-establishes the connection later, and for emulators the agent starts
# inside the sandbox.
adb_port=""
if [ -n "$emu_pod" ]; then
  forca_cli="$(dirname "$0")/forca"
  if [ ! -x "$forca_cli" ]; then
    log "missing $forca_cli (run forca sync); skipping the local emulator connection"
  elif connect_json="$(ORCA_KATA_KUBECTL="$kubectl_bin" "$forca_cli" emulator connect --sandbox "$claim" --kubeconfig "$kubeconfig" --json </dev/null)"; then
    adb_port="$(printf '%s' "$connect_json" | sed -n 's/.*"adb":"127\.0\.0\.1:\([0-9][0-9]*\)".*/\1/p')"
  else
    log "could not connect this desktop's adb; run: forca emulator connect --sandbox $claim"
  fi
fi

created=1
log "ready: $sandbox ($project_root)${emu_pod:+, emulator $emu_pod (desktop adb 127.0.0.1:$adb_port)}"
checkout_mode=""
[ "$result_schema" = 2 ] && checkout_mode=',"checkoutMode":"provisioned-root"'
printf '{"schemaVersion":%s%s,"connection":{"type":"ssh","projectRoot":"%s","target":{"label":"kata-%s","host":"%s","port":22,"username":"dev","identityFile":"%s","identitiesOnly":true,"proxyCommand":"%s"}},"userData":{"provider":"agent-sandbox","resourceId":"%s","sandbox":"%s","namespace":"%s","kubeconfig":"%s","emuClaim":"%s","adbPort":"%s"}}\n' \
  "$result_schema" "$checkout_mode" "$project_root" "$repo_name" "$sandbox" "$ssh_key" "$proxy_command" "$claim" "$sandbox" "$NAMESPACE" "$kubeconfig" "$emu_claim" "$adb_port"
