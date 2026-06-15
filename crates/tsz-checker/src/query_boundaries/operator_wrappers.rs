pub(crate) const fn is_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_compound_assignment_operator(operator_token)
}

pub(crate) const fn is_logical_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_logical_compound_assignment_operator(
        operator_token,
    )
}

pub(crate) const fn is_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_assignment_operator(operator_token)
}

pub(crate) const fn map_compound_assignment_to_binary(operator_token: u16) -> Option<&'static str> {
    tsz_solver::operations::compound_assignment::map_compound_assignment_to_binary(operator_token)
}

/// Classification of a binary equality/inequality comparison operator
/// (`===`, `!==`, `==`, `!=`).
///
/// This is the single source of truth for decoding the four equality operator
/// tokens used by the condition-interpretation narrowing pipelines. Both the
/// `TypeGuard`-producing classifier (`extract_type_guard`) and the direct
/// flow narrowing path (`narrow_by_binary_expr`) — plus the nullish, boolean,
/// and assignment-proving helpers — previously re-encoded this decode
/// independently, which let coverage drift between sites. Keeping the decode in
/// one place means a new operator form (or a change to equality polarity) is
/// added once, not in every pattern matcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EqualityComparison {
    /// `true` for `===`/`==` (equals), `false` for `!==`/`!=` (not-equals).
    pub(crate) is_equals: bool,
    /// `true` for `===`/`!==` (strict), `false` for `==`/`!=` (loose).
    pub(crate) is_strict: bool,
}

impl EqualityComparison {
    /// Effective truth of the underlying comparison for a given branch.
    ///
    /// On an equality operator the comparison is true exactly on the true
    /// branch; on an inequality operator the polarity flips. This mirrors the
    /// `if is_equals { is_true_branch } else { !is_true_branch }` decode that
    /// each narrowing site applied locally.
    pub(crate) const fn effective_truth(self, is_true_branch: bool) -> bool {
        if self.is_equals {
            is_true_branch
        } else {
            !is_true_branch
        }
    }
}

/// Classify a binary operator token as an equality/inequality comparison.
///
/// Returns `None` for any operator that is not one of `===`, `!==`, `==`, `!=`.
pub(crate) const fn classify_equality_comparison(
    operator_token: u16,
) -> Option<EqualityComparison> {
    use tsz_scanner::SyntaxKind;
    match operator_token {
        k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => Some(EqualityComparison {
            is_equals: true,
            is_strict: true,
        }),
        k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => Some(EqualityComparison {
            is_equals: false,
            is_strict: true,
        }),
        k if k == SyntaxKind::EqualsEqualsToken as u16 => Some(EqualityComparison {
            is_equals: true,
            is_strict: false,
        }),
        k if k == SyntaxKind::ExclamationEqualsToken as u16 => Some(EqualityComparison {
            is_equals: false,
            is_strict: false,
        }),
        _ => None,
    }
}

/// `true` when the operator is any equality/inequality comparison
/// (`===`, `!==`, `==`, `!=`).
pub(crate) const fn is_equality_comparison_operator(operator_token: u16) -> bool {
    classify_equality_comparison(operator_token).is_some()
}

/// `true` when the operator is a loose equality/inequality comparison
/// (`==`, `!=`).
pub(crate) const fn is_loose_equality_operator(operator_token: u16) -> bool {
    match classify_equality_comparison(operator_token) {
        Some(comparison) => !comparison.is_strict,
        None => false,
    }
}
