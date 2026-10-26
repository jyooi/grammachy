#!/usr/bin/env bash
set -euo pipefail

repo=$(git -C "$(dirname "$(readlink -f "$0")")" rev-parse --show-toplevel)
bin="$repo/cli/target/release/grammachy"
tag=$(printf '%s' "$repo" | sha1sum | cut -c1-8)
home=${VG_HOME:-${TMPDIR:-/tmp}/verify-grammachy-$tag}
scratch="$home/scratch"
evidence=${VG_EVIDENCE:-$home/evidence}
step_timeout=${VG_TIMEOUT:-60}

usage() {
  cat <<EOF
usage:
  vg.sh build
  vg.sh launch
  vg.sh doctor
  vg.sh path shell-json|engines|bindings|menu|evidence|scratch
  vg.sh run <feature-id> [--stdin <file> | --text <string>] -- <grammachy args>
  vg.sh stub start|stop
  vg.sh cleanup
EOF
}

die() { echo "vg: $*" >&2; exit 2; }

path_of() {
  case $1 in
    shell-json) echo "$scratch/omarchy/shell.json" ;;
    engines) echo "$scratch/engines" ;;
    bindings) echo "$scratch/hypr/bindings.lua" ;;
    menu) echo "$scratch/omarchy/extensions/omarchy-menu.jsonc" ;;
    evidence) echo "$evidence" ;;
    scratch) echo "$scratch" ;;
    *) die "unknown path name: $1" ;;
  esac
}

isolated_env() {
  local env=(
    "GRAMMACHY_SHELL_JSON=$(path_of shell-json)"
    "GRAMMACHY_ENGINES_DIR=$(path_of engines)"
    "GRAMMACHY_BINDINGS_LUA=$(path_of bindings)"
    "GRAMMACHY_MENU_JSONC=$(path_of menu)"
    "GRAMMACHY_HYPRCTL_RELOAD=never"
    "GRAMMACHY_ENGINE_STOP=never"
  )
  if [[ -f "$scratch/stub.env" ]]; then
    mapfile -t -O "${#env[@]}" env < "$scratch/stub.env"
  fi
  printf '%s\n' "${env[@]}"
}

cmd_build() {
  ( cd "$repo/cli" && timeout 900 cargo build --release )
  "$bin" --version
}

cmd_launch() {
  [[ -x "$bin" ]] || die "no binary at $bin. Run: vg.sh build"
  [[ -f "$scratch/stub.pid" ]] && die "a stub from an earlier run is still recorded. Run: vg.sh cleanup"
  rm -rf "$scratch"
  mkdir -p "$scratch/omarchy/extensions" "$scratch/hypr" "$scratch/engines" "$evidence"
  printf '%s\n' '{"bar":{"layout":{"right":[{"id":"io.github.jyooi.grammachy"}]}}}' > "$(path_of shell-json)"
  echo "ready: scratch=$scratch evidence=$evidence binary=$bin"
}

cmd_doctor() {
  local status=0
  if [[ -x "$bin" ]]; then
    local want got
    want=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/cli/Cargo.toml" | head -1)
    got=$("$bin" --version | awk '{print $2}')
    echo "binary: $bin ($got, Cargo.toml $want)"
    [[ "$got" == "$want" ]] || { echo "FAIL: binary version is not the checkout version. Run: vg.sh build"; status=1; }
    if [[ -n $(find "$repo/cli/src" "$repo/cli/Cargo.toml" -newer "$bin" -print -quit) ]]; then
      echo "FAIL: cli sources are newer than the binary. Run: vg.sh build"; status=1
    fi
  else
    echo "FAIL: no binary at $bin. Run: vg.sh build"; status=1
  fi
  echo "checkout: $repo at $(git -C "$repo" rev-parse --short HEAD)"
  if [[ -f "$(path_of shell-json)" ]]; then
    echo "scratch: $scratch"
  else
    echo "FAIL: no scratch tree. Run: vg.sh launch"; status=1
  fi
  echo "evidence: $evidence"
  if [[ -f "$scratch/stub.env" ]]; then
    echo "stub: $(grep BASE_URL "$scratch/stub.env" | cut -d= -f2-) pid $(cat "$scratch/stub.pid")"
  else
    echo "stub: not running"
  fi
  echo "shared unit grammachy-languagetool: $(timeout 3 systemctl --user is-active grammachy-languagetool 2>/dev/null || true)"
  if [[ -e "$HOME/.config/omarchy/plugins/io.github.jyooi.grammachy" ]]; then
    echo "live plugin: present in the operator shell. Do not change it."
  fi
  return $status
}

