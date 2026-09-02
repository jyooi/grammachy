#!/usr/bin/env bash
# Fetches the pinned companion binary, spec section 10.
#
# The setup card runs this through a Quickshell Process and streams stdout
# and stderr, so every line here is a status the reader can see. The script
# reads version, sha256, and sizeBytes from cli.lock. It downloads the
# release asset over https only, with a wall-clock limit and the pinned size
# as the byte limit, then checks the size and the hash. It moves the binary
# into bin/grammachy only once both match. A mismatch, a missing pin, or a
# failed download all exit non-zero and leave bin/grammachy as it was.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock_path="${GRAMMACHY_BOOTSTRAP_LOCK:-$repo_root/cli.lock}"
out_path="${GRAMMACHY_BOOTSTRAP_OUT:-$repo_root/bin/grammachy}"
repo="${GRAMMACHY_BOOTSTRAP_REPO:-jyooi/grammachy}"
base_url="${GRAMMACHY_BOOTSTRAP_BASE_URL:-https://github.com/$repo/releases/download}"
curl_bin="${GRAMMACHY_BOOTSTRAP_CURL:-curl}"
asset="grammachy-x86_64-linux"
connect_timeout_seconds=15
max_time_seconds=300

if [[ ! -f "$lock_path" ]]; then
  echo "cli.lock is missing at $lock_path" >&2
  exit 1
fi

lock_text="$(cat "$lock_path")"
version="$(printf '%s' "$lock_text" | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
pinned_sha256="$(printf '%s' "$lock_text" | sed -n 's/.*"sha256"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
pinned_size="$(printf '%s' "$lock_text" | sed -n 's/.*"sizeBytes"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -n1)"

if [[ -z "$version" ]]; then
  echo "cli.lock names no version" >&2
  exit 1
fi

if [[ -z "$pinned_sha256" ]]; then
  echo "No release is pinned in cli.lock yet." >&2
  echo "Build from source instead: cargo build --release, then copy the binary into bin/grammachy." >&2
  exit 1
fi

if [[ -z "$pinned_size" || "$pinned_size" == "0" ]]; then
  echo "cli.lock pins no sizeBytes for $version, so the download has no byte limit." >&2
  echo "Run bin/release-lock.sh v$version to pin it." >&2
  exit 1
fi

tag="v$version"
url="$base_url/$tag/$asset"
out_dir="$(dirname "$out_path")"
mkdir -p "$out_dir"

tmp_file="$(mktemp "$out_dir/.grammachy.XXXXXX")"
trap 'rm -f "$tmp_file"' EXIT

echo "Downloading $asset $tag from $url"
echo "Limits: $pinned_size bytes, $max_time_seconds seconds, https only"
http_code="$("$curl_bin" -sS -L \
  --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout "$connect_timeout_seconds" --max-time "$max_time_seconds" \
  --max-filesize "$pinned_size" \
  -o "$tmp_file" -w '%{http_code}' "$url" || echo "000")"

if [[ "$http_code" != "200" ]]; then
  echo "Download failed: $url answered HTTP $http_code" >&2
  exit 1
fi

actual_size="$(stat -c %s "$tmp_file")"
if [[ "$actual_size" != "$pinned_size" ]]; then
  echo "size mismatch for $asset $tag" >&2
  echo "expected $pinned_size bytes" >&2
  echo "got      $actual_size bytes" >&2
  exit 1
fi

actual_sha256="$(sha256sum "$tmp_file" | cut -d ' ' -f 1)"
if [[ "$actual_sha256" != "$pinned_sha256" ]]; then
  echo "sha256 mismatch for $asset $tag" >&2
  echo "expected $pinned_sha256" >&2
  echo "got      $actual_sha256" >&2
  exit 1
fi

chmod +x "$tmp_file"
mv "$tmp_file" "$out_path"
trap - EXIT
echo "Installed $asset $tag to $out_path"
