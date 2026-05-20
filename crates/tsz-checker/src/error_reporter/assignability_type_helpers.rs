//! Small helper predicates used by assignability diagnostics.

use tsz_solver::TypeId;

/// Returns true if the formatted type name matches a built-in wrapper type
/// (Boolean, Number, String, Object). These types inherit properties from Object
/// and missing-property diagnostics should be suppressed in favor of TS2322.
pub(super) fn is_builtin_wrapper_name(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String" | "Object")
}

/// Returns true if the formatted type name represents a TypeScript primitive type.
/// This catches cases where a complex type (e.g., homomorphic mapped type over a
/// primitive) evaluates/displays as a primitive, even if the solver's TypeId doesn't
/// directly represent the primitive.
pub(crate) fn is_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "number"
            | "boolean"
            | "bigint"
            | "symbol"
            | "void"
            | "undefined"
            | "null"
            | "never"
    )
}

/// Returns true when a formatted-type display represents a single literal
/// value (a quoted string, a numeric literal, or one of the keyword
/// literals `true`/`false`/`null`/`undefined`).
///
/// TS2719 is meant for two NOMINAL types that share a name but are
/// structurally distinct (typically merged-declaration ambiguity). Literal
/// values have no nominal identity, so identical literal-value displays
/// always mean identical types — emitting TS2719 with messages like
/// `Type '"foo"' is not assignable to type '"foo"'` is misleading.
pub(crate) fn display_is_literal_value(s: &str) -> bool {
    if s == "true" || s == "false" || s == "null" || s == "undefined" {
        return true;
    }
    if s.starts_with('"') || s.starts_with('\'') || s.starts_with('`') {
        return true;
    }
    let bare = s.strip_prefix('-').unwrap_or(s);
    let mut chars = bare.chars();
    chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.all(|c| {
            c.is_ascii_digit()
                || c == '.'
                || c == 'e'
                || c == 'E'
                || c == '+'
                || c == '-'
                || c == 'n'
        })
}

/// Returns true if the name is a reserved type name that cannot be used as
/// an interface or class name (TS2427/TS2414). Matches tsc's
/// `checkTypeNameIsReserved` which checks the `typeNames` set.
pub(crate) fn is_reserved_type_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
}

/// Returns true if the display string looks like a function/callable type.
/// Used as a fallback when TypeId-level detection fails due to TypeQuery/Lazy wrapping.
/// Function types display as `(params) => ReturnType`.
pub(super) fn is_function_type_display(name: &str) -> bool {
    // A function type display always starts with `(` and contains `) => `.
    name.starts_with('(') && name.contains(") => ")
}

/// Returns true if the property name is a standard Object.prototype method.
/// These are implicitly available on all interfaces/objects through the Object
/// prototype chain. When such a property appears as "missing" in a subtype check,
/// it typically means the source type inherits it implicitly but its `ObjectShape`
/// doesn't include it. In this case, the mismatch is a type compatibility issue
/// (TS2322), not a missing property issue (TS2741).
pub(super) fn is_object_prototype_method(name: impl AsRef<str>) -> bool {
    matches!(
        name.as_ref(),
        "valueOf"
            | "toString"
            | "toLocaleString"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "constructor"
    )
}

/// Subset of Object.prototype methods that should still be reported as missing
/// when the target is an array-like type. Array types override `toString` and
/// `toLocaleString` with their own signatures, so these should NOT be filtered
/// out from TS2739/TS2740 missing property lists for array targets.
pub(super) fn is_object_prototype_method_for_array_target(name: impl AsRef<str>) -> bool {
    matches!(
        name.as_ref(),
        "valueOf" | "hasOwnProperty" | "isPrototypeOf" | "propertyIsEnumerable" | "constructor"
    )
}

/// Check if a type is a callable application type.
/// This checks if it's an Application type whose base is a callable/function type,
/// or if it's directly a callable/function type.
pub(super) fn is_callable_application_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    // Check if it's an application of a callable type
    if let Some(app) = crate::query_boundaries::common::type_application(db, type_id) {
        crate::query_boundaries::common::callable_shape_for_type(db, app.base).is_some()
            || crate::query_boundaries::common::function_shape_for_type(db, app.base).is_some()
    } else {
        // Also check if it's directly a callable/function type
        crate::query_boundaries::common::callable_shape_for_type(db, type_id).is_some()
            || crate::query_boundaries::common::function_shape_for_type(db, type_id).is_some()
    }
}

/// Check if a callable/function type has its own signature-level type parameters.
pub(super) fn has_own_signature_type_params(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, type_id) {
        return shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(|sig| !sig.type_params.is_empty());
    }
    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, type_id) {
        return !shape.type_params.is_empty();
    }
    false
}
