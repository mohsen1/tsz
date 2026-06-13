//! Canonical lexical path normalization shared across tsz crates.
//!
//! Module identity in tsz is textual: two `PathBuf`s with different segment
//! shapes are treated as distinct files even when they refer to the same
//! physical location. The resolver (in `tsz-core`) and the CLI driver (in
//! `tsz-cli`) both need to collapse `.`/`..` segments into a single canonical
//! spelling before a resolved path becomes a file-graph identity key. Keeping
//! two copies of that algorithm let them drift: a naive `PathBuf::pop` loop
//! pops *past* the filesystem root, so an absolute `/a/../../b` degrades to
//! `/../b` in one layer while the other clamps it to `/b`. The two spellings
//! then mint distinct module identities for one file — the "unstable canonical
//! IDs / duplicate declaration roots" module-resolution drift.
//!
//! This module owns the one true implementation so both layers share it.

use std::path::{Component, Path, PathBuf};

/// Lexically normalize a path: collapse `.`, resolve `..` against the preceding
/// *named* segment, and leave the path otherwise untouched. This is purely
/// textual — it never touches the filesystem — so it is the stable identity key
/// for files that cannot be canonicalized.
///
/// Two corrections over a naive `PathBuf::pop` loop, both of which otherwise let
/// one logical file mint several distinct identity keys:
/// - `..` clamps at the filesystem root / drive prefix (matching `tsc`/Node)
///   instead of popping past it, so an absolute `/a/../../b` stays absolute
///   (`/b`) rather than degrading to `/../b` or a relative `b`.
/// - leading `..` on a relative path is preserved (`../foo` stays `../foo`)
///   instead of being silently dropped.
pub fn normalize_segments(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    // Count of poppable `Normal` segments currently sitting above any root or
    // leading `..` run. Tracking it lets `..` resolve in a single pass without
    // re-parsing `normalized` on every parent segment.
    let mut poppable = 0usize;
    let mut rooted = false;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if poppable > 0 {
                    normalized.pop();
                    poppable -= 1;
                } else if !rooted {
                    // Relative path with nothing to pop: keep the leading `..`.
                    normalized.push("..");
                }
                // Otherwise `..` is at the filesystem root or a drive prefix,
                // where `tsc`/Node clamp instead of escaping root: a no-op.
            }
            Component::RootDir | Component::Prefix(_) => {
                rooted = true;
                normalized.push(component.as_os_str());
            }
            Component::Normal(segment) => {
                normalized.push(segment);
                poppable += 1;
            }
        }
    }

    normalized
}

/// Shared collapse loop for the string-domain helpers below.
///
/// Returns `false` only when `bail_on_underflow` is set and a `..` segment had
/// no collected segment left to cancel.
fn apply_slash_segments<'a>(
    segments: &mut Vec<&'a str>,
    specifier: &'a str,
    bail_on_underflow: bool,
) -> bool {
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() && bail_on_underflow {
                    return false;
                }
            }
            part => segments.push(part),
        }
    }
    true
}

/// String-domain sibling of [`normalize_segments`] for `/`-joined *virtual*
/// paths (ambient module names, AMD module ids) that never pass through
/// `std::path`. Collapses the segments of `specifier` onto an existing segment
/// stack: empty and `.` segments are skipped, `..` pops the most recently
/// collected segment — including segments the caller seeded from a base path,
/// which pop verbatim (a seeded `..` is itself poppable, matching the
/// historical emitter loops this replaces).
///
/// Underflow policy: an unmatched `..` is silently dropped (lossy). Callers
/// that must not lose an unmatched `..` use
/// [`resolve_relative_slash_specifier`], which bails instead; callers in the
/// `Path` domain use [`normalize_segments`], which keeps the `..` (relative)
/// or clamps it (rooted) the way `tsc`/Node do.
pub fn apply_slash_segments_lossy<'a>(segments: &mut Vec<&'a str>, specifier: &'a str) {
    apply_slash_segments(segments, specifier, false);
}

