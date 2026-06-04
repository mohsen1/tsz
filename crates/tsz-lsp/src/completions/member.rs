use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct PropertyCompletion {
    pub type_id: TypeId,
    pub is_method: bool,
    pub is_optional: bool,
}

include!("member_parts/part1.rs");
include!("member_parts/part2.rs");

/// Members inherited from `Object.prototype`. They appear on every object via
/// the prototype chain but TypeScript only lists them in member completions
/// for types that explicitly redeclare them on their own interface.
const OBJECT_PROTOTYPE_ONLY_MEMBERS: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
];

/// String methods that are not part of the ES2015 baseline that our no-lib
/// apparent fallback covers. When the user has not loaded a TypeScript lib
/// (e.g. the playground default), tsc would not surface these names because
/// the user's effective target predates their introduction. The apparent
/// fallback still answers property lookups for them so that real code
/// targeting modern runtimes keeps type-checking; this filter just hides them
/// from the completion list, matching tsc's behavior.
const STRING_POST_ES2015_MEMBERS: &[&str] = &[
    // es2017
    "padStart",
    "padEnd",
    // es2019
    "trimStart",
    "trimEnd",
    "trimLeft",
    "trimRight",
    // es2020
    "matchAll",
    // es2021
    "replaceAll",
    // es2022
    "at",
    // esnext
    "isWellFormed",
    "toWellFormed",
];

/// Whether `name` is owned by the primitive's own lib interface (and
/// therefore should appear in completions) versus being inherited from
/// `Object.prototype` (and therefore hidden, matching tsc).
///
/// Only the actual primitive kinds — `String`, `Number`, `Boolean`, `Bigint`,
/// `Symbol` — are filtered. `Object` (the non-primitive `object` type) and
/// `Function` keep their full apparent-member set so completions on
/// `o: object` continue to surface every `Object.prototype` method.
fn primitive_member_is_completion_eligible(kind: IntrinsicKind, name: &str) -> bool {
    if !matches!(
        kind,
        IntrinsicKind::String
            | IntrinsicKind::Number
            | IntrinsicKind::Boolean
            | IntrinsicKind::Bigint
            | IntrinsicKind::Symbol,
    ) {
        return true;
    }
    if OBJECT_PROTOTYPE_ONLY_MEMBERS.contains(&name) {
        return false;
    }
    // `Boolean`'s lib.es5 interface declares only `valueOf`; every other
    // primitive interface redeclares `toString`. So `toString` is only
    // inherited (and therefore filtered) for booleans.
    if name == "toString" && matches!(kind, IntrinsicKind::Boolean) {
        return false;
    }
    // `Number` and `Bigint` redeclare `toLocaleString` on their lib.es5
    // interfaces; all other primitives inherit it from `Object.prototype`.
    if name == "toLocaleString" && !matches!(kind, IntrinsicKind::Number | IntrinsicKind::Bigint) {
        return false;
    }
    if matches!(kind, IntrinsicKind::String) && STRING_POST_ES2015_MEMBERS.contains(&name) {
        return false;
    }
    true
}

/// Dot-access completions can only commit names that are directly usable after
/// `receiver.`. Computed members are stored with bracketed display labels such
/// as `[Symbol.iterator]`, which belong to element-access completions instead.
fn dot_access_member_label_is_completion_eligible(name: &str) -> bool {
    !(name.starts_with('[') && name.ends_with(']'))
}
