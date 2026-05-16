use crate::TypeDatabase;
use crate::types::{
    IndexSignature, IntrinsicKind, ObjectFlags, ObjectShape, PropertyInfo, TypeId, Visibility,
};

pub enum ApparentMemberKind {
    Value(TypeId),
    Method(TypeId),
}

pub struct ApparentMember {
    pub name: &'static str,
    pub kind: ApparentMemberKind,
}

// `at` (es2022) is intentionally absent: when a real lib is loaded that
// predates es2022, the bootstrap fallback must not paper over the
// property-not-found result, so the checker can emit the correct TS2550
// "change your target library" suggestion. Other version-specific methods
// remain for now to preserve existing fallback behavior; remove them
// individually as their conformance regressions are addressed.
//
// Also note that `at` returns `string | undefined`, not `string`, so it
// would have been incorrect to keep here even setting the lib question
// aside.
const STRING_METHODS_RETURN_STRING: &[&str] = &[
    "anchor",
    "big",
    "blink",
    "bold",
    "charAt",
    "concat",
    "fixed",
    "fontcolor",
    "fontsize",
    "italics",
    "link",
    "normalize",
    "padEnd",
    "padStart",
    "repeat",
    "replace",
    "replaceAll",
    "slice",
    "small",
    "strike",
    "sub",
    "substr",
    "substring",
    "sup",
    "toLocaleLowerCase",
    "toLocaleUpperCase",
    "toLowerCase",
    "toString",
    "toUpperCase",
    "trim",
    "trimEnd",
    "trimLeft",
    "trimRight",
    "trimStart",
    "toWellFormed",
    "valueOf",
];
const STRING_METHODS_RETURN_NUMBER: &[&str] = &[
    "charCodeAt",
    "codePointAt",
    "indexOf",
    "lastIndexOf",
    "localeCompare",
    "search",
];
const STRING_METHODS_RETURN_BOOLEAN: &[&str] =
    &["endsWith", "includes", "isWellFormed", "startsWith"];
const STRING_METHODS_RETURN_ANY: &[&str] = &["match", "matchAll"];
const STRING_METHODS_RETURN_STRING_ARRAY: &[&str] = &["split"];

const NUMBER_METHODS_RETURN_STRING: &[&str] = &[
    "toExponential",
    "toFixed",
    "toLocaleString",
    "toPrecision",
    "toString",
];

const BOOLEAN_METHODS_RETURN_STRING: &[&str] = &["toLocaleString", "toString"];

const BIGINT_METHODS_RETURN_STRING: &[&str] = &["toLocaleString", "toString"];

const OBJECT_METHODS_RETURN_BOOLEAN: &[&str] =
    &["hasOwnProperty", "isPrototypeOf", "propertyIsEnumerable"];
const OBJECT_METHODS_RETURN_STRING: &[&str] = &["toString"];
const OBJECT_METHODS_RETURN_ANY: &[&str] = &["valueOf"];

