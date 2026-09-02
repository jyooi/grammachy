//! `bin/release-lock.sh` against a stub curl and a stub gh, spec section 10.
//!
//! No test reaches the network or GitHub: `GRAMMACHY_BOOTSTRAP_CURL` serves
//! a fixture asset and `GRAMMACHY_BOOTSTRAP_GH` stands in for
//! `gh attestation verify`. The provenance check is the gate between the
//! download and the pin, so the stub's exit status is what each scenario
//! turns on.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("the repo root exists")
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("release-lock")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

const STALE_LOCK: &str = "{\n  \"version\": \"0.0.1\",\n  \"sha256\": \"\"\n}\n";

fn write_stale_lock(dir: &Path) -> PathBuf {
    let path = dir.join("cli.lock");
    fs::write(&path, STALE_LOCK).expect("cli.lock is written");
    path
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .expect("the stub has metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("the stub is made executable");
}

/// A stub `curl` that serves `asset_content` at the `-o` path with HTTP 200.
fn write_curl_stub(dir: &Path, asset_content: &[u8]) -> PathBuf {
    let asset_path = dir.join("fixture-asset");
    fs::write(&asset_path, asset_content).expect("the fixture asset is written");

    let script_path = dir.join("curl-stub.sh");
    let script = format!(
        "#!/usr/bin/env bash\n\
         set -euo pipefail\n\
         out=\"\"\n\
         prev=\"\"\n\
         for arg in \"$@\"; do\n\
         \x20 if [[ \"$prev\" == \"-o\" ]]; then out=\"$arg\"; fi\n\
         \x20 prev=\"$arg\"\n\
         done\n\
         cp {asset:?} \"$out\"\n\
         printf '200'\n",
        asset = asset_path.display()
    );
    fs::write(&script_path, script).expect("the curl stub is written");
    make_executable(&script_path);
    script_path
}

/// A stub `gh` that records its arguments to `gh-args` beside itself and
/// exits with `status`, the way `gh attestation verify` answers a verified
/// or an unverified asset.
fn write_gh_stub(dir: &Path, status: i32) -> PathBuf {
    let args_path = dir.join("gh-args");
    let script_path = dir.join("gh-stub.sh");
    let script = format!(
        "#!/usr/bin/env bash\n\
         set -euo pipefail\n\
         printf '%s\\n' \"$@\" > {args:?}\n\
         exit {status}\n",
        args = args_path.display()
    );
    fs::write(&script_path, script).expect("the gh stub is written");
    make_executable(&script_path);
    script_path
}

fn gh_args(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("gh-args"))
        .expect("the gh stub recorded its arguments")
        .lines()
        .map(str::to_string)
        .collect()
}

/// The same sha256sum release-lock.sh itself shells out to.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sha256sum runs");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(bytes)
        .expect("bytes are written to sha256sum");
    let output = child.wait_with_output().expect("sha256sum finishes");
    String::from_utf8(output.stdout)
        .expect("sha256sum prints UTF-8")
        .split_whitespace()
        .next()
        .expect("sha256sum prints a hash")
        .to_string()
}

fn release_lock(dir: &Path, lock: &Path, curl_stub: &Path, gh_stub: &Path, tag: &str) -> Output {
    Command::new("bash")
        .arg(repo_root().join("bin/release-lock.sh"))
        .arg(tag)
        .env("GRAMMACHY_BOOTSTRAP_LOCK", lock)
        .env("GRAMMACHY_BOOTSTRAP_CURL", curl_stub)
        .env("GRAMMACHY_BOOTSTRAP_GH", gh_stub)
        .current_dir(dir)
        .output()
        .expect("release-lock.sh runs")
}

#[test]
fn a_verified_asset_pins_version_sha256_and_size() {
    let dir = scratch_dir("verified");
    let content = b"a fixture binary";
    let lock = write_stale_lock(&dir);
    let curl_stub = write_curl_stub(&dir, content);
    let gh_stub = write_gh_stub(&dir, 0);

    let output = release_lock(&dir, &lock, &curl_stub, &gh_stub, "v9.9.9");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pinned: Value =
        serde_json::from_str(&fs::read_to_string(&lock).expect("cli.lock is readable"))
            .expect("cli.lock is JSON");
    assert_eq!(pinned["version"], "9.9.9");
    assert_eq!(pinned["sha256"], sha256_hex(content));
    assert_eq!(pinned["sizeBytes"], content.len() as u64);
}

#[test]
fn the_provenance_check_names_the_tag_and_the_release_workflow() {
    let dir = scratch_dir("provenance-args");
    let lock = write_stale_lock(&dir);
    let curl_stub = write_curl_stub(&dir, b"a fixture binary");
    let gh_stub = write_gh_stub(&dir, 0);

    let output = release_lock(&dir, &lock, &curl_stub, &gh_stub, "v9.9.9");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = gh_args(&dir);
    let flag_value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|index| args[index + 1].as_str())
    };
    assert_eq!(&args[..2], ["attestation", "verify"]);
    assert_eq!(flag_value("--repo"), Some("jyooi/grammachy"));
    assert_eq!(flag_value("--source-ref"), Some("refs/tags/v9.9.9"));
    assert_eq!(
        flag_value("--signer-workflow"),
        Some("jyooi/grammachy/.github/workflows/release.yml")
    );
    assert!(args.iter().any(|arg| arg == "--deny-self-hosted-runners"));
}

#[test]
fn an_unverified_asset_leaves_the_lock_untouched() {
    let dir = scratch_dir("unverified");
    let lock = write_stale_lock(&dir);
    let curl_stub = write_curl_stub(&dir, b"a fixture binary");
    let gh_stub = write_gh_stub(&dir, 1);

    let output = release_lock(&dir, &lock, &curl_stub, &gh_stub, "v9.9.9");

    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(&lock).expect("cli.lock is readable"),
        STALE_LOCK
    );
}
