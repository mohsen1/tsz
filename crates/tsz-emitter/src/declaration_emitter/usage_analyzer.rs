use rustc_hash::{FxHashMap, FxHashSet};

use std::sync::Arc;

use tracing::debug;

use tsz_binder::{BinderState, SymbolId};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::construction::TypeInterner;

use tsz_solver::visitor;

use crate::transforms::emit_utils::string_literal_text;

use crate::type_cache_view::TypeCacheView;

mod ambient_module;

mod public_surface;

mod symbol_references;

mod type_walk;

mod value_references;

pub(super) type SolverTypeId = tsz_solver::TypeId;

#[derive(Debug, Clone, Copy, Default)]
pub struct UsageAnalyzerSourceFlags {
    pub source_is_js_file: bool,
    pub source_is_declaration_file: bool,
}

/// Tracks how a symbol is used - as a type, a value, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageKind {
    bits: u8,
}

impl UsageKind {
    pub const NONE: Self = Self { bits: 0 };
    pub const TYPE: Self = Self { bits: 1 };
    pub const VALUE: Self = Self { bits: 2 };

    #[inline]
    pub const fn is_type(self) -> bool {
        self.bits & Self::TYPE.bits != 0
    }

    #[inline]
    pub const fn is_value(self) -> bool {
        self.bits & Self::VALUE.bits != 0
    }
}

impl std::ops::BitOr for UsageKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

impl std::ops::BitOrAssign for UsageKind {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

/// Usage analyzer for determining which symbols are referenced in exported declarations.
pub struct UsageAnalyzer<'a> {
    /// AST arena for walking explicit type annotations
    arena: &'a NodeArena,
    /// Binder state for symbol resolution (`node_symbols`)
    binder: &'a BinderState,
    /// Type cache view for inferred types and `def_to_symbol` mapping
    type_cache: &'a TypeCacheView,
    /// Type interner for type operations
    type_interner: &'a TypeInterner,
    /// Map of import name -> `SymbolId` for resolving type references
    import_name_map: &'a FxHashMap<String, SymbolId>,
    /// Map of symbols to their usage kind (Type, Value, or Both)
    used_symbols: FxHashMap<SymbolId, UsageKind>,
    /// Visited AST nodes (for cycle detection)
    visited_nodes: FxHashSet<NodeIndex>,
    /// Visited `TypeIds` (for cycle detection)
    visited_types: FxHashSet<tsz_solver::TypeId>,
    /// Memoized transitive symbol usages per `TypeId`.
    type_symbol_cache: FxHashMap<tsz_solver::TypeId, Arc<[(SymbolId, UsageKind)]>>,
    /// `TypeIds` currently being memoized (cycle guard).
    memoizing_types: FxHashSet<tsz_solver::TypeId>,
    /// The current file's arena (for distinguishing local vs foreign symbols)
    current_arena: Arc<NodeArena>,
    /// Current source file path, used to resolve relative import aliases.
    current_file_path: Option<String>,
    /// Whether the current source file is JavaScript.
    source_is_js_file: bool,
    /// Whether the current source file is already a declaration file.
    source_is_declaration_file: bool,
    /// Set of symbols from other modules that need imports
    foreign_symbols: FxHashSet<SymbolId>,
    /// Context flag: true when we're in a value position (expression, typeof)
    in_value_pos: bool,
    /// String-literal ambient module currently being analyzed, if any.
    current_ambient_module_specifier: Option<String>,
}

include!("usage_analyzer_parts/part1.rs");
include!("usage_analyzer_parts/part2.rs");
