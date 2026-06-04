use super::super::super::{Printer, get_operator_text};

use crate::transforms::private_fields_es5::get_private_field_name;

use tsz_parser::parser::{NodeIndex, NodeList, node::Node, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

/// Result of extracting a private field access from a (possibly parenthesized) node.
#[derive(Clone)]
struct PrivateFieldAccess {
    /// The receiver expression node index (e.g., `this` or `A.getInstance()`)
    expression: NodeIndex,
    /// The cleaned field name (without `#`)
    clean_name: String,
    /// The weakmap variable name
    weakmap_name: String,
}

struct PrivateDestructuringTarget {
    target: NodeIndex,
    access: PrivateFieldAccess,
    receiver_temp: Option<String>,
    setter_value: String,
}

enum OptionalChainSegment {
    Property(NodeIndex),
    Element(NodeIndex),
}

include!("private_fields_parts/part1.rs");
include!("private_fields_parts/part2.rs");
