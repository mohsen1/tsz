//! Focused unit tests for block-aware related-information normalization.
//!
//! `normalize_related_information_blocks` is the mechanism that lets sibling
//! elaboration chains anchored at one `(file, start)` round-trip: each chain is
//! normalized as a contiguous block instead of line-by-line. These tests drive
//! the free function directly with synthetic chains so the block boundaries,
//! per-block dedup, and stable block ordering are pinned independent of any
//! particular diagnostic that happens to produce them.

use crate::diagnostics::{DiagnosticCategory, DiagnosticRelatedInformation};
use crate::error_reporter::RelatedInformationPolicy;
use crate::error_reporter::fingerprint_policy::normalize_related_information_blocks;

/// Build a related-info line. `code`/`category` are held constant across a test
/// so ordering is driven purely by `(start, depth, message)`, matching the
/// sort keys under test.
fn rel(file: &str, start: u32, depth: u8, message: &str) -> DiagnosticRelatedInformation {
    DiagnosticRelatedInformation {
        category: DiagnosticCategory::Message,
        code: 2322,
        file: file.to_string(),
        start,
        length: 1,
        message_text: message.to_string(),
        depth,
    }
}

fn lines(items: &[DiagnosticRelatedInformation]) -> Vec<(u32, u8, String)> {
    items
        .iter()
        .map(|i| (i.start, i.depth, i.message_text.clone()))
        .collect()
}

/// A single chain keeps its header above its leaves. This is the byte-identical
/// path the previous per-line normalization already produced, so the block
/// machinery must not perturb it — even when the leaf is appended before the
/// header (the depth key, not construction order, decides intra-chain order).
#[test]
fn single_chain_orders_header_before_leaf_by_depth() {
    let input = vec![
        rel("a.ts", 10, 1, "Type 'X' is not assignable to type 'Y'."),
        rel("a.ts", 10, 0, "Types of property 'p' are incompatible."),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        lines(&out),
        vec![
            (10, 0, "Types of property 'p' are incompatible.".to_string()),
            (10, 1, "Type 'X' is not assignable to type 'Y'.".to_string()),
        ],
    );
}

/// Two sibling chains at one anchor that share a leaf line must both round-trip:
/// each chain stays contiguous and the shared leaf survives once per chain. The
/// former global sort interleaved the headers and the global dedup collapsed the
/// shared leaf to a single line — this is the exact bug the block model fixes.
#[test]
fn sibling_chains_at_one_anchor_keep_both_shared_leaves() {
    let input = vec![
        rel("a.ts", 10, 0, "Header A."),
        rel("a.ts", 10, 1, "Shared leaf."),
        rel("a.ts", 10, 0, "Header B."),
        rel("a.ts", 10, 1, "Shared leaf."),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        lines(&out),
        vec![
            (10, 0, "Header A.".to_string()),
            (10, 1, "Shared leaf.".to_string()),
            (10, 0, "Header B.".to_string()),
            (10, 1, "Shared leaf.".to_string()),
        ],
        "both chains stay contiguous and each keeps its own leaf"
    );
}

/// Sibling chains sharing an anchor render in construction order, not sorted by
/// header text. Here the first-built chain's header sorts *after* the
/// second-built one alphabetically; the old global sort would swap them, but tsc
/// (and overload declaration order) require the build order to survive.
#[test]
fn sibling_chains_preserve_construction_order_over_message_order() {
    let input = vec![
        rel("a.ts", 10, 0, "Zeta header."),
        rel("a.ts", 10, 1, "Zeta leaf."),
        rel("a.ts", 10, 0, "Alpha header."),
        rel("a.ts", 10, 1, "Alpha leaf."),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        lines(&out),
        vec![
            (10, 0, "Zeta header.".to_string()),
            (10, 1, "Zeta leaf.".to_string()),
            (10, 0, "Alpha header.".to_string()),
            (10, 1, "Alpha leaf.".to_string()),
        ],
        "same-anchor chains keep construction order regardless of header spelling"
    );
}

/// Chains at *different* anchors keep their former positional grouping: the
/// block ordering sort keys on the head `(file, start)`, so a later-built chain
/// at an earlier position still sorts ahead. This preserves the pre-existing
/// cross-location ordering that the flat sort's leading `(file, start)` keys gave.
#[test]
fn different_anchor_chains_order_by_head_position() {
    let input = vec![
        rel("a.ts", 20, 0, "Later position header."),
        rel("a.ts", 20, 1, "Later position leaf."),
        rel("a.ts", 10, 0, "Earlier position header."),
        rel("a.ts", 10, 1, "Earlier position leaf."),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        lines(&out),
        vec![
            (10, 0, "Earlier position header.".to_string()),
            (10, 1, "Earlier position leaf.".to_string()),
            (20, 0, "Later position header.".to_string()),
            (20, 1, "Later position leaf.".to_string()),
        ],
    );
}

/// Exact duplicate lines *within* one chain are still deduped, matching the old
/// behavior for a single chain.
#[test]
fn exact_duplicate_line_within_chain_is_deduped() {
    let input = vec![
        rel("a.ts", 10, 0, "Header."),
        rel("a.ts", 10, 1, "Leaf."),
        rel("a.ts", 10, 1, "Leaf."),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        lines(&out),
        vec![(10, 0, "Header.".to_string()), (10, 1, "Leaf.".to_string()),],
    );
}

/// Overload-style siblings: several single-line depth-0 chains at one anchor
/// (each an overload candidate's applicability error) keep declaration order
/// under `OVERLOAD_FAILURES`. The old sort alphabetized them; block ordering is
/// stable at a shared anchor, so this is now the default rather than a
/// `preserve_order` special case.
#[test]
fn overload_failure_siblings_keep_declaration_order() {
    let input = vec![
        rel("a.ts", 5, 0, "Overload 2: string is not assignable."),
        rel("a.ts", 5, 0, "Overload 1: number is not assignable."),
        rel("a.ts", 5, 0, "Overload 3: boolean is not assignable."),
    ];
    let out =
        normalize_related_information_blocks(input, RelatedInformationPolicy::OVERLOAD_FAILURES);
    assert_eq!(
        lines(&out),
        vec![
            (5, 0, "Overload 2: string is not assignable.".to_string()),
            (5, 0, "Overload 1: number is not assignable.".to_string()),
            (5, 0, "Overload 3: boolean is not assignable.".to_string()),
        ],
        "candidate failures render in the order they were built, not sorted by text"
    );
}

/// An identical leaf line under two *different* headers survives under both — the
/// per-block dedup never merges across chains even when every field but the
/// header matches.
#[test]
fn identical_leaf_under_different_headers_is_not_merged() {
    let input = vec![
        rel("a.ts", 10, 0, "Types of property 'a' are incompatible."),
        rel(
            "a.ts",
            10,
            1,
            "Type 'number' is not assignable to type 'string'.",
        ),
        rel("a.ts", 10, 0, "Types of property 'b' are incompatible."),
        rel(
            "a.ts",
            10,
            1,
            "Type 'number' is not assignable to type 'string'.",
        ),
    ];
    let out = normalize_related_information_blocks(input, RelatedInformationPolicy::ELABORATION);
    let leaf_count = out
        .iter()
        .filter(|i| i.message_text == "Type 'number' is not assignable to type 'string'.")
        .count();
    assert_eq!(
        leaf_count,
        2,
        "each chain keeps its own leaf: {:?}",
        lines(&out)
    );
}
