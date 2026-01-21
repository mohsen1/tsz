//! Build script for Project Zang
//!
//! This build script sets the `in_docker` cfg flag when running inside Docker.
//! This allows the test suite to enforce that tests are run in the Docker environment.

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
}
