//! Build script for Project Zang
//!
//! This build script:
//! - Sets the `in_docker` cfg flag when running inside Docker
//! - Installs git pre-commit hooks on first build

use std::process::Command;

fn main() {
    // Always declare the cfg so rustc doesn't warn
    println!("cargo::rustc-check-cfg=cfg(in_docker)");
    println!("cargo::rustc-check-cfg=cfg(ci)");

    // Check if we're running inside Docker
    // The standard way is to check for the /.dockerenv file
    let in_docker = std::path::Path::new("/.dockerenv").exists();

    if in_docker {
        println!("cargo:rustc-cfg=in_docker");
    }

    // For development purposes, also allow running outside Docker
    // by checking an environment variable
    if std::env::var("ZANG_ALLOW_LOCAL_TESTS").is_ok() {
        println!("cargo:rustc-cfg=in_docker");
    }

    // Mark when running in CI (tests allowed outside Docker in CI)
    if std::env::var("CI").is_ok() {
        println!("cargo:rustc-cfg=ci");
    }

    // Install git hooks if not already configured (skip in Docker/CI)
    if !in_docker && std::env::var("CI").is_err() {
        install_git_hooks();
    }
}

/// Install git hooks by configuring core.hooksPath
///
/// This function is careful to:
/// - Only run in the project root (not when used as a dependency)
/// - Never overwrite existing hook configurations (husky, lefthook, etc.)
/// - Scope all git commands to the local repository
fn install_git_hooks() {
    // Get the manifest directory (where Cargo.toml lives)
    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => return,
    };

    // Only install hooks if we're building the root project, not as a dependency
    // Check if .git directory exists in this directory
    if !manifest_dir.join(".git").exists() {
        return;
    }

    // Check if .githooks directory exists
    if !manifest_dir.join(".githooks").exists() {
        return;
    }

    // Check if any hooks path is already configured (don't overwrite user's setup)
    let output = Command::new("git")
        .args(["-C", manifest_dir.to_str().unwrap_or("."), "config", "--local", "--get", "core.hooksPath"])
        .output();

    match output {
        Ok(result) => {
            let current_path = String::from_utf8_lossy(&result.stdout);
            let trimmed = current_path.trim();
            // If any hooks path is configured, don't overwrite it
            if !trimmed.is_empty() {
                return;
            }
        }
        Err(_) => {
            // git not available, skip
            return;
        }
    }

    // Configure git to use .githooks directory (scoped to local repo only)
    let result = Command::new("git")
        .args(["-C", manifest_dir.to_str().unwrap_or("."), "config", "--local", "core.hooksPath", ".githooks"])
        .status();

    if let Ok(status) = result {
        if status.success() {
            println!("cargo:warning=Git hooks installed (core.hooksPath=.githooks)");
        }
    }
}
