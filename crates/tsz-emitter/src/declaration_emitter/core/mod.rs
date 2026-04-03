// This module was split from a single core.rs file. Each submodule contains
// methods on `DeclarationEmitter` grouped by concern.

mod emit_declarations;
mod emit_members;
mod js_emit;
mod setup;

use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::CommentRange;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::node::{MethodDeclData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::type_queries;

/// Declaration emitter for .d.ts files
pub struct DeclarationEmitter<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) writer: SourceWriter,
    pub(super) indent_level: u32,
    pub(super) source_map_text: Option<&'a str>,
    pub(super) source_map_state: Option<SourceMapState>,
    pub(super) pending_source_pos: Option<SourcePosition>,
    /// Whether we're currently emitting a declaration file.
    pub(super) source_is_declaration_file: bool,
    /// Whether the source file being lowered is JavaScript-like (.js/.jsx/.mjs/.cjs).
    pub(super) source_is_js_file: bool,
    /// If true, only emit declarations that are part of the public API surface.
    pub(super) emit_public_api_only: bool,
    /// Track whether we're currently emitting inside a public-API namespace/module.
    pub(super) public_api_scope_depth: u32,
    /// Raw source text for this source file, used for keyword fallback emission.
    pub(super) source_file_text: Option<Arc<str>>,
    /// Type cache for looking up inferred types
    pub(super) type_cache: Option<TypeCacheView>,
    /// Root source-file node for the current emit pass.
    pub(super) current_source_file_idx: Option<NodeIndex>,
    /// Type interner for printing types
    pub(super) type_interner: Option<&'a TypeInterner>,
    /// Binder state for symbol resolution (used by `UsageAnalyzer`)
    pub(super) binder: Option<&'a BinderState>,
    /// Precomputed export surface summary (replaces ad-hoc re-extraction).
    pub(super) export_surface: Option<tsz_binder::ExportSurface>,
    /// Map of symbols to their usage kind (Type, Value, or Both) for import elision
    pub(super) used_symbols:
        Option<FxHashMap<SymbolId, crate::declaration_emitter::usage_analyzer::UsageKind>>,
    /// Set of foreign symbols that need imports (for import generation)
    pub(super) foreign_symbols: Option<FxHashSet<SymbolId>>,
    /// The current file's arena (for distinguishing local vs foreign symbols)
    pub(super) current_arena: Option<Arc<NodeArena>>,
    /// The current file's path (for calculating relative import paths)
    pub(super) current_file_path: Option<String>,
    /// Map of arena address -> file path (for resolving foreign symbol locations)
    pub(super) arena_to_path: FxHashMap<usize, String>,
    /// Map of module → symbol names to auto-generate imports for
    /// Pre-calculated in driver where `MergedProgram` is available
    pub(super) required_imports: FxHashMap<String, Vec<String>>,
    /// Tracks names that are taken in the top-level scope of the file
    /// (includes local declarations and imported names)
    pub(super) reserved_names: FxHashSet<String>,
    /// Maps (`ModulePath`, `ExportName`) -> `AliasName` for string-based imports
    pub(super) import_string_aliases: FxHashMap<(String, String), String>,
    /// Map of imported `SymbolId` -> `ModuleSpecifier` for elision
    /// Tracks which module each imported symbol claims to come from
    pub(super) import_symbol_map: FxHashMap<SymbolId, String>,
    /// Map of imported name -> `SymbolId` for resolving type references
    /// Helps bridge the gap between type references and import symbols
    pub(super) import_name_map: FxHashMap<String, SymbolId>,
    /// Cache of `SymbolId` -> resolved module specifier.
    pub(super) symbol_module_specifier_cache: FxHashMap<SymbolId, Option<String>>,
    /// Precomputed import emission plan for the current file.
    pub(super) import_plan: ImportPlan,
    /// Whether we're inside a declare namespace (don't emit 'declare' keyword inside)
    pub(super) inside_declare_namespace: bool,
    /// Symbol of the innermost enclosing namespace (for context-relative type names)
    pub(super) enclosing_namespace_symbol: Option<SymbolId>,
    /// Whether we're inside a non-ambient namespace (filter non-exported members)
    pub(super) inside_non_ambient_namespace: bool,
    /// Whether we're emitting constructor parameters (don't emit accessibility modifiers)
    pub(super) in_constructor_params: bool,
    /// Track function names that have overload signatures (to skip implementation signatures)
    pub(super) function_names_with_overloads: FxHashSet<String>,
    /// Track whether current class has constructor overloads (to skip implementation constructor)
    pub(super) class_has_constructor_overloads: bool,
    /// Track whether current class extends another class
    pub(super) class_extends_another: bool,
    /// Track method names that have overload signatures in current class (to skip implementation signatures)
    pub(super) method_names_with_overloads: FxHashSet<String>,
    pub(super) all_comments: Vec<CommentRange>,
    pub(super) comment_emit_idx: usize,
    /// When true, strip all comments from .d.ts output (--removeComments)
    pub(super) remove_comments: bool,
    /// When true, strip declarations annotated with `@internal` (--stripInternal)
    pub(super) strip_internal: bool,
    /// Set of absolute file paths whose source contains module augmentations.
    pub(super) files_with_augmentations: FxHashSet<String>,
    /// Tracks whether any non-exported declaration was actually emitted
    /// (used for deciding whether `export {};` scope fix marker is needed)
    pub(super) emitted_non_exported_declaration: bool,
    /// Tracks whether any export statement was emitted that acts as a scope marker
    /// (`ExportDeclaration` with named/namespace exports, `ExportAssignment`, `NamespaceExportDeclaration`)
    pub(super) emitted_scope_marker: bool,
    /// Tracks whether any module indicator was emitted in the output
    /// (exported declarations, imports, scope markers)
    pub(super) emitted_module_indicator: bool,
    /// When true, the current ambient module/namespace body has a mix of
    /// exported and non-exported members, so `export` keywords should be
    /// preserved even though `inside_declare_namespace` is true.
    pub(super) ambient_module_has_scope_marker: bool,
    /// Top-level JS bindings that are re-exported via a foldable `export { x }` clause.
    pub(super) js_named_export_names: FxHashSet<String>,
    /// Foldable JS named export clauses mapped to deferred local statements.
    pub(super) js_folded_named_export_statements: FxHashMap<NodeIndex, Vec<NodeIndex>>,
    /// JS local statements skipped at their original position and re-emitted at
    /// a later `export { ... }` clause to preserve declaration order.
    pub(super) js_deferred_named_export_statements: FxHashSet<NodeIndex>,
    /// Top-level JS bindings referenced by an explicit `export = name` assignment.
    pub(super) js_export_equals_names: FxHashSet<String>,
    /// JS `export = name` assignments already emitted ahead of their declaration.
    pub(super) emitted_js_export_equals_names: FxHashSet<String>,
    /// JS namespace-like alias exports synthesized from expando assignments such
    /// as `foo.default = foo` and `module.exports.Bar = Bar`.
    pub(super) js_namespace_export_aliases: FxHashMap<String, Vec<(String, String)>>,
    /// CJS export aliases for `exports.X = Y` / `module.exports.X = Y`.
    pub(super) js_cjs_export_aliases: Vec<(String, String)>,
    /// Statements consumed by CJS export alias collection.
    pub(super) js_cjs_export_alias_statements: FxHashSet<NodeIndex>,
    /// Statements consumed by `module.exports = { Name1, Name2 }` object pattern.
    pub(super) js_module_exports_object_stmts: FxHashSet<NodeIndex>,
    /// Deferred JS CommonJS `Root.prop = function(){}` statements re-emitted as
    /// top-level synthetic function declarations.
    /// The boolean marks whether the synthetic declaration should be exported.
    pub(super) js_deferred_function_export_statements:
        FxHashMap<NodeIndex, (NodeIndex, NodeIndex, bool)>,
    /// Deferred JS CommonJS `Root.prop = value` statements re-emitted as
    /// top-level synthetic value declarations.
    /// The boolean marks whether the synthetic declaration should be exported.
    pub(super) js_deferred_value_export_statements:
        FxHashMap<NodeIndex, (NodeIndex, NodeIndex, bool)>,
    /// Deferred JS CommonJS `Root.prototype.method = function(){}` statements
    /// re-emitted as a synthetic `declare class Root { method(): ... }`.
    pub(super) js_deferred_prototype_method_statements:
        FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// JS class-like heuristic: `let X; X.prototype.b = ...` → `declare class X { ... }`.
    /// Maps variable name → list of (`member_name_idx`, `initializer_idx`).
    pub(super) js_class_like_prototype_members: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// Expression statements consumed by the class-like prototype heuristic (skipped during emit).
    pub(super) js_class_like_prototype_stmts: FxHashSet<NodeIndex>,
    /// JS `Clazz.method.prop = value` statements re-emitted as merged
    /// `namespace Clazz { function method(); namespace method { ... } }`.
    pub(super) js_static_method_augmentation_statements:
        FxHashMap<NodeIndex, crate::declaration_emitter::helpers::JsStaticMethodAugmentationGroup>,
    /// Extra JS static-method augmentation statements folded into an earlier
    /// synthetic namespace emit.
    pub(super) js_skipped_static_method_augmentation_statements: FxHashSet<NodeIndex>,
    /// Static class method nodes suppressed from class emit because an
    /// augmentation statement re-emits them as namespace members.
    pub(super) js_augmented_static_method_nodes: FxHashSet<NodeIndex>,
    /// Consecutive JS re-export declarations that should be merged at the first statement.
    pub(super) js_grouped_reexports: FxHashMap<NodeIndex, Vec<NodeIndex>>,
    /// JS re-export declarations skipped because they are emitted by an earlier merged group.
    pub(super) js_skipped_reexports: FxHashSet<NodeIndex>,
    /// Synthetic JSDoc type aliases already emitted for the current file.
    pub(super) emitted_jsdoc_type_aliases: FxHashSet<String>,
    /// Local declarations emitted on-demand to support synthetic class base aliases.
    pub(super) emitted_synthetic_dependency_symbols: FxHashSet<SymbolId>,
    /// Diagnostics collected during declaration emit (e.g., TS2883 for non-portable types).
    pub(super) diagnostics: Vec<Diagnostic>,
    /// When true, skip TS2883 non-portable type reference checks.
    /// Set for node16/nodenext module modes where module resolution already
    /// enforces portability via the exports map (TS2307).
    pub(super) skip_portability_check: bool,
    pub(super) strict_null_checks: bool,
    pub(super) isolated_declarations: bool,
    /// Accumulated enum values from all previously-evaluated enums in this file.
    /// Persists across enum declarations so cross-enum references (e.g., `B.Y = A.X`)
    /// can be resolved.
    pub(super) all_enum_values:
        FxHashMap<String, FxHashMap<String, crate::enums::evaluator::EnumValue>>,
}

pub(super) struct SourceMapState {
    pub(super) output_name: String,
    pub(super) source_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedImportSymbol {
    pub(crate) name: String,
    pub(crate) alias: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedImportModule {
    pub(crate) module: String,
    pub(crate) symbols: Vec<PlannedImportSymbol>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportPlan {
    pub(crate) required: Vec<PlannedImportModule>,
    pub(crate) auto_generated: Vec<PlannedImportModule>,
}
