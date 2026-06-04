use rustc_hash::FxHashMap;

use crate::symbols::document_symbols::SymbolKind;

use crate::utils::{find_node_at_offset, identifier_text, node_range};

use tsz_common::position::{Position, Range};

use tsz_parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

/// An item in the call hierarchy (represents a function, method, or constructor).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyItem {
    /// The name of the function/method.
    pub name: String,
    /// The kind of this symbol (Function, Method, Constructor, etc.).
    pub kind: SymbolKind,
    /// The URI of the file containing this symbol.
    pub uri: String,
    /// The range enclosing the entire function/method.
    pub range: Range,
    /// The range of the function/method name (selection range).
    pub selection_range: Range,
    /// Optional containing symbol name (class/module/function).
    pub container_name: Option<String>,
}

/// An incoming call (a caller of the target function).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyIncomingCall {
    /// The calling function/method.
    pub from: CallHierarchyItem,
    /// The ranges within `from` where the target is called.
    pub from_ranges: Vec<Range>,
}

/// An outgoing call (a callee from within the target function).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyOutgoingCall {
    /// The called function/method.
    pub to: CallHierarchyItem,
    /// The ranges within the source function where the callee is invoked.
    pub from_ranges: Vec<Range>,
    /// Set when the callee resolved to an `import` binding rather than a local
    /// declaration. Carries the module specifier text and the imported name so
    /// the LSP server can re-resolve the target in the imported module's
    /// source file (issue #3753).
    #[serde(skip)]
    pub import_resolution: Option<ImportResolutionRequest>,
}

/// Cross-file resolution request emitted by the call-hierarchy provider when
/// it cannot resolve an outgoing callee within the current file because the
/// callee is bound by an import statement.
#[derive(Debug, Clone)]
pub struct ImportResolutionRequest {
    /// The module specifier text as it appears in the source — e.g. `"./a"`,
    /// `"@scope/pkg"`, etc. The LSP server is responsible for resolving this
    /// to an absolute path before re-running the call-hierarchy lookup.
    pub module_specifier: String,
    /// The local name as visible inside the importing file. For default
    /// imports this is the local binding (which tsc treats as the imported
    /// `default` export); for named imports this is the imported name.
    pub local_name: String,
    /// The exported name to look up in the target module. `None` for
    /// namespace imports (`import * as ns`), where call hierarchy treats
    /// the namespace itself as the target.
    pub exported_name: Option<String>,
}

define_lsp_provider!(binder CallHierarchyProvider, "Provider for call hierarchy operations.");

/// Aggregated outgoing-call information keyed by callee declaration. Carries
/// the synthesized hierarchy item, the call-site source ranges, and an
/// optional cross-file resolution request emitted when the callee resolves
/// to an `import` binding (issue #3753).
type OutgoingCalleeEntry = (
    Option<CallHierarchyItem>,
    Vec<Range>,
    Option<ImportResolutionRequest>,
);

include!("call_hierarchy_parts/part1.rs");
include!("call_hierarchy_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/call_hierarchy_tests.rs"]
mod call_hierarchy_tests;