/// String methods introduced after ES2015 that must not appear in the no-lib
/// completion fallback. ES2015 methods (`includes`, `startsWith`, `endsWith`,
/// `codePointAt`, `repeat`, `normalize`) remain because they are in the default
/// ES2015 baseline that the fallback targets.
const STRING_POST_ES2015_MEMBERS: &[&str] = &[
    // es2017
    "padStart",
    "padEnd",
    // es2019 (trimLeft/trimRight are non-standard aliases of trimStart/trimEnd)
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

pub(crate) fn is_member(name: &str, list: &[&str]) -> bool {
    list.contains(&name)
}

fn object_member_kind(name: &str, include_to_locale: bool) -> Option<ApparentMemberKind> {
    if name == "constructor" {
        return Some(ApparentMemberKind::Value(TypeId::FUNCTION));
    }
    if name == "toString" {
        return Some(ApparentMemberKind::Method(TypeId::STRING));
    }
    if name == "valueOf" {
        return Some(ApparentMemberKind::Method(TypeId::ANY));
    }
    if is_member(name, OBJECT_METHODS_RETURN_BOOLEAN) {
        return Some(ApparentMemberKind::Method(TypeId::BOOLEAN));
    }
    if is_member(name, OBJECT_METHODS_RETURN_STRING) {
        return Some(ApparentMemberKind::Method(TypeId::STRING));
    }
    if is_member(name, OBJECT_METHODS_RETURN_ANY) {
        return Some(ApparentMemberKind::Method(TypeId::ANY));
    }
    if include_to_locale && name == "toLocaleString" {
        return Some(ApparentMemberKind::Method(TypeId::STRING));
    }
    None
}

fn push_object_members(members: &mut Vec<ApparentMember>, include_to_locale: bool) {
    members.push(ApparentMember {
        name: "constructor",
        kind: ApparentMemberKind::Value(TypeId::ANY),
    });
    members.push(ApparentMember {
        name: "toString",
        kind: ApparentMemberKind::Method(TypeId::STRING),
    });
    members.push(ApparentMember {
        name: "valueOf",
        kind: ApparentMemberKind::Method(TypeId::ANY),
    });
    for &name in OBJECT_METHODS_RETURN_BOOLEAN {
        members.push(ApparentMember {
            name,
            kind: ApparentMemberKind::Method(TypeId::BOOLEAN),
        });
    }
    for &name in OBJECT_METHODS_RETURN_STRING {
        members.push(ApparentMember {
            name,
            kind: ApparentMemberKind::Method(TypeId::STRING),
        });
    }
    for &name in OBJECT_METHODS_RETURN_ANY {
        members.push(ApparentMember {
            name,
            kind: ApparentMemberKind::Method(TypeId::ANY),
        });
    }
    if include_to_locale {
        members.push(ApparentMember {
            name: "toLocaleString",
            kind: ApparentMemberKind::Method(TypeId::STRING),
        });
    }
}

pub fn apparent_object_member_kind(name: &str) -> Option<ApparentMemberKind> {
    object_member_kind(name, true)
}

/// Whether `name` is a primitive member that was added to its boxed interface
/// in lib.es2015.* or later. The bootstrap fallback in
/// `resolve_primitive_property` consults this so that, when a real lib is
/// loaded that predates the property's introduction (e.g. es5 with
/// `String.includes`), the not-found result from the boxed interface
/// propagates to the checker (which then emits TS2550 / TS2339) instead of
/// being silently resolved by the no-lib fallback.
pub fn is_post_es5_primitive_member(kind: IntrinsicKind, name: &str) -> bool {
    match kind {
        IntrinsicKind::String => matches!(
            name,
            // es2015 (lib.es2015.core)
            "codePointAt"
                | "includes"
                | "endsWith"
                | "normalize"
                | "repeat"
                | "startsWith"
                // es2017
                | "padStart"
                | "padEnd"
                // es2019
                | "trimStart"
                | "trimEnd"
                | "trimLeft"
                | "trimRight"
                // es2020
                | "matchAll"
                // es2021
                | "replaceAll"
                // es2022
                | "at"
                // esnext
                | "isWellFormed"
                | "toWellFormed"
        ),
        IntrinsicKind::Number => matches!(
            name,
            // es2015: Number gained no instance methods (constructor only — handled separately)
            "" // placeholder so the matches! body is non-empty without listing es5 baselines
        ),
        IntrinsicKind::Symbol => matches!(
            name,
            // es2018
            "asyncIterator"
                // es2019
                | "description"
        ),
        // Boolean and Bigint do not gain new prototype members in any
        // post-es5 lib that the apparent fallback covers; nothing to
        // gate here.
        _ => false,
    }
}

/// Returns `true` if `name` belongs to the primitive type's **own** TypeScript
/// interface at the ES2015 baseline, and therefore should appear in the no-lib
/// fallback completion list.
///
/// The two structural conditions that jointly define eligibility are:
///
/// 1. **Own-interface membership** — the member is declared in the TypeScript
///    `String`/`Number`/`Boolean`/`BigInt`/`Symbol` interface, not only
///    inherited from `Object.prototype`. Members like `hasOwnProperty`,
///    `isPrototypeOf`, `propertyIsEnumerable`, and `constructor` are excluded
///    from every primitive; `toLocaleString`/`toString` are excluded from
///    `Boolean` because its interface declares only `valueOf`.
///
/// 2. **ES2015 baseline** — string members introduced after ES2015
///    (`padStart`/`padEnd`, `trimStart`/`trimEnd`, `matchAll`, `replaceAll`,
///    `at`, `isWellFormed`, `toWellFormed`, and the non-standard `trimLeft`/
///    `trimRight` aliases) are excluded. Those members are only available when
///    the caller has configured the appropriate `lib` target; the no-lib
///    fallback must not silently surface them.
///
/// This predicate is used exclusively for LSP member-completion filtering. It
/// does **not** affect property-lookup, subtype checking, or the full apparent
/// shape used for structural compatibility (all of which use
/// `apparent_primitive_shape` or `apparent_primitive_member_kind` directly).
pub fn apparent_primitive_member_is_completion_eligible(kind: IntrinsicKind, name: &str) -> bool {
    match kind {
        IntrinsicKind::String => {
            if is_member(name, STRING_POST_ES2015_MEMBERS) {
                return false;
            }
            name == "length"
                || is_member(name, STRING_METHODS_RETURN_STRING)
                || is_member(name, STRING_METHODS_RETURN_NUMBER)
                || is_member(name, STRING_METHODS_RETURN_BOOLEAN)
                || is_member(name, STRING_METHODS_RETURN_ANY)
                || is_member(name, STRING_METHODS_RETURN_STRING_ARRAY)
        }
        IntrinsicKind::Number => is_member(name, NUMBER_METHODS_RETURN_STRING) || name == "valueOf",
        IntrinsicKind::Boolean => {
            // Boolean's own interface declares only `valueOf()` — toString and
            // toLocaleString are Object.prototype contributions and must not appear.
            name == "valueOf"
        }
        IntrinsicKind::Bigint => is_member(name, BIGINT_METHODS_RETURN_STRING) || name == "valueOf",
        IntrinsicKind::Symbol => name == "description" || name == "toString" || name == "valueOf",
        // Non-primitive kinds (Function, Object, …) are not filtered here —
        // their members all appear in completions as-is.
        _ => true,
    }
}

pub fn apparent_primitive_member_kind(
    interner: &dyn TypeDatabase,
    kind: IntrinsicKind,
    name: &str,
) -> Option<ApparentMemberKind> {
    match kind {
        IntrinsicKind::String => {
            if name == "length" {
                return Some(ApparentMemberKind::Value(TypeId::NUMBER));
            }
            if is_member(name, STRING_METHODS_RETURN_STRING) {
                return Some(ApparentMemberKind::Method(TypeId::STRING));
            }
            if is_member(name, STRING_METHODS_RETURN_NUMBER) {
                return Some(ApparentMemberKind::Method(TypeId::NUMBER));
            }
            if is_member(name, STRING_METHODS_RETURN_BOOLEAN) {
                return Some(ApparentMemberKind::Method(TypeId::BOOLEAN));
            }
            if is_member(name, STRING_METHODS_RETURN_ANY) {
                return Some(ApparentMemberKind::Method(TypeId::ANY));
            }
            if is_member(name, STRING_METHODS_RETURN_STRING_ARRAY) {
                let string_array = interner.array(TypeId::STRING);
                return Some(ApparentMemberKind::Method(string_array));
            }
            object_member_kind(name, true)
        }
        IntrinsicKind::Number => {
            if is_member(name, NUMBER_METHODS_RETURN_STRING) {
                return Some(ApparentMemberKind::Method(TypeId::STRING));
            }
            if name == "valueOf" {
                return Some(ApparentMemberKind::Method(TypeId::NUMBER));
            }
            object_member_kind(name, false)
        }
        IntrinsicKind::Boolean => {
            if is_member(name, BOOLEAN_METHODS_RETURN_STRING) {
                return Some(ApparentMemberKind::Method(TypeId::STRING));
            }
            if name == "valueOf" {
                return Some(ApparentMemberKind::Method(TypeId::BOOLEAN));
            }
            object_member_kind(name, false)
        }
        IntrinsicKind::Bigint => {
            if is_member(name, BIGINT_METHODS_RETURN_STRING) {
                return Some(ApparentMemberKind::Method(TypeId::STRING));
            }
            if name == "valueOf" {
                return Some(ApparentMemberKind::Method(TypeId::BIGINT));
            }
            object_member_kind(name, false)
        }
        IntrinsicKind::Symbol => {
            if name == "description" {
                let description = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
                return Some(ApparentMemberKind::Value(description));
            }
            if name == "toString" {
                return Some(ApparentMemberKind::Method(TypeId::STRING));
            }
            if name == "valueOf" {
                return Some(ApparentMemberKind::Method(TypeId::SYMBOL));
            }
            object_member_kind(name, true)
        }
        IntrinsicKind::Object => object_member_kind(name, true),
        _ => None,
    }
}

pub fn apparent_primitive_members(
    interner: &dyn TypeDatabase,
    kind: IntrinsicKind,
) -> Vec<ApparentMember> {
    let mut members = Vec::new();

    match kind {
        IntrinsicKind::String => {
            members.push(ApparentMember {
                name: "length",
                kind: ApparentMemberKind::Value(TypeId::NUMBER),
            });
            for &name in STRING_METHODS_RETURN_STRING {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::STRING),
                });
            }
            for &name in STRING_METHODS_RETURN_NUMBER {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::NUMBER),
                });
            }
            for &name in STRING_METHODS_RETURN_BOOLEAN {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::BOOLEAN),
                });
            }
            for &name in STRING_METHODS_RETURN_ANY {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::ANY),
                });
            }
            let string_array = interner.array(TypeId::STRING);
            for &name in STRING_METHODS_RETURN_STRING_ARRAY {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(string_array),
                });
            }
            push_object_members(&mut members, true);
        }
        IntrinsicKind::Number => {
            for &name in NUMBER_METHODS_RETURN_STRING {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::STRING),
                });
            }
            members.push(ApparentMember {
                name: "valueOf",
                kind: ApparentMemberKind::Method(TypeId::NUMBER),
            });
            push_object_members(&mut members, false);
        }
        IntrinsicKind::Boolean => {
            for &name in BOOLEAN_METHODS_RETURN_STRING {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::STRING),
                });
            }
            members.push(ApparentMember {
                name: "valueOf",
                kind: ApparentMemberKind::Method(TypeId::BOOLEAN),
            });
            push_object_members(&mut members, false);
        }
        IntrinsicKind::Bigint => {
            for &name in BIGINT_METHODS_RETURN_STRING {
                members.push(ApparentMember {
                    name,
                    kind: ApparentMemberKind::Method(TypeId::STRING),
                });
            }
            members.push(ApparentMember {
                name: "valueOf",
                kind: ApparentMemberKind::Method(TypeId::BIGINT),
            });
            push_object_members(&mut members, false);
        }
        IntrinsicKind::Symbol => {
            let description = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
            members.push(ApparentMember {
                name: "description",
                kind: ApparentMemberKind::Value(description),
            });
            members.push(ApparentMember {
                name: "toString",
                kind: ApparentMemberKind::Method(TypeId::STRING),
            });
            members.push(ApparentMember {
                name: "valueOf",
                kind: ApparentMemberKind::Method(TypeId::SYMBOL),
            });
            push_object_members(&mut members, true);
        }
        IntrinsicKind::Object => {
            push_object_members(&mut members, true);
        }
        _ => {}
    }

    members
}