uses_shared_unit() {
  local engine="" prev=""
  for arg in "$@"; do
    [[ "$prev" == "--engine" ]] && engine=$arg
    [[ "$arg" == --engine=* ]] && engine=${arg#--engine=}
    prev=$arg
  done
  if [[ -z "$engine" ]]; then
    engine=$(jq -r '[.bar.layout[]?[]?, .plugins[]?] | map(select(.id == "io.github.jyooi.grammachy")) | .[0].engine // ""' "$(path_of shell-json)" 2>/dev/null || true)
  fi
  [[ "$engine" == "languagetool" ]]
}

cmd_run() {
  local feature=${1:-}
  [[ -n "$feature" ]] || die "missing feature id"
  shift
  local stdin=/dev/null text_given=0 text=""
  while [[ $# -gt 0 && "$1" != "--" ]]; do
    case $1 in
      --stdin) stdin=$2; shift 2 ;;
      --text) text=$2; text_given=1; shift 2 ;;
      *) die "unknown option $1" ;;
    esac
  done
  [[ "${1:-}" == "--" ]] || die "missing -- before the grammachy arguments"
  shift
  [[ $# -gt 0 ]] || die "missing grammachy arguments"
  [[ -f "$(path_of shell-json)" ]] || die "no scratch tree. Run: vg.sh launch"
  if [[ "$1" == "check" ]] && uses_shared_unit "$@" && [[ "${VG_ALLOW_SHARED_UNIT:-}" != 1 ]]; then
    die "check on languagetool starts or reuses the shared unit grammachy-languagetool. Set VG_ALLOW_SHARED_UNIT=1 only with operator approval."
  fi
  if [[ "$1 ${2:-}" == "engine install" && ! -f "$scratch/stub.env" && "${VG_ALLOW_DOWNLOAD:-}" != 1 ]]; then
    die "engine install without the stub downloads about 250 MB. Run: vg.sh stub start, or set VG_ALLOW_DOWNLOAD=1."
  fi

  local dir="$evidence/$feature"
  mkdir -p "$dir"
  local n
  n=$(printf '%02d' $(( $(find "$dir" -maxdepth 1 -name '*.cmd' | wc -l) + 1 )))
  local base="$dir/$n"
  if [[ $text_given == 1 ]]; then
    printf '%s' "$text" > "$base.stdin"
  else
    cat "$stdin" > "$base.stdin"
  fi
  printf '%q ' grammachy "$@" > "$base.cmd"
  echo >> "$base.cmd"
  local -a env
  mapfile -t env < <(isolated_env)
  local code=0
  env "${env[@]}" timeout "$step_timeout" "$bin" "$@" < "$base.stdin" > "$base.stdout" 2> "$base.stderr" || code=$?
  echo "$code" > "$base.exit"
  {
    echo "feature: $feature"
    echo "commit: $(git -C "$repo" rev-parse HEAD)"
    echo "binary: $("$bin" --version)"
    echo "time: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '%s\n' "${env[@]}"
  } > "$base.meta"
  echo "\$ $(cat "$base.cmd")" >&2
  cat "$base.stdout"
  if [[ -s "$base.stderr" ]]; then
    echo "--- stderr" >&2
    cat "$base.stderr" >&2
  fi
  echo "--- exit $code, evidence $base.*" >&2
  return "$code"
}

stub_answers() {
  curl -fsI --max-time 1 --cacert "$scratch/stub-tls/cert.pem" "$1" >/dev/null 2>&1
}

cmd_stub() {
  case ${1:-} in
    start)
      [[ -d "$scratch" ]] || die "no scratch tree. Run: vg.sh launch"
      [[ -f "$scratch/stub.pid" ]] && die "stub already running"
      local root="$scratch/stub" build="$scratch/stub-build"
      mkdir -p "$root/download" "$build/LanguageTool-6.6"
      echo "verification stub, not a real server jar" > "$build/LanguageTool-6.6/languagetool-server.jar"
      ( cd "$build" && zip -q -r "$root/download/LanguageTool-6.6.zip" LanguageTool-6.6 )
      local tls="$scratch/stub-tls"
      mkdir -p "$tls"
      openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=127.0.0.1 \
        -addext subjectAltName=IP:127.0.0.1 -keyout "$tls/key.pem" -out "$tls/cert.pem" 2>/dev/null
      local zipfile="$root/download/LanguageTool-6.6.zip" port url
      port=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
      url="https://127.0.0.1:$port/download/LanguageTool-6.6.zip"
      python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$root" \
        --tls-cert "$tls/cert.pem" --tls-key "$tls/key.pem" > "$scratch/stub.log" 2>&1 &
      echo $! > "$scratch/stub.pid"
      for _ in $(seq 50); do
        stub_answers "$url" && break
        sleep 0.1
      done
      stub_answers "$url" || die "stub did not answer on port $port. Run: vg.sh cleanup"
      {
        echo "GRAMMACHY_ENGINE_BASE_URL=https://127.0.0.1:$port"
        echo "GRAMMACHY_ENGINE_SHA256=$(sha256sum "$zipfile" | cut -d' ' -f1)"
        echo "GRAMMACHY_ENGINE_SIZE_BYTES=$(stat -c %s "$zipfile")"
        echo "CURL_CA_BUNDLE=$tls/cert.pem"
      } > "$scratch/stub.env"
      echo "stub ready: https://127.0.0.1:$port pid $(cat "$scratch/stub.pid")"
      ;;
    stop)
      if [[ -f "$scratch/stub.pid" ]]; then
        kill "$(cat "$scratch/stub.pid")" 2>/dev/null || true
        rm -f "$scratch/stub.pid" "$scratch/stub.env"
        echo "stub stopped"
      else
        echo "stub: not running"
      fi
      ;;
    *) die "usage: vg.sh stub start|stop" ;;
  esac
}

cmd_cleanup() {
  cmd_stub stop
  rm -rf "$scratch"
  echo "scratch removed. evidence kept at $evidence"
}

case ${1:-} in
  build) cmd_build ;;
  launch) cmd_launch ;;
  doctor) cmd_doctor ;;
  path) path_of "${2:?missing path name}" ;;
  run) shift; cmd_run "$@" ;;
  stub) shift; cmd_stub "$@" ;;
  cleanup) cmd_cleanup ;;
  *) usage; exit 2 ;;
esac
