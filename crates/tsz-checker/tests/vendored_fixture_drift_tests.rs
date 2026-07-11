//! Drift guards for `vendor/TypeScript/` and the version-controlled compiled
//! lib copies under `crates/tsz-website/src/lib`.
//!
//! Tests resolve fixtures and compiled libs from version-controlled copies
//! so results do not depend on environment provisioning (issue #15685 — see
//! `tsz_checker::test_utils::load_typescript_fixture` for the full
//! rationale). These guards keep those copies byte-identical to the
//! TypeScript ref recorded in `scripts/ci/typescript-submodule-ref`.
//!
//! The byte-equality guards run wherever a `TypeScript/` checkout at the
//! pinned ref is present — always true in the `unit` CI job, which restores
//! the pinned checkout — and skip elsewhere (there is nothing to compare
//! against). The source-scan guard runs everywhere.

use std::fs;
use std::path::{Path, PathBuf};

/// Workspace root, resolved from this crate's manifest dir.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The ref every vendored copy must match, from
/// `scripts/ci/typescript-submodule-ref`.
fn pinned_typescript_ref(root: &Path) -> String {
    let ref_file = root.join("scripts/ci/typescript-submodule-ref");
    fs::read_to_string(&ref_file)
        .unwrap_or_else(|e| panic!("read {}: {e}", ref_file.display()))
        .trim()
        .to_string()
}

/// Best-effort commit hash of the `TypeScript/` checkout at `dir`.
///
/// Understands the three layouts this repo produces: the CI cache marker
/// (`.tsz-cache-ref`), a plain clone (`.git/HEAD`, detached), and a
/// submodule-style checkout (`.git` file with a `gitdir:` redirect).
fn checkout_ref(dir: &Path) -> Option<String> {
    if let Ok(cached) = fs::read_to_string(dir.join(".tsz-cache-ref")) {
        return Some(cached.trim().to_string());
    }

    let dot_git = dir.join(".git");
    let git_dir = if dot_git.is_file() {
        let redirect = fs::read_to_string(&dot_git).ok()?;
        let target = redirect.trim().strip_prefix("gitdir:")?.trim();
        let target = PathBuf::from(target);
        if target.is_absolute() {
            target
        } else {
            dir.join(target)
        }
    } else {
        dot_git
    };

    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        // Detached HEAD (what CI's pinned clone uses) is the hash itself.
        None => Some(head.to_string()),
        Some(branch_ref) => fs::read_to_string(git_dir.join(branch_ref.trim()))
            .ok()
            .map(|h| h.trim().to_string()),
    }
}

/// A `TypeScript/` checkout to compare against, but only when it is at the
/// pinned ref — comparing against a stale checkout would report false drift.
fn pinned_checkout(root: &Path) -> Option<PathBuf> {
    let checkout = root.join("TypeScript");
    if !checkout.is_dir() {
        return None;
    }
    (checkout_ref(&checkout).as_deref() == Some(pinned_typescript_ref(root).as_str()))
        .then_some(checkout)
}

/// Every file under `dir` (recursively), as paths relative to `dir`.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).unwrap_or_else(|e| {
            panic!("read_dir {}: {e}", current.display());
        }) {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(dir)
                        .expect("path under walk root")
                        .to_path_buf(),
                );
            }
        }
    }
    out.sort();
    out
}

/// Assert every `(relative-path, our-copy, pinned-checkout-copy)` pair is
/// present and byte-identical, listing each drifted file with its reason.
fn assert_no_drift(pairs: &[(PathBuf, PathBuf, PathBuf)], what: &str) {
    assert!(!pairs.is_empty(), "no {what} files found to compare");

    let drift: Vec<String> = pairs
        .iter()
        .filter_map(|(rel, ours, theirs)| {
            if !theirs.exists() {
                return Some(format!("{} (absent in pinned checkout)", rel.display()));
            }
            let ours_bytes =
                fs::read(ours).unwrap_or_else(|e| panic!("read {}: {e}", ours.display()));
            let theirs_bytes =
                fs::read(theirs).unwrap_or_else(|e| panic!("read {}: {e}", theirs.display()));
            (ours_bytes != theirs_bytes).then(|| format!("{} (content differs)", rel.display()))
        })
        .collect();

    assert!(
        drift.is_empty(),
        "{what} drifted from the pinned checkout; re-copy the listed files per \
         vendor/TypeScript/README.md:\n{}",
        drift.join("\n")
    );
}

