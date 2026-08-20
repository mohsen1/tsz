//! Unit matrix for block-aware related-information normalization (#15388).
//!
//! These drive the pure `normalize_related_information_blocks` normalizer
//! directly with synthetic elaboration chains — no binder, no `CheckerState`,
//! and no user-chosen identifier spelling (every message is a fixed literal), so
//! the assertions cannot ride on a particular type or property name.
//!
//! The property under test: `tsc` renders each root-anchored elaboration chain
//! contiguously in construction order and never dedupes across chains. A
//! per-line normalizer (global dedup + `(file, start, depth, message)` sort)
//! cannot represent two sibling chains at one `(file, start)` anchor — it
//! interleaves them by depth and merges an identical leaf that legitimately
//! sits under two different headers. The block normalizer scopes dedup and the
//! depth sort to a single chain and orders chains by their head anchor with a
//! stable sort, so sibling chains survive whole and in build order while
//! single-chain output stays byte-identical.

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind,
};
use crate::error_reporter::RelatedInformationPolicy;
use crate::error_reporter::fingerprint_policy::normalize_related_information_blocks;

/// Build a chain link at `start`/`depth` with message `msg`. `code`/`length`
/// participate in the dedup key; the default `code = 1` keeps identical-message
/// leaves genuinely identical so the cross-chain dedup behavior is exercised.
fn link(start: u32, depth: u8, msg: &str) -> DiagnosticRelatedInformation {
    DiagnosticRelatedInformation {
        category: DiagnosticCategory::Message,
        code: 1,
        file: "a.ts".to_string(),
        start,
        length: 1,
        message_text: msg.to_string(),
        depth,
        kind: RelatedInformationKind::ChainLink,
    }
}

fn messages(items: &[DiagnosticRelatedInformation]) -> Vec<&str> {
    items.iter().map(|i| i.message_text.as_str()).collect()
}

#[test]
fn single_chain_keeps_header_above_leaf() {
    // One chain, appended head-first: the output is byte-identical to the
    // former per-line normalization (header at depth 0, leaf at depth 1).
    let items = vec![
        link(0, 0, "Types of property 'p' are incompatible."),
        link(0, 1, "Type 'X' is not assignable to type 'Y'."),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "Types of property 'p' are incompatible.",
            "Type 'X' is not assignable to type 'Y'.",
        ]
    );
}

#[test]
fn single_chain_depth_sort_seats_header_above_leaf_regardless_of_input_order() {
    // A deeper line that arrives before its head must still land in the head's
    // block (partition is insensitive to intra-chain ordering) and the
    // depth-major within-block sort seats the head above it.
    let items = vec![
        link(0, 1, "Type 'X' is not assignable to type 'Y'."),
        link(0, 0, "Types of property 'p' are incompatible."),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "Types of property 'p' are incompatible.",
            "Type 'X' is not assignable to type 'Y'.",
        ]
    );
}

#[test]
fn sibling_chains_at_one_anchor_sharing_a_leaf_both_survive_contiguous() {
    // The reported failure: two chains anchored at the same (file, start), each
    // with an identical leaf line. Under a global dedup the second leaf would be
    // merged away; under a global depth sort the two headers would be pulled
    // ahead of both leaves. Block-scoped normalization keeps each chain whole
    // and both leaves present.
    let items = vec![
        link(0, 0, "Overload 1 of 2 gave the following error."),
        link(0, 1, "Type 'A' is not assignable to type 'B'."),
        link(0, 0, "Overload 2 of 2 gave the following error."),
        link(0, 1, "Type 'A' is not assignable to type 'B'."),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "Overload 1 of 2 gave the following error.",
            "Type 'A' is not assignable to type 'B'.",
            "Overload 2 of 2 gave the following error.",
            "Type 'A' is not assignable to type 'B'.",
        ]
    );
}

