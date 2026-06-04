use super::super::{ModuleKind, Printer};

use super::core::{CjsExportAssignmentValue, CjsExportVariableSchedule};

use crate::emitter::declarations::class::class_has_self_references;

use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::NodeList;

use tsz_parser::parser::node::Node;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

#[derive(Clone)]
pub(in crate::emitter) struct ObjectRestExportParts {
    non_rest_elements: Vec<NodeIndex>,
    bindings: Vec<ObjectRestExportBinding>,
    rest_name: String,
    excluded_props: Vec<String>,
}

impl ObjectRestExportParts {
    pub(in crate::emitter) const fn needs_source_temp(&self, has_reusable_source: bool) -> bool {
        !self.bindings.is_empty() && !has_reusable_source
    }
}

#[derive(Clone)]
struct ObjectRestExportBinding {
    local_name: String,
    property_name: String,
}

struct DestructuringExportBinding {
    export_name: String,
    access: DestructuringExportAccess,
    leading_comment_pos: u32,
}

enum DestructuringExportAccess {
    Property(String),
    Element(usize),
}

enum EsmObjectRestExportDecl {
    ObjectRest {
        initializer: NodeIndex,
        parts: ObjectRestExportParts,
    },
    Plain(NodeIndex),
}

include!("exports_parts/part1.rs");
include!("exports_parts/part2.rs");