/// String-domain sibling of [`normalize_segments`]: resolve a relative
/// (`./`/`../`) module specifier against a `/`-joined virtual base directory,
/// collapsing `.`/`..` lexically. Empty and `.` segments are skipped; `..`
/// pops the most recently collected segment, including segments seeded from
/// `base_dir` (which pop verbatim — a seeded `..` is itself poppable).
///
/// Underflow policy: returns `None` when a `..` has no segment left to cancel
/// (the specifier escapes the virtual root) or when the result is empty, so
/// the caller chooses the fallback (typically the raw specifier).
pub fn resolve_relative_slash_specifier(base_dir: &str, specifier: &str) -> Option<String> {
    let mut segments: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    if !apply_slash_segments(&mut segments, specifier, true) {
        return None;
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

/// Returns `true` when `path` contains no `.` or `..` segment, i.e. it is
/// already lexically canonical and [`normalize_segments`] would be an identity
/// transform. Callers on hot probe paths use this to avoid an allocation.
pub fn is_already_normalized(path: &Path) -> bool {
    !path
        .components()
        .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_excess_parent_segments_at_root() {
        // The historical divergence: a naive pop loop produced `/../b` here,
        // while the driver clamped to `/b`. Both layers now agree on `/b`.
        assert_eq!(
            normalize_segments(Path::new("/a/../../b")),
            PathBuf::from("/b")
        );
        assert_eq!(
            normalize_segments(Path::new("/a/b/../../../c")),
            PathBuf::from("/c")
        );
        assert_eq!(
            normalize_segments(Path::new("/root/../x")),
            PathBuf::from("/x")
        );
    }

    #[test]
    fn preserves_leading_parent_on_relative_paths() {
        assert_eq!(
            normalize_segments(Path::new("../foo")),
            PathBuf::from("../foo")
        );
        assert_eq!(
            normalize_segments(Path::new("../../foo/bar")),
            PathBuf::from("../../foo/bar")
        );
        assert_eq!(
            normalize_segments(Path::new("a/../../foo")),
            PathBuf::from("../foo")
        );
    }

    #[test]
    fn collapses_interior_dot_and_parent_segments() {
        assert_eq!(normalize_segments(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(
            normalize_segments(Path::new("a/b/../c")),
            PathBuf::from("a/c")
        );
        assert_eq!(
            normalize_segments(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
    }

    #[test]
    fn equivalent_spellings_share_one_identity() {
        // Every spelling of the same absolute file collapses to one key, so the
        // file graph cannot mint duplicate declaration roots for it.
        let canonical = normalize_segments(Path::new("/pkg/lib/index.d.ts"));
        for spelling in [
            "/pkg/lib/index.d.ts",
            "/pkg/./lib/index.d.ts",
            "/pkg/lib/sub/../index.d.ts",
            "/pkg/extra/../../pkg/lib/index.d.ts",
        ] {
            assert_eq!(normalize_segments(Path::new(spelling)), canonical);
        }
    }

    #[test]
    fn resolve_relative_slash_specifier_collapses_against_base() {
        assert_eq!(
            resolve_relative_slash_specifier("src/lib", "./mod"),
            Some("src/lib/mod".to_string())
        );
        assert_eq!(
            resolve_relative_slash_specifier("src/lib", "../mod"),
            Some("src/mod".to_string())
        );
        assert_eq!(
            resolve_relative_slash_specifier("", "./mod"),
            Some("mod".to_string())
        );
        // Empty segments (doubled slashes) are skipped, matching the
        // historical AMD-resolver loops.
        assert_eq!(
            resolve_relative_slash_specifier("src", ".//.//mod"),
            Some("src/mod".to_string())
        );
        // Segments seeded from `base_dir` pop verbatim: a seeded `..` is
        // itself poppable, so it does not trigger the underflow bail.
        assert_eq!(
            resolve_relative_slash_specifier("../lib", "../mod"),
            Some("../mod".to_string())
        );
    }

    #[test]
    fn resolve_relative_slash_specifier_bails_on_underflow_and_empty() {
        // `..` escaping the virtual root: the caller picks the fallback.
        assert_eq!(resolve_relative_slash_specifier("", "../mod"), None);
        assert_eq!(resolve_relative_slash_specifier("src", "../../mod"), None);
        // Empty results also bail (`define()` dep arrays cannot hold "").
        assert_eq!(resolve_relative_slash_specifier("", "."), None);
        assert_eq!(resolve_relative_slash_specifier("src", ".."), None);
    }

    #[test]
    fn apply_slash_segments_lossy_drops_unmatched_parent() {
        let mut segments = vec!["pkg"];
        apply_slash_segments_lossy(&mut segments, "../../mod");
        // First `..` pops `pkg`; the second has nothing to cancel and is
        // dropped (lossy), matching the historical jsdoc ambient-module loop.
        assert_eq!(segments, vec!["mod"]);

        let mut segments: Vec<&str> = Vec::new();
        apply_slash_segments_lossy(&mut segments, "./a/../b");
        assert_eq!(segments, vec!["b"]);
    }

    #[test]
    fn is_already_normalized_matches_normalize_segments_fast_path() {
        // Already-canonical paths take the borrow fast path.
        assert!(is_already_normalized(Path::new("/a/b/c.ts")));
        assert!(is_already_normalized(Path::new("a/b/c.ts")));
        // `std::path::Path::components` collapses *interior* `.` (so `a/./b`
        // already has no `CurDir` component and compares equal to `a/b` as a
        // map key); a *leading* `.` and any `..` survive and must be normalized.
        assert!(!is_already_normalized(Path::new("./a/b")));
        assert!(!is_already_normalized(Path::new("a/../b")));
        // Any path the fast path skips must be byte-identical after normalizing,
        // so taking the borrow path never changes the resulting identity.
        for already in ["/a/b/c.ts", "a/b/c.ts", "../keep/me"] {
            let p = Path::new(already);
            if is_already_normalized(p) {
                assert_eq!(normalize_segments(p), PathBuf::from(already));
            }
        }
    }
}
