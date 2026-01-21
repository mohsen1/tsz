//! Build script for Project Zang
//!
//! This build script sets the `in_docker` cfg flag when running inside Docker.
//! This allows the test suite to enforce that tests are run in the Docker environment.

fn main() {
    // Check if we're running inside Docker
    // The standard way is to check for the /.dockerenv file
    let in_docker = std::path::Path::new("/.dockerenv").exists();

    if in_docker {
        println!("cargo:rustc-cfg=in_docker");
    }
}
