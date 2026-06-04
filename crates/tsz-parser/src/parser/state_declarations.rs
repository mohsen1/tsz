use super::state::ParserState;

use crate::parser::{
    NodeIndex, NodeList,
    node::{EnumData, EnumMemberData, IdentifierData, ParameterData},
    syntax_kind_ext,
};

use tsz_common::interner::Atom;

use tsz_scanner::SyntaxKind;

fn is_reserved_interface_type_name(name: &str) -> bool {
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

enum TypeMemberPropertyOrMethodName {
    Property(NodeIndex),
    IndexSignature(NodeIndex),
}

include!("state_declarations_parts/part1.rs");
include!("state_declarations_parts/part2.rs");
