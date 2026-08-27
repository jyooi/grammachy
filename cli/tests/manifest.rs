//! The plugin manifest, spec sections 10 and 11.

use serde_json::Value;

fn manifest() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../manifest.json");
    let text = std::fs::read_to_string(path).expect("manifest.json is readable");
    serde_json::from_str(&text).expect("manifest.json is JSON")
}

fn cli_lock() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../cli.lock");
    let text = std::fs::read_to_string(path).expect("cli.lock is readable");
    serde_json::from_str(&text).expect("cli.lock is JSON")
}

#[test]
fn the_manifest_declares_the_plugin() {
    let manifest = manifest();

    assert_eq!(manifest["id"], "io.github.jyooi.grammachy");
    assert_eq!(
        manifest["kinds"],
        serde_json::json!(["bar-widget", "overlay"])
    );
    assert_eq!(manifest["keepLoaded"], true);
}

#[test]
fn the_manifest_version_equals_the_crate_version() {
    assert_eq!(manifest()["version"], env!("CARGO_PKG_VERSION"));
}

// Section 10: a release is two commits, the tag CI builds and the cli.lock
// bump. This keeps the bump honest against the crate it pins.
#[test]
fn the_cli_lock_version_equals_the_crate_version() {
    assert_eq!(cli_lock()["version"], env!("CARGO_PKG_VERSION"));
}
