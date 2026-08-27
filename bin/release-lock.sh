#!/usr/bin/env bash
# Bumps cli.lock to one tag's release asset, spec section 10.
#
# A release is two commits: the tag CI builds, then this bump. Run it after
# CI has finished building the tag, with the tag itself as the one argument:
#
#   bin/release-lock.sh v0.1.0
#
# It downloads grammachy-x86_64-linux for that tag, hashes it, and rewrites
# cli.lock with the version and the sha256, so the bump commit is mechanical.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: bin/release-lock.sh <tag>" >&2
  exit 1
fi

tag="$1"
version="${tag#v}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock_path="${GRAMMACHY_BOOTSTRAP_LOCK:-$repo_root/cli.lock}"
repo="${GRAMMACHY_BOOTSTRAP_REPO:-jyooi/grammachy}"
base_url="${GRAMMACHY_BOOTSTRAP_BASE_URL:-https://github.com/$repo/releases/download}"
curl_bin="${GRAMMACHY_BOOTSTRAP_CURL:-curl}"
asset="grammachy-x86_64-linux"
url="$base_url/$tag/$asset"

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

echo "Downloading $asset $tag from $url"
http_code="$("$curl_bin" -sS -L -o "$tmp_file" -w '%{http_code}' "$url")"
if [[ "$http_code" != "200" ]]; then
  echo "Download failed: $url answered HTTP $http_code" >&2
  exit 1
fi

sha256="$(sha256sum "$tmp_file" | cut -d ' ' -f 1)"

tmp_lock="$(mktemp)"
trap 'rm -f "$tmp_file" "$tmp_lock"' EXIT
jq --arg version "$version" --arg sha256 "$sha256" \
  '.version = $version | .sha256 = $sha256' "$lock_path" > "$tmp_lock"
mv "$tmp_lock" "$lock_path"

echo "cli.lock now pins $version at $sha256"
