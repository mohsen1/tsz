//! Tests for deterministic canonical module-identity keys.
//!
//! Structural rule: when `preserveSymlinks` is off and a resolved path cannot
//! be canonicalized, module identity must key on the deterministic
//! lexically-normalized path — never the raw textual input — and lexical
//! normalization must clamp `..` at the filesystem root the way `tsc`/Node do.
//! The same logical file must therefore mint the same canonical ID regardless
//! of how the specifier was spelled.
//!
//! Coverage matrix:
//! - `.` / `..` collapse in lexical normalization
//! - `..` clamps at the filesystem root (no escape past root)
//! - leading `..` preserved on relative paths
//! - equivalent spellings of a *missing* file collapse to one canonical ID
//! - `preserveSymlinks` keeps the verbatim (lexically-normalized) path
//! - real on-disk files still canonicalize to their absolute real path
use super::{normalize_path, normalize_resolved_path};
use crate::config::{ModuleResolutionKind, ResolvedCompilerOptions};
use crate::driver::resolution::canonicalize_or_owned;
use std::path::{Path, PathBuf};

fn opts(preserve_symlinks: bool) -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks,
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}

#[test]
fn normalize_path_collapses_cur_and_parent_segments() {
    assert_eq!(
        normalize_path(Path::new("/a/./b/c")),
        PathBuf::from("/a/b/c")
    );
    assert_eq!(
        normalize_path(Path::new("/a/b/../c")),
        PathBuf::from("/a/c")
    );
    assert_eq!(
        normalize_path(Path::new("/a/b/../../c/./d")),
        PathBuf::from("/c/d")
    );
}

#[test]
fn normalize_path_clamps_parent_dir_at_root() {
    // `..` must never pop past the filesystem root; an absolute path stays
    // absolute. The naive `PathBuf::pop` loop dropped the root component here,
    // degrading `/a/../../b` into a *relative* `b` and splitting identity.
    assert_eq!(normalize_path(Path::new("/a/../../b")), PathBuf::from("/b"));
    assert_eq!(normalize_path(Path::new("/../../x")), PathBuf::from("/x"));
    assert_eq!(normalize_path(Path::new("/..")), PathBuf::from("/"));
}

#[test]
fn normalize_path_preserves_leading_parent_on_relative_paths() {
    // Relative paths have no root to clamp against, so leading `..` is real
    // and must survive normalization rather than being silently dropped.
    assert_eq!(normalize_path(Path::new("../foo")), PathBuf::from("../foo"));
    assert_eq!(
        normalize_path(Path::new("../../a/b")),
        PathBuf::from("../../a/b")
    );
    assert_eq!(
        normalize_path(Path::new("a/../../b")),
        PathBuf::from("../b")
    );
}

#[test]
fn missing_file_equivalent_spellings_share_one_canonical_id() {
    // A path that cannot be canonicalized must fall back to the lexically
    // normalized form, so every equivalent spelling of one (missing) file
    // produces a single identity key. Use a path guaranteed not to exist.
    let base = "/tsz-nonexistent-canonical-id-probe/pkg";
    let options = opts(false);
    let expected = PathBuf::from(format!("{base}/a/b.ts"));

    let id_of = |spelling: &str| normalize_resolved_path(Path::new(spelling), &options);
    assert_eq!(id_of(&format!("{base}/a/b.ts")), expected);
    assert_eq!(
        id_of(&format!("{base}/a/./b.ts")),
        expected,
        "`./` spelling must not split identity"
    );
    assert_eq!(
        id_of(&format!("{base}/a/x/../b.ts")),
        expected,
        "`..` spelling must not split identity"
    );
}

#[test]
fn preserve_symlinks_keeps_verbatim_normalized_path() {
    let options = opts(true);
    let p = Path::new("/tsz-nonexistent-canonical-id-probe/a/./b/../c.ts");
    assert_eq!(
        normalize_resolved_path(p, &options),
        PathBuf::from("/tsz-nonexistent-canonical-id-probe/a/c.ts")
    );
}

#[test]
fn canonicalize_or_owned_falls_back_to_lexically_normalized_path() {
    // The shared helper that feeds program-file caches, dedup sets, and
    // redirect maps must also collapse equivalent spellings of a missing file
    // to one key — not echo the raw input — so identity stays deterministic.
    let base = "/tsz-nonexistent-canonical-id-probe/lib";
    let expected = PathBuf::from(format!("{base}/x/y.ts"));
    assert_eq!(
        canonicalize_or_owned(Path::new(&format!("{base}/x/y.ts"))),
        expected
    );
    assert_eq!(
        canonicalize_or_owned(Path::new(&format!("{base}/x/./y.ts"))),
        expected
    );
    assert_eq!(
        canonicalize_or_owned(Path::new(&format!("{base}/x/z/../y.ts"))),
        expected
    );
}

#[test]
fn existing_file_canonicalizes_to_absolute_real_path() {
    use std::fs;

    let dir = tempfile::TempDir::new().expect("temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("sub/mod.ts"), "export {};").unwrap();

    let options = opts(false);
    // A `.`-laden spelling of a real file resolves to its canonical real path,
    // which is itself absolute and free of `.`/`..` segments.
    let spelled = root.join("sub/./mod.ts");
    let canonical = normalize_resolved_path(&spelled, &options);
    let expected = fs::canonicalize(root.join("sub/mod.ts")).unwrap();
    assert_eq!(canonical, expected);
}
