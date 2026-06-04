use crate::state::FileFeatures;

use crate::{
    ContainerKind, FlowNodeId, Symbol, SymbolArena, SymbolId, SymbolTable, flow_flags, symbol_flags,
};

use std::sync::Arc;

use tsz_parser::parser::node::{Node, NodeArena};

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use crate::state::BinderState;

/// Named parameters for `record_semantic_def_ext` and `record_semantic_def_with_declare`.
#[derive(Default)]
pub(crate) struct SemanticDefDetails {
    pub type_param_count: u16,
    pub type_param_names: Vec<String>,
    pub is_exported: bool,
    pub enum_member_names: Vec<String>,
    pub is_const: bool,
    pub is_abstract: bool,
    pub is_declare: bool,
    pub extends_names: Vec<String>,
    pub implements_names: Vec<String>,
}

include!("declaration_parts/part1.rs");
include!("declaration_parts/part2.rs");
include!("declaration_parts/part3.rs");

impl Default for BinderState {
    fn default() -> Self {
        Self::new()
    }
}
