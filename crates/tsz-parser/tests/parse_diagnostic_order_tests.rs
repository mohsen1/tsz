//! Tests for the canonical, deterministic ordering of parse diagnostics.
//!
//! These lock the rule that parse diagnostics are sorted through
//! [`ParseDiagnostic::compare`] (tsc's `compareDiagnostics` key restricted to a
//! single file): by start, then length, then code, then message. Sorting by
//! position alone left ties resolved by scanner/parser merge order, which made
//! the reported order fragile under reordering.

use crate::parser::state::ParseDiagnostic;

fn diag(start: u32, length: u32, code: u32, message: &str) -> ParseDiagnostic {
    ParseDiagnostic {
        start,
        length,
        message: message.to_string(),
        code,
    }
}

/// Sorting through the canonical comparator yields the same order no matter how
/// the input was permuted — the property that keeps reported parse-diagnostic
/// order deterministic.
fn assert_permutation_invariant(canonical: &[ParseDiagnostic]) {
    for window in canonical.windows(2) {
        assert_ne!(
            window[0].compare(&window[1]),
            std::cmp::Ordering::Greater,
            "input slice is expected to already be in canonical order"
        );
    }
    let keys: Vec<_> = canonical
        .iter()
        .map(|d| (d.start, d.length, d.code, d.message.clone()))
        .collect();

    let mut reversed = canonical.to_vec();
    reversed.reverse();
    let mut rotated = canonical.to_vec();
    rotated.rotate_left(canonical.len() / 2);
    for mut permutation in [reversed, rotated] {
        permutation.sort_by(|a, b| a.compare(b));
        let got: Vec<_> = permutation
            .iter()
            .map(|d| (d.start, d.length, d.code, d.message.clone()))
            .collect();
        assert_eq!(got, keys);
    }
}

#[test]
fn parse_diagnostic_order_breaks_position_ties_by_length_code_message() {
    let canonical = vec![
        diag(0, 5, 1005, "alpha"),
        // same start, shorter length sorts first
        diag(10, 2, 9999, "zzz"),
        diag(10, 4, 1000, "aaa"),
        // same start+length, lower code first
        diag(20, 3, 1109, "msg"),
        diag(20, 3, 1128, "msg"),
        // same start+length+code, message breaks the tie
        diag(30, 1, 1005, "aaa"),
        diag(30, 1, 1005, "bbb"),
    ];
    assert_permutation_invariant(&canonical);
}

#[test]
fn parse_diagnostic_compare_is_total_and_antisymmetric() {
    let a = diag(5, 2, 1005, "msg");
    let b = a.clone();
    assert_eq!(a.compare(&b), std::cmp::Ordering::Equal);

    let c = diag(5, 2, 1005, "msg2");
    assert_eq!(a.compare(&c), std::cmp::Ordering::Less);
    assert_eq!(c.compare(&a), std::cmp::Ordering::Greater);

    // Position ties never collapse to `Equal` when later keys differ.
    let shorter = diag(5, 1, 9999, "z");
    let longer = diag(5, 9, 1000, "a");
    assert_eq!(shorter.compare(&longer), std::cmp::Ordering::Less);
}