/// Build an `ObjectShape` for a primitive type's apparent members.
///
/// This is the shared implementation used by both the evaluator and the subtype
/// checker.  The `make_method_type` callback controls how method signatures are
/// created — the evaluator passes `make_apparent_method_type` (which includes a
/// `...any[]` rest param for full evaluation semantics), while the subtype
/// checker may use a simpler shape.
pub fn apparent_primitive_shape(
    db: &dyn TypeDatabase,
    kind: IntrinsicKind,
    make_method_type: impl Fn(&dyn TypeDatabase, TypeId) -> TypeId,
) -> ObjectShape {
    let members = apparent_primitive_members(db, kind);
    let mut properties = Vec::with_capacity(members.len());

    for member in members {
        let name = db.intern_string(member.name);
        match member.kind {
            ApparentMemberKind::Value(type_id) => properties.push(PropertyInfo {
                name,
                type_id,
                write_type: type_id,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            }),
            ApparentMemberKind::Method(return_type) => {
                let method_ty = make_method_type(db, return_type);
                properties.push(PropertyInfo {
                    name,
                    type_id: method_ty,
                    write_type: method_ty,
                    optional: false,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                });
            }
        }
    }
    properties.sort_by_key(|a| a.name);

    // Note: string's implicit number index is semantically readonly (you cannot
    // assign to characters), but we keep readonly=false here because the
    // ReadonlyChecker visitor handles this in is_readonly_index_signature().
    // Setting readonly=true here would change subtype variance (readonly → covariant)
    // which has broader implications for assignability that need separate work.
    let number_index = (kind == IntrinsicKind::String).then_some(IndexSignature {
        key_type: TypeId::NUMBER,
        value_type: TypeId::STRING,
        readonly: false,
        param_name: None,
    });

    ObjectShape {
        flags: ObjectFlags::empty(),
        properties,
        string_index: None,
        number_index,
        symbol: None,
    }
}
