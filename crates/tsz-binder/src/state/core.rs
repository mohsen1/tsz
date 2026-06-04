use super::{BinderState, BinderStateScopeInputs, LibContext};

use crate::lib_loader;

use crate::modules::resolution_debug::ModuleResolutionDebugger;

use crate::{
    ContainerKind, FlowNodeArena, FlowNodeId, Scope, ScopeContext, ScopeId, Symbol, SymbolArena,
    SymbolId, SymbolTable, flow_flags, symbol_flags,
};

use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::Arc;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use super::{BinderOptions, FileFeatures};

/// Returns true if the file extension implies module semantics (.mts, .cts, .mjs, .cjs).
/// In TypeScript, these extensions always indicate module files regardless of content
/// or moduleDetection settings. This matches tsc behavior where .mts files are ES modules
/// and .cts files are CommonJS modules.
fn is_module_file_extension(file_name: &str) -> bool {
    // Check for .mts, .cts (TypeScript module extensions)
    // and .mjs, .cjs (JavaScript module extensions)
    // Also handle declaration variants: .d.mts, .d.cts
    file_name.ends_with(".mts")
        || file_name.ends_with(".cts")
        || file_name.ends_with(".mjs")
        || file_name.ends_with(".cjs")
}

pub(super) fn is_js_like_file_name(file_name: &str) -> bool {
    file_name.ends_with(".js")
        || file_name.ends_with(".jsx")
        || file_name.ends_with(".mjs")
        || file_name.ends_with(".cjs")
}

pub(super) const fn next_persistent_scope_id(scope_count: usize) -> Option<ScopeId> {
    // `ScopeId(u32::MAX)` is reserved as `ScopeId::NONE`, so valid persistent
    // scope IDs are limited to `0..u32::MAX`.
    if scope_count >= u32::MAX as usize {
        return None;
    }
    Some(ScopeId(scope_count as u32))
}

impl BinderStateScopeInputs {
    pub(super) fn with_scopes(
        scopes: Arc<Vec<Scope>>,
        node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
    ) -> Self {
        Self {
            scopes,
            node_scope_ids,
            flow_nodes: Arc::new(FlowNodeArena::new()),
            ..Self::default()
        }
    }
}

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