/// `vendor/TypeScript/tests/**` must be byte-identical to the pinned
/// checkout's `tests/**`.
#[test]
fn vendored_fixtures_match_pinned_typescript_checkout() {
    let root = repo_root();
    let Some(checkout) = pinned_checkout(&root) else {
        return;
    };

    let vendored_tests = root.join("vendor/TypeScript/tests");
    let pairs: Vec<_> = files_under(&vendored_tests)
        .into_iter()
        .map(|rel| {
            (
                Path::new("tests").join(&rel),
                vendored_tests.join(&rel),
                checkout.join("tests").join(&rel),
            )
        })
        .collect();
    assert_no_drift(&pairs, "vendor/TypeScript/tests");
}

/// Compiled lib files under `crates/tsz-website/src/lib` are the primary root
/// probed by `load_compiled_lib_files`. TypeScript 7 no longer checks a built
/// `lib/` directory into the legacy corpus repository, so the generated core
/// asset tree is the always-present byte-for-byte source for this guard.
#[test]
fn website_compiled_libs_match_generated_core_assets() {
    let root = repo_root();
    let website_lib = root.join("crates/tsz-website/src/lib");
    let core_lib = root.join("crates/tsz-core/src/lib-assets");
    let pairs: Vec<_> = files_under(&website_lib)
        .into_iter()
        .filter(|rel| {
            let name = rel.to_string_lossy();
            name.starts_with("lib.") && name.ends_with(".d.ts")
        })
        .map(|rel| {
            let website_name = rel.to_string_lossy();
            let core_name = if website_name == "lib.es5.full.d.ts" {
                "es5.full.d.ts".to_string()
            } else {
                website_name
                    .strip_prefix("lib.")
                    .expect("website lib names have lib prefix")
                    .to_string()
            };
            (
                rel.clone(),
                website_lib.join(&rel),
                core_lib.join(core_name),
            )
        })
        .collect();
    assert_no_drift(&pairs, "generated website TypeScript libs");
}

/// Every `TypeScript/...` fixture path a test passes to
/// `load_typescript_fixture` must have a vendored copy, so no test's
/// execution depends on the environment providing the checkout. Scans the
/// source of every `.rs` file under `crates/` that mentions the helper.
#[test]
fn all_fixture_paths_used_by_tests_are_vendored() {
    let root = repo_root();
    let crates_dir = root.join("crates");
    let vendor_dir = root.join("vendor");

    let mut missing = Vec::new();
    let mut referenced = 0usize;
    let rust_sources = files_under(&crates_dir)
        .into_iter()
        .filter(|rel| rel.extension().is_some_and(|ext| ext == "rs"));
    for rel in rust_sources {
        let path = crates_dir.join(&rel);
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if !source.contains("load_typescript_fixture") {
            continue;
        }
        // Any checkout-relative TypeScript-source-file string literal in a
        // file that loads fixtures is treated as a fixture path; false
        // positives would only demand vendoring a file the test suite
        // plausibly reads anyway.
        for chunk in source.split('"').skip(1).step_by(2) {
            let Some(fixture_rel) = chunk.strip_prefix("TypeScript/") else {
                continue;
            };
            if !(fixture_rel.ends_with(".ts") || fixture_rel.ends_with(".tsx")) {
                continue;
            }
            referenced += 1;
            if !vendor_dir.join("TypeScript").join(fixture_rel).is_file() {
                missing.push(format!("{} (from {})", chunk, rel.display()));
            }
        }
    }

    assert!(
        referenced > 0,
        "expected load_typescript_fixture call sites under crates/"
    );
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "fixture paths without a vendored copy under vendor/ (add them per \
         vendor/TypeScript/README.md):\n{}",
        missing.join("\n")
    );
}
