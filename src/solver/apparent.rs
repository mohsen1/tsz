use crate::solver::TypeDatabase;
use crate::solver::types::{IntrinsicKind, TypeId};

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

fn is_member(name: &str, list: &[&str]) -> bool {
    list.iter().any(|&item| item == name)
}

fn object_member_kind(name: &str, include_to_locale: bool) -> Option<ApparentMemberKind> {
    if name == "constructor" {
        return Some(ApparentMemberKind::Value(TypeId::ANY));
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
