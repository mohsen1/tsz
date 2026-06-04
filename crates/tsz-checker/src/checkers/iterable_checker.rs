use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

use crate::query_boundaries::checkers::iterable::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind,
    async_iterable_protocol_lookup_type, call_signatures_for_type, classify_async_iterable_type,
    classify_for_of_element_type, classify_full_iterable_type, function_shape_for_type,
    is_array_type, is_string_literal_type, is_string_type, is_this_type, is_tuple_type,
    union_members_for_type,
};

use crate::query_boundaries::common;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_solver::TypeId;

include!("iterable_checker_parts/part1.rs");
include!("iterable_checker_parts/part2.rs");

/// The kind of iteration use, determining which diagnostic to emit
/// when the iterator's `next()` parameter type is incompatible.
pub enum IterationUseKind {
    /// `for (... of expr)` - emits TS2763
    ForOf,
    /// `[...expr]` - emits TS2764
    Spread,
    /// `let [x] = expr` or `[x] = expr` - emits TS2765
    Destructuring,
    /// `yield* expr` - emits TS2766
    YieldStar,
}
