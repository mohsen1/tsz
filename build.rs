//! Build script for tsz
//!
//! This script ensures lib assets are generated before compilation.
//! It runs `node scripts/generate-lib-assets.mjs` which:
//! - Installs TypeScript npm package if needed
//! - Copies lib.*.d.ts files to src/lib-assets/
//! - Generates lib_manifest.json

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    // Declare custom cfg flags for check-cfg
    println!("cargo::rustc-check-cfg=cfg(in_docker)");
    println!("cargo::rustc-check-cfg=cfg(ci)");

    // Detect Docker environment (allow tests to run inside Docker)
    // Docker containers typically have /.dockerenv file or cgroup with docker
    let in_docker = Path::new("/.dockerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup")
            .map(|s| s.contains("docker"))
            .unwrap_or(false);
    if in_docker {
        println!("cargo:rustc-cfg=in_docker");
    }

    // Detect CI environment
    if env::var("CI").is_ok() {
        println!("cargo:rustc-cfg=ci");
    }

    // Allow tests to run locally when TSZ_ALLOW_LOCAL_TESTS is set
    // This is for development convenience when Docker is not available
    if env::var("TSZ_ALLOW_LOCAL_TESTS").is_ok() {
        println!("cargo:rustc-cfg=in_docker");
    }

    // Auto-generate when lib-assets are missing or LIB_ASSETS_FORCE is set
    let force = env::var("LIB_ASSETS_FORCE").is_ok();

    // Check if lib-assets directory exists
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_assets_dir = Path::new(&manifest_dir).join("src/lib-assets");
    let version_file = lib_assets_dir.join("lib_version.json");

    // Skip generation if lib-assets already exist and not forced
    if version_file.exists() && !force {
        println!("cargo:rerun-if-changed=src/lib-assets/lib_version.json");
        println!("cargo:rerun-if-changed=conformance/typescript-versions.json");
        return;
    }

    // Auto-generate lib-assets for any profile if node is available
    // Previously release builds required pre-generation, but this caused CI failures
    // when lib-assets weren't committed (they are gitignored).

    println!("cargo:warning=Generating lib-assets from TypeScript npm package...");

    // Check if node is available
    let node_check = Command::new("node").arg("--version").output();
    if node_check.is_err() {
        panic!(
            "Node.js is required to generate lib-assets.\n\
             Please install Node.js and run: node scripts/generate-lib-assets.mjs"
        );
    }

    // Run the generate script
    let script_path = Path::new(&manifest_dir).join("scripts/generate-lib-assets.mjs");
    let status = Command::new("node")
        .arg(&script_path)
        .current_dir(&manifest_dir)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=Lib-assets generated successfully.");
        }
        Ok(s) => {
            panic!(
                "Failed to generate lib-assets. Exit code: {:?}\n\
                 Try running manually: node scripts/generate-lib-assets.mjs",
                s.code()
            );
        }
        Err(e) => {
            panic!(
                "Failed to run generate-lib-assets.mjs: {}\n\
                 Try running manually: node scripts/generate-lib-assets.mjs",
                e
            );
        }
    }

    // Tell Cargo to rerun if these files change
    println!("cargo:rerun-if-changed=scripts/generate-lib-assets.mjs");
    println!("cargo:rerun-if-changed=conformance/typescript-versions.json");
}