#[test]
fn sibling_chains_render_in_construction_order_not_header_message_order() {
    // Two sibling chains at one anchor whose headers are in reverse alphabetical
    // order. The global message sort used to reorder them alphabetically; the
    // stable block sort keeps them in the order they were built.
    let items = vec![
        link(0, 0, "Zebra header."),
        link(0, 1, "leaf-z"),
        link(0, 0, "Apple header."),
        link(0, 1, "leaf-a"),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec!["Zebra header.", "leaf-z", "Apple header.", "leaf-a"]
    );
}

#[test]
fn different_anchor_chains_order_by_head_position() {
    // Chains at distinct anchors keep the former positional ordering: the block
    // sort keys on the head's (file, start), so the earlier-positioned head wins
    // regardless of construction order.
    let items = vec![
        link(10, 0, "second-position header."),
        link(10, 1, "second-position leaf"),
        link(5, 0, "first-position header."),
        link(5, 1, "first-position leaf"),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "first-position header.",
            "first-position leaf",
            "second-position header.",
            "second-position leaf",
        ]
    );
}

#[test]
fn exact_duplicate_line_within_a_chain_is_still_deduped() {
    // Block-scoped dedup still drops an exact duplicate that sits inside the same
    // chain (same category/code/file/start/length/message).
    let items = vec![
        link(0, 0, "Types of property 'p' are incompatible."),
        link(0, 1, "Type 'X' is not assignable to type 'Y'."),
        link(0, 1, "Type 'X' is not assignable to type 'Y'."),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "Types of property 'p' are incompatible.",
            "Type 'X' is not assignable to type 'Y'.",
        ]
    );
}

#[test]
fn identical_leaf_under_two_different_headers_is_not_merged() {
    // The cross-chain half of the dedup fix, isolated: two chains at one anchor
    // whose headers differ but whose leaves are byte-identical. Both leaves must
    // survive, one under each header.
    let items = vec![
        link(0, 0, "First header."),
        link(0, 1, "shared leaf"),
        link(0, 0, "Second header."),
        link(0, 1, "shared leaf"),
    ];
    let out = normalize_related_information_blocks(items, RelatedInformationPolicy::ELABORATION);
    assert_eq!(
        messages(&out),
        vec![
            "First header.",
            "shared leaf",
            "Second header.",
            "shared leaf",
        ]
    );
    assert_eq!(
        out.iter()
            .filter(|i| i.message_text == "shared leaf")
            .count(),
        2
    );
}

#[test]
fn overload_chains_keep_declaration_order_with_dedupe_off() {
    // OVERLOAD_CHAINS (dedupe off) is the production surface that formerly needed
    // the `preserve_order` flag: several sibling chains at the call anchor in
    // declaration order. Block ordering (stable, keyed on the shared head anchor)
    // reproduces that without a special case, and identical-failing overloads
    // keep both bodies.
    let items = vec![
        link(0, 0, "Overload 1 of 3 gave the following error."),
        link(0, 1, "Argument of type 'A' is not assignable."),
        link(0, 0, "Overload 2 of 3 gave the following error."),
        link(0, 1, "Argument of type 'A' is not assignable."),
        link(0, 0, "Overload 3 of 3 gave the following error."),
        link(0, 1, "Argument of type 'A' is not assignable."),
    ];
    let out =
        normalize_related_information_blocks(items, RelatedInformationPolicy::OVERLOAD_CHAINS);
    assert_eq!(
        messages(&out),
        vec![
            "Overload 1 of 3 gave the following error.",
            "Argument of type 'A' is not assignable.",
            "Overload 2 of 3 gave the following error.",
            "Argument of type 'A' is not assignable.",
            "Overload 3 of 3 gave the following error.",
            "Argument of type 'A' is not assignable.",
        ]
    );
}

#[test]
fn empty_input_normalizes_to_empty() {
    let out =
        normalize_related_information_blocks(Vec::new(), RelatedInformationPolicy::ELABORATION);
    assert!(out.is_empty());
}
