//! `bin/bootstrap.sh` against a stub curl, spec section 10.
//!
//! No test reaches the network: `GRAMMACHY_BOOTSTRAP_CURL` points at a stub
//! standing in for curl. The stub always answers 200 with a fixture file and
//! records the arguments it was called with, so the size check, the sha256
//! check, and the limits the script hands curl are what every scenario here
//! turns on.

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

/// A cli.lock naming one version, one pinned sha256, and one pinned size.
fn write_lock(dir: &Path, version: &str, sha256: &str, size_bytes: u64) -> PathBuf {
    let path = dir.join("cli.lock");
    fs::write(
        &path,
        format!(
            "{{\n  \"version\": \"{version}\",\n  \"sha256\": \"{sha256}\",\n  \"sizeBytes\": {size_bytes}\n}}\n"
        ),
    )
    .expect("cli.lock is written");
    path
}

/// A cli.lock from before the size pin existed: version and sha256 only.
fn write_lock_without_size(dir: &Path, version: &str, sha256: &str) -> PathBuf {
    let path = dir.join("cli.lock");
    fs::write(
        &path,
        format!("{{\n  \"version\": \"{version}\",\n  \"sha256\": \"{sha256}\"\n}}\n"),
    )
    .expect("cli.lock is written");
    path
}

/// A stub `curl` that ignores the URL and always serves `asset_content` at
/// the `-o` path with HTTP 200: something arrives every time, so the size
/// and hash checks are what decide what happens next. It writes its
/// arguments, one per line, to `curl-args` beside itself.
fn write_curl_stub(dir: &Path, asset_content: &[u8]) -> PathBuf {
    let asset_path = dir.join("fixture-asset");
    fs::write(&asset_path, asset_content).expect("the fixture asset is written");
    let args_path = dir.join("curl-args");

    let script_path = dir.join("curl-stub.sh");
    let script = format!(
        "#!/usr/bin/env bash\n\
         set -euo pipefail\n\
         printf '%s\\n' \"$@\" > {args:?}\n\
         out=\"\"\n\
         prev=\"\"\n\
         for arg in \"$@\"; do\n\
         \x20 if [[ \"$prev\" == \"-o\" ]]; then out=\"$arg\"; fi\n\
         \x20 prev=\"$arg\"\n\
         done\n\
         cp {asset:?} \"$out\"\n\
         printf '200'\n",
        args = args_path.display(),
        asset = asset_path.display()
    );
    fs::write(&script_path, script).expect("the curl stub is written");
    make_executable(&script_path);
    script_path
}

fn curl_args(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("curl-args"))
        .expect("the curl stub recorded its arguments")
        .lines()
        .map(str::to_string)
        .collect()
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
    let content = b"not the pinned binary";
    let lock = write_lock(&dir, "9.9.9", &"0".repeat(64), content.len() as u64);
    let curl_stub = write_curl_stub(&dir, content);

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sha256 mismatch"), "{stderr}");
    assert!(!dir.join("grammachy").exists());
}

#[test]
fn an_asset_of_the_wrong_size_fails_before_hashing_and_leaves_no_binary() {
    let dir = scratch_dir("oversize");
    let content = b"a fixture binary with more bytes than the pin";
    let lock = write_lock(&dir, "9.9.9", &sha256_hex(content), 16);
    let curl_stub = write_curl_stub(&dir, content);

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("size mismatch"), "{stderr}");
    assert!(!stderr.contains("sha256 mismatch"), "{stderr}");
    assert!(!dir.join("grammachy").exists());
}

#[test]
fn a_lock_without_a_size_refuses_before_downloading() {
    let dir = scratch_dir("no-size");
    let content = b"a fixture binary";
    let lock = write_lock_without_size(&dir, "9.9.9", &sha256_hex(content));
    let curl_stub = write_curl_stub(&dir, content);

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pins no sizeBytes"), "{stderr}");
    assert!(!dir.join("curl-args").exists(), "curl never ran");
    assert!(!dir.join("grammachy").exists());
}

#[test]
fn the_download_is_bounded_in_bytes_time_and_scheme() {
    let dir = scratch_dir("bounded");
    let content = b"a fixture binary";
    let lock = write_lock(&dir, "9.9.9", &sha256_hex(content), content.len() as u64);
    let curl_stub = write_curl_stub(&dir, content);

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = curl_args(&dir);
    let flag_value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|index| args[index + 1].as_str())
    };
    assert_eq!(flag_value("--max-filesize"), Some("16"));
    assert!(flag_value("--max-time").is_some(), "{args:?}");
    assert!(flag_value("--connect-timeout").is_some(), "{args:?}");
    assert_eq!(flag_value("--proto"), Some("=https"));
    assert_eq!(flag_value("--proto-redir"), Some("=https"));
}

#[test]
fn a_matching_asset_lands_at_the_output_path_and_is_executable() {
    let dir = scratch_dir("matching");
    let content = b"a fixture binary";
    let lock = write_lock(&dir, "9.9.9", &sha256_hex(content), content.len() as u64);
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
    let lock = write_lock(&dir, "0.1.0", "", 0);
    let curl_stub = write_curl_stub(&dir, b"unused");

    let output = bootstrap(&dir, &lock, &curl_stub);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No release is pinned"), "{stderr}");
    assert!(!dir.join("grammachy").exists());
}
