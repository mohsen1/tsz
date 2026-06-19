//! Shared apparent-type display strings for index/destructuring diagnostics.

use tsz_solver::TypeId;

/// Display string for the *apparent* type of a primitive/`object` intrinsic, as
/// used in implicit-any index and destructuring diagnostics (`TS7053`/`TS2538`/
/// `TS2339`). `tsc` reports `typeToString(getApparentType(receiver))` for these
/// failures, so the `object` intrinsic widens to the empty object type `{}` and
/// each primitive widens to its global wrapper interface (`number` -> `Number`,
/// etc.). Returns `None` for any other type so callers fall back to the normal
/// type formatter.
pub(crate) fn apparent_intrinsic_type_display(type_id: TypeId) -> Option<String> {
    match type_id {
        TypeId::OBJECT => Some("{}".to_string()),
        TypeId::STRING => Some("String".to_string()),
        TypeId::NUMBER => Some("Number".to_string()),
        TypeId::BOOLEAN => Some("Boolean".to_string()),
        TypeId::BIGINT => Some("BigInt".to_string()),
        TypeId::SYMBOL => Some("Symbol".to_string()),
        _ => None,
    }
}
