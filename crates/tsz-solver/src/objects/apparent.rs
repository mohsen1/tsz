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

const STRING_METHODS_RETURN_STRING: &[&str] = &[
    "anchor",
    "at",
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
