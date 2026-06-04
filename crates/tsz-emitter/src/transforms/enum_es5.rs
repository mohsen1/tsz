use std::collections::{HashMap, HashSet};

use crate::transforms::emit_utils::is_valid_identifier_name;

use crate::transforms::ir::{IRNode, IRParam};

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

#[path = "enum_es5_emitter.rs"]
mod enum_es5_emitter;

pub use enum_es5_emitter::EnumES5Emitter;

/// Enum ES5 transformer - produces IR for enum declarations
pub struct EnumES5Transformer<'a> {
    arena: &'a NodeArena,
    /// Track last numeric value for auto-incrementing (integer path)
    last_value: Option<i64>,
    /// Track last float value for auto-incrementing (float path, e.g., 0.1 → 1.1)
    last_float_value: Option<f64>,
    /// Source text for extracting raw expressions
    source_text: Option<&'a str>,
    /// Names of all enum members declared so far (for qualifying self-references)
    member_names: HashSet<String>,
    /// Names of all members in same-name enum declarations in the current source file.
    /// This lets forward-reference detection see later merged enum blocks.
    merged_member_names: HashSet<String>,
    /// Names of enum members that have been processed (had their IR emitted).
    /// Used to distinguish forward references (not yet processed → resolve to 0)
    /// from self-references and back-references (already processed → keep expression).
    processed_members: HashSet<String>,
    /// The name of the member currently being processed (for detecting self-references)
    current_member_name: String,
    /// Names of enum members with string-valued initializers (no reverse mapping)
    string_members: HashSet<String>,
    /// Evaluated numeric values of enum members (for constant folding in subsequent member initializers)
    member_values: HashMap<String, i64>,
    /// Evaluated string values of enum members (for constant folding in string concatenation)
    string_member_values: HashMap<String, String>,
    /// Source file containing the enum currently being transformed.
    /// Used to resolve top-level `const` initializers in enum constant expressions.
    current_source_file: Option<NodeIndex>,
    /// The enum parameter name used inside the IIFE (for qualifying self-references)
    current_enum_name: String,
    /// When true, emit const enums instead of erasing them
    preserve_const_enums: bool,
    /// Previously-evaluated enum member values from other enums.
    /// Keyed by `enum_name` → `member_name` → value.
    prior_enum_values: HashMap<String, HashMap<String, i64>>,
    /// Previously-evaluated string enum member names from other enums.
    /// Keyed by `enum_name` → set of member names that have string values.
    prior_string_members: HashMap<String, HashSet<String>>,
    /// Previously-evaluated string enum member values from other enums.
    /// Keyed by `enum_name` → `member_name` → value.
    prior_string_values: HashMap<String, HashMap<String, String>>,
    /// Whether this enum should emit its own `var E;` declaration.
    emit_var_declaration: bool,
    /// Whether the emit target downlevels block-scoped declarations to `var`
    /// (ES5/ES3). Only then does a block-scoped enum's hoisted binding need the
    /// `= void 0` reset; at ES2015+ the caller upgrades the binding to `let`,
    /// which is properly block-scoped and needs no reset.
    target_es5: bool,
    /// Structured module export fold for the enum IIFE tail.
    export_fold: Option<EnumExportFold>,
}

#[derive(Clone, Debug)]
enum EnumExportFold {
    /// Source-ordered list of CJS export aliases for the enum's local name.
    /// The emitter chains them so the local-name assignment is right-most:
    /// `["E", "EE"]` produces `(E || (exports.EE = exports.E = E = {}))`.
    CommonJs {
        export_names: Vec<String>,
    },
    System {
        export_names: Vec<String>,
    },
}

fn commonjs_export_access(export_name: &str) -> IRNode {
    let exports = IRNode::Identifier("exports".into());
    if is_valid_identifier_name(export_name) {
        IRNode::PropertyAccess {
            object: Box::new(exports),
            property: export_name.to_string().into(),
        }
    } else {
        IRNode::ElementAccess {
            object: Box::new(exports),
            index: Box::new(IRNode::StringLiteral(export_name.to_string().into())),
        }
    }
}

include!("enum_es5_parts/part1.rs");
include!("enum_es5_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/enum_es5.rs"]
mod tests;
