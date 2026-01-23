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
fn install_git_hooks() {
    // Check if hooks are already configured
    let output = Command::new("git")
        .args(["config", "--get", "core.hooksPath"])
        .output();

    match output {
        Ok(result) => {
            let current_path = String::from_utf8_lossy(&result.stdout);
            if current_path.trim() == ".githooks" {
                // Already configured
                return;
            }
        }
        Err(_) => {
            // git not available, skip
            return;
        }
    }

    // Check if .githooks directory exists
    if !std::path::Path::new(".githooks").exists() {
        return;
    }

    // Configure git to use .githooks directory
    let result = Command::new("git")
        .args(["config", "core.hooksPath", ".githooks"])
        .status();

    if let Ok(status) = result {
        if status.success() {
            println!("cargo:warning=Git hooks installed (core.hooksPath=.githooks)");
        }
    }
}
