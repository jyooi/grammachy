//! `bin/bootstrap.sh` against a stub curl, spec section 10.
//!
//! No test reaches the network: `GRAMMACHY_BOOTSTRAP_CURL` points at a stub
//! standing in for curl, and `GRAMMACHY_BOOTSTRAP_GH` is `never`, so a 404
//! never tries `gh` either. The stub always answers 200 with a fixture file,
//! so the sha256 check is what every scenario here turns on.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("the repo root exists")
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("bootstrap")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

/// A cli.lock naming one version and one pinned sha256.
fn write_lock(dir: &Path, version: &str, sha256: &str) -> PathBuf {
    let path = dir.join("cli.lock");
    fs::write(
        &path,
        format!("{{\n  \"version\": \"{version}\",\n  \"sha256\": \"{sha256}\"\n}}\n"),
    )
    .expect("cli.lock is written");
    path
}

/// A stub `curl` that ignores the URL and always serves `asset_content` at
/// the `-o` path with HTTP 200: something arrives every time, so the hash
/// check is the one thing left to decide what happens next.
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

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .expect("the stub has metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("the stub is made executable");
}

fn bootstrap(dir: &Path, lock: &Path, curl_stub: &Path) -> Output {
    Command::new("bash")
        .arg(repo_root().join("bin/bootstrap.sh"))
        .env("GRAMMACHY_BOOTSTRAP_LOCK", lock)
        .env("GRAMMACHY_BOOTSTRAP_OUT", dir.join("grammachy"))
        .env("GRAMMACHY_BOOTSTRAP_CURL", curl_stub)
        .env("GRAMMACHY_BOOTSTRAP_GH", "never")
        .output()
        .expect("bootstrap.sh runs")
}

/// The same sha256sum bootstrap.sh itself shells out to, so the fixture's
/// pinned hash is honest about what the real tool would compute.
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

#[test]
fn a_tampered_asset_fails_the_hash_check_and_leaves_no_binary() {
    let dir = scratch_dir("tampered");
    let lock = write_lock(&dir, "9.9.9", &"0".repeat(64));
    let curl_stub = write_curl_stub(&dir, b"not the pinned binary");

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sha256 mismatch"), "{stderr}");
    assert!(!dir.join("grammachy").exists());
}

#[test]
fn a_matching_asset_lands_at_the_output_path_and_is_executable() {
    let dir = scratch_dir("matching");
    let content = b"a fixture binary";
    let lock = write_lock(&dir, "9.9.9", &sha256_hex(content));
    let curl_stub = write_curl_stub(&dir, content);

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out_path = dir.join("grammachy");
    assert_eq!(fs::read(&out_path).expect("the binary landed"), content);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&out_path)
            .expect("the binary has metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "the binary is executable");
    }
}

#[test]
fn no_pinned_hash_refuses_with_a_clear_message_and_no_binary() {
    let dir = scratch_dir("unpinned");
    let lock = write_lock(&dir, "0.1.0", "");
    let curl_stub = write_curl_stub(&dir, b"unused");

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No release is pinned"), "{stderr}");
    assert!(!dir.join("grammachy").exists());
}
