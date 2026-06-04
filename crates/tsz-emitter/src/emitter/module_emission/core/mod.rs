mod anonymous_default;

mod export_default_parens;

use super::super::{JsxEmit, ModuleKind, Printer, ScriptTarget};

use crate::context::transform::IdentifierId;

use crate::transforms::emit_utils;

use crate::transforms::private_fields_es5::is_private_identifier;

use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};

use tsz_parser::parser::node::{Node, NodeAccess};

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

#[derive(Default)]
pub(in crate::emitter) struct CjsExportVariableSchedule {
    pub local_groups: Vec<CjsExportLocalDeclGroup>,
    pub assignments: Vec<CjsExportAssignment>,
}

pub(in crate::emitter) struct CjsExportLocalDeclGroup {
    pub keyword: &'static str,
    pub declarations: Vec<NodeIndex>,
}

pub(in crate::emitter) struct CjsExportAssignment {
    pub decoded_name: String,
    pub emit_name: String,
    pub value: CjsExportAssignmentValue,
}

pub(in crate::emitter) enum CjsExportAssignmentValue {
    Initializer(NodeIndex),
    LocalName(String),
}

const fn cjs_export_decl_list_keyword(node: &Node, target_es5: bool) -> Option<&'static str> {
    let flags = node.flags as u32;
    if flags & node_flags::USING != 0 {
        None
    } else if target_es5 {
        Some("var")
    } else if flags & node_flags::CONST != 0 {
        Some("const")
    } else if flags & node_flags::LET != 0 {
        Some("let")
    } else {
        Some("var")
    }
}

include!("mod_parts/part1.rs");
include!("mod_parts/part2.rs");

#[cfg(test)]
mod tests;
