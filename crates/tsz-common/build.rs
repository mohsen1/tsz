//! Auto-install the repo's git hooks when cargo runs in a dev checkout.
//!
//! Why: without the pre-commit hook, unformatted code sneaks into commits and
//! CI's `cargo fmt --check` fails. Running `cargo` anywhere in the workspace
//! wires `core.hooksPath` to `scripts/githooks` so the hook runs on every
//! commit. The hook itself formats + re-stages files.
//!
//! Safe in published builds and non-checkouts: bails if the repo's hooks dir
//! or `.git` is missing. Never runs git commands from a `cargo publish` tree.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // CI never needs the dev-side git hook install. Running the script
    // there forces cargo to consider the build-script output dynamic,
    // which invalidates the workspace cache on every cache-warm rebuild.
    println!("cargo:rerun-if-env-changed=CI");
    if std::env::var_os("CI").is_some() {
        return;
    }

    let manifest_dir = match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(p) => p,
        None => return,
    };
    let workspace_root = match Path::new(&manifest_dir).parent().and_then(Path::parent) {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    let hooks_dir = workspace_root.join("scripts/githooks");
    let git_dir = workspace_root.join(".git");

    // Guard: only operate in a dev checkout that ships the hooks.
    // This skips crates.io installs, cargo publish dry-runs, and any tree
    // that doesn't look like a tsz git checkout.
    if !hooks_dir.is_dir() || !git_dir.exists() {
        return;
    }

    println!(
        "cargo:rerun-if-changed={}",
        hooks_dir.join("pre-commit").display()
    );

    let current = Command::new("git")
        .args(["config", "--get", "core.hooksPath"])
        .current_dir(&workspace_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if current == "scripts/githooks" {
        return;
    }

    // Best-effort install; never fail the build if git is missing or the
    // user has a restricted environment.
    let _ = Command::new("git")
        .args(["config", "core.hooksPath", "scripts/githooks"])
        .current_dir(&workspace_root)
        .output();
}
