//! The one owner of the "child of a header" elaboration-depth rule shared by
//! every nested-mismatch renderer under `render_failure`.

/// Elaboration depth of the *first child* of a header rendered at chain depth
/// `depth`. A top-level mismatch (`depth == 0`) is the diagnostic's main
/// message header itself, so its first child stays at elaboration depth `0`
/// (indent `2` under the renderer's `2 * (depth + 1)`-space rule); a nested
/// header at `depth > 0` is already an elaboration line, so its first child
/// sits one level deeper at `depth + 1`.
///
/// Every nested renderer that hangs a note, leaf, frame, or constraint-walk
/// step beneath a header shares this rule. Funnel it through here rather than
/// re-deriving the `depth == 0` special case at each site: re-deriving it is
/// how a `base_depth + 1` seed over-indented a whole subtree by one level
/// (see #17797 / #17718), and a single owner keeps the convention in one place.
pub(in crate::error_reporter) const fn first_child_depth(depth: u32) -> u32 {
    if depth == 0 { 0 } else { depth + 1 }
}
