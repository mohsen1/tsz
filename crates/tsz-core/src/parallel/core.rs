//! Parallel Processing Module
//!
//! Provides parallel file parsing and processing using Rayon.
//! This enables significant speedups on multi-core machines.
//!
//! # Architecture
//!
//! The compilation pipeline has these parallelization opportunities:
//!
//! 1. **Parsing** - Each file can be parsed independently (embarrassingly parallel)
//! 2. **Binding** - After parsing, binding can be parallelized per-file
//! 3. **Type Checking** - Function bodies can be checked in parallel
//!    (once global symbols are merged)
//!
//! # Usage
//!
//! ```text
//! use tsz::parallel::parse_files_parallel;
//!
//! let files = vec![
//!     ("src/a.ts".to_string(), "let a = 1;".to_string()),
//!     ("src/b.ts".to_string(), "let b = 2;".to_string()),
//! ];
//!
//! let results = parse_files_parallel(files);
//! // results is Vec<ParseResult> with parsed ASTs
//! ```

use crate::binder::BinderOptions;
use crate::binder::BinderState;
use crate::binder::state::{
    BinderStateScopeInputs, CrossFileNodeSymbols, DeclarationArenaMap, SymToDeclIndicesMap,
    WildcardReexportsMap, WildcardReexportsTypeOnlyMap,
};
use crate::binder::{
    FlowNodeArena, FlowNodeId, Scope, ScopeId, SymbolArena, SymbolId, SymbolTable,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::config::resolve_default_lib_files;
use crate::emitter::ScriptTarget;
use crate::lib_loader;
use crate::parser::NodeIndex;
use crate::parser::NodeList;
use crate::parser::node::{NodeArena, SourceFileData};
use crate::parser::{ParseDiagnostic, ParserState};
use anyhow::{Context, Result, bail};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Once;
use tsz_common::interner::{Atom, Interner};
use tsz_scanner::SyntaxKind;

type ModuleExportEntry = FxHashMap<String, (String, Option<String>)>;
type Reexports = FxHashMap<String, ModuleExportEntry>;

/// Validate JSON syntax and return parse diagnostics for violations.
///
/// TypeScript's JSON parser enforces strict JSON rules when parsing `.json` files.
/// This validates property names must be double-quoted string literals (TS1327).
/// Violations include single-quoted strings, computed property names (`[expr]`),
/// and unquoted identifiers used as property names.
fn validate_json_syntax(source: &str) -> Vec<ParseDiagnostic> {
    let mut diagnostics = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let is_ws = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r');
    let is_ident_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let is_ident_part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';

    // Root-level recovery for invalid bare identifier runs in JSON files.
    // tsc emits a specific sequence:
    //   first identifier  -> TS1005 "'{' expected." + TS1136
    //   next identifiers  -> TS1005 "',' expected." + TS1136
    //   end of run        -> TS1005 "'}' expected."
    //
    // Valid JSON roots `true` / `false` / `null` are explicitly allowed.
    let mut j = 0usize;
    while j < len && is_ws(bytes[j]) {
        j += 1;
    }
    if j < len && is_ident_start(bytes[j]) {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut k = j;
        loop {
            let start = k;
            while k < len && is_ident_part(bytes[k]) {
                k += 1;
            }
            spans.push((start, k));

            while k < len && is_ws(bytes[k]) {
                k += 1;
            }

            if k < len && is_ident_start(bytes[k]) {
                continue;
            }
            break;
        }

        if k >= len {
            let single_keyword_root = spans.len() == 1
                && std::str::from_utf8(&bytes[spans[0].0..spans[0].1])
                    .map(|s| matches!(s, "true" | "false" | "null"))
                    .unwrap_or(false);

            if !single_keyword_root {
                for (idx, (start, _end)) in spans.iter().enumerate() {
                    let expected_msg = if idx == 0 {
                        "'{' expected."
                    } else {
                        "',' expected."
                    };
                    diagnostics.push(ParseDiagnostic {
                        start: *start as u32,
                        length: 1,
                        message: expected_msg.to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                    });
                    diagnostics.push(ParseDiagnostic {
                        start: *start as u32,
                        length: 1,
                        message: tsz_common::diagnostics::diagnostic_messages::PROPERTY_ASSIGNMENT_EXPECTED.to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                    });
                }
                if let Some((_, end)) = spans.last() {
                    diagnostics.push(ParseDiagnostic {
                        start: *end as u32,
                        length: 1,
                        message: "'}' expected.".to_string(),
                        code: tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                    });
                }
            }
        }
    }

    // Track whether we're inside an object and expecting a property name.
    // JSON property names must be double-quoted strings per the JSON spec.
    // We use a simple state machine: after `{` or `,` inside an object,
    // the next non-whitespace token must be `"` (property name) or `}` (end).
    let mut object_depth: i32 = 0;
    let mut array_depth: i32 = 0;
    let mut expecting_property_name = false;

    while i < len {
        let b = bytes[i];

        // Skip whitespace
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            i += 1;
            continue;
        }

        if b == b'{' {
            object_depth += 1;
            expecting_property_name = true;
            i += 1;
            continue;
        }

        if b == b'}' {
            object_depth -= 1;
            expecting_property_name = false;
            i += 1;
            continue;
        }

        if b == b'[' && !expecting_property_name {
            array_depth += 1;
            i += 1;
            continue;
        }

        if b == b']' && array_depth > 0 {
            array_depth -= 1;
            i += 1;
            continue;
        }

        if b == b',' {
            // After a comma inside an object (not array), expect a property name
            if object_depth > array_depth {
                expecting_property_name = true;
            }
            i += 1;
            continue;
        }

        if b == b':' {
            expecting_property_name = false;
            i += 1;
            continue;
        }

        // When expecting a property name, check what we got
        if expecting_property_name && object_depth > 0 {
            if b == b'"' {
                // Valid double-quoted property name - skip past the string
                expecting_property_name = false;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escape sequence
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }

            // Not a double-quoted string in property name position → TS1327
            diagnostics.push(ParseDiagnostic {
                start: i as u32,
                length: 1,
                message: tsz_common::diagnostics::diagnostic_messages::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED.to_string(),
                code: tsz_common::diagnostics::diagnostic_codes::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
            });
            expecting_property_name = false;
        }

        // Skip over strings (both single and double quoted) to avoid false matches
        if b == b'"' || b == b'\'' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }

        i += 1;
    }

    diagnostics
}

fn synthesize_json_bind_result(file_name: String, source_text: String) -> BindResult {
    let parse_diagnostics = validate_json_syntax(&source_text);

    let mut arena = NodeArena::new();
    let end_pos = source_text.len() as u32;
    let eof_token = arena.add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);
    let source_file = arena.add_source_file(
        0,
        end_pos,
        SourceFileData {
            statements: NodeList::default(),
            end_of_file_token: eof_token,
            file_name: file_name.clone(),
            text: Arc::<str>::from(source_text),
            language_version: 99,
            language_variant: 0,
            script_kind: 3,
            is_declaration_file: false,
            has_no_default_lib: false,
            comments: Vec::new(),
            parent: NodeIndex::NONE,
            id: 0,
            modifier_flags: 0,
            transform_flags: 0,
        },
    );

    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);
    binder.bind_source_file(&arena, source_file);

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders: Arc::new(Vec::new()),
        lib_arenas: Vec::new(),
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
        file_import_sources: binder.file_import_sources,
    }
}

#[cfg(target_arch = "wasm32")]
fn resolve_default_lib_files(_target: ScriptTarget) -> anyhow::Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
static RAYON_POOL_INIT: Once = Once::new();

/// Ensure Rayon global pool is configured once with stack size suitable for checker recursion.
///
/// We initialize lazily to avoid paying global pool startup cost for single-file sequential paths.
///
/// Worker threads get the same 64 MB stack as the main CLI thread. Type-level
/// libraries (ts-toolbelt, ts-essentials) can produce deeply nested conditional/
/// mapped type evaluation chains that easily exceed 8 MB even with logical
/// recursion guards, because every `evaluate -> evaluate_application ->
/// instantiate -> evaluate` cycle still consumes real stack frames.
#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_rayon_global_pool() {
    RAYON_POOL_INIT.call_once(|| {
        // If the pool was already initialized through another rayon call, keep going.
        let _ = rayon::ThreadPoolBuilder::new()
            .stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES)
            .build_global();
    });
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_rayon_global_pool() {}

/// Conditionally use parallel or sequential iteration based on target.
/// For WASM, Rayon parallelism creates oversubscription when combined with
/// external worker-level parallelism (e.g., Node worker threads in conformance tests).
/// This causes worker crashes and OOM issues.
///
/// Usage:
/// - `maybe_parallel_iter!(collection)` for `.par_iter()` / `.iter()`
/// - `maybe_parallel_into!(collection)` for `.into_par_iter()` / `.into_iter()`
#[cfg(target_arch = "wasm32")]
macro_rules! maybe_parallel_iter {
    ($iter:expr) => {
        $iter.iter()
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! maybe_parallel_iter {
    ($iter:expr) => {
        $iter.par_iter()
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! maybe_parallel_into {
    ($iter:expr) => {
        $iter.into_iter()
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! maybe_parallel_into {
    ($iter:expr) => {
        $iter.into_par_iter()
    };
}

/// Result of parsing a single file
pub struct ParseResult {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes
    pub arena: NodeArena,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}

/// Parse multiple files in parallel using Parser
///
/// Each file is parsed independently, producing its own arena.
/// This is optimal for initial parsing before symbol resolution.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
///
/// # Returns
/// Vector of `ParseResult` for each file
pub fn parse_files_parallel(files: Vec<(String, String)>) -> Vec<ParseResult> {
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            // Consume the parser and take its arena/diagnostics
            let (arena, parse_diagnostics) = parser.into_parts();

            ParseResult {
                file_name,
                source_file,
                arena,
                parse_diagnostics,
            }
        })
        .collect()
}

/// Parse a single file (for comparison/testing)
pub fn parse_file_single(file_name: String, source_text: String) -> ParseResult {
    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    // Consume the parser and take its arena/diagnostics
    let (arena, parse_diagnostics) = parser.into_parts();

    ParseResult {
        file_name,
        source_file,
        arena,
        parse_diagnostics,
    }
}

/// Statistics about parallel parsing performance
#[derive(Debug, Clone)]
pub struct ParallelStats {
    /// Number of files parsed
    pub file_count: usize,
    /// Total source bytes
    pub total_bytes: usize,
    /// Total nodes created
    pub total_nodes: usize,
    /// Number of parse errors
    pub error_count: usize,
}

// =============================================================================
// Parallel Binding
// =============================================================================

/// Result of binding a single file
pub struct BindResult {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes
    pub arena: Arc<NodeArena>,
    /// Symbols created in this file
    pub symbols: SymbolArena,
    /// File-level symbol table (exports, declarations)
    pub file_locals: SymbolTable,
    /// Ambient module declarations by specifier
    pub declared_modules: FxHashSet<String>,
    /// Module exports keyed by specifier or file name
    pub module_exports: Arc<FxHashMap<String, SymbolTable>>,
    /// Node-to-symbol mapping.
    ///
    /// Shared via `Arc` because the binder owns it as `Arc<FxHashMap<...>>`
    /// to avoid deep clones when reconstructing per-file binders for the
    /// cross-file lookup pipeline. The Arc is moved out of the binder when
    /// finalizing the bind result. See PR #1202 for the equivalent fix to
    /// `semantic_defs`; this is the same template applied to `node_symbols`.
    pub node_symbols: Arc<FxHashMap<u32, SymbolId>>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: Arc<FxHashMap<u32, bool>>,
    /// Symbol-to-arena mapping for cross-file declaration lookup (including lib symbols)
    pub symbol_arenas: Arc<FxHashMap<SymbolId, Arc<NodeArena>>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup.
    /// `Arc`-wrapped end-to-end so merging the per-file `BinderState.declaration_arenas`
    /// (also `Arc`) into the final `MergedProgram` does not require deep cloning.
    pub declaration_arenas: Arc<DeclarationArenaMap>,
    /// Persistent scopes for stateless checking.
    ///
    /// `Arc`-wrapped to mirror `BinderState.scopes` (same field) so per-file
    /// binders constructed by the CLI driver share via `Arc::clone` instead
    /// of deep-cloning the underlying `Vec<Scope>`. Each `Scope` already
    /// holds an `Arc<FxHashMap>` symbol table internally (PR #1535) so even
    /// if `Arc::make_mut` ever has to copy-on-write, the per-`Scope` clone
    /// stays cheap.
    pub scopes: Arc<Vec<Scope>>,
    /// Map from AST node to scope ID.
    ///
    /// `Arc`-wrapped to mirror `BinderState.node_scope_ids` so per-file
    /// binders share via `Arc::clone` instead of deep-cloning. Read-only
    /// after binding completes.
    pub node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Shorthand ambient modules (`declare module "foo"` without body)
    pub shorthand_ambient_modules: Arc<FxHashSet<String>>,
    /// Global augmentations (interface declarations inside `declare global` blocks)
    pub global_augmentations: Arc<FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>>,
    /// Module augmentations (interface/type declarations inside `declare module 'x'` blocks)
    /// Maps module specifier -> [`ModuleAugmentation`]
    pub module_augmentations: Arc<FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>>,
    /// Maps symbols declared inside module augmentation blocks to their target module specifier
    pub augmentation_target_modules: Arc<FxHashMap<SymbolId, String>>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    pub reexports: Arc<Reexports>,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// `Arc`-wrapped to mirror `BinderState.wildcard_reexports`; the
    /// final `MergedProgram` builds its own `Arc` once and shares it
    /// with every per-file `BinderState` via `Arc::clone`.
    pub wildcard_reexports: Arc<WildcardReexportsMap>,
    /// Wildcard re-export type-only provenance aligned with `wildcard_reexports`.
    pub wildcard_reexports_type_only: Arc<WildcardReexportsTypeOnlyMap>,
    /// Lib binders for global type resolution (Array, String, etc.)
    /// These are merged from lib.d.ts files and enable cross-file symbol lookup
    pub lib_binders: Arc<Vec<Arc<BinderState>>>,
    /// Arenas corresponding to each `lib_binder` (same order/length as `lib_binders`).
    /// Used by `merge_bind_results_ref` to populate `declaration_arenas` for lib symbols.
    pub lib_arenas: Vec<Arc<NodeArena>>,
    /// Symbol IDs that originated from lib files (pre-merge local IDs).
    /// `Arc`-wrapped so the merge can move it into the per-file
    /// `BinderState.lib_symbol_ids` (also `Arc`) without deep-cloning.
    pub lib_symbol_ids: Arc<FxHashSet<SymbolId>>,
    /// Reverse mapping from user-local lib symbol IDs to (`lib_binder_ptr`, `original_local_id`).
    ///
    /// `Arc`-wrapped to mirror `BinderState.lib_symbol_reverse_remap` so the
    /// final `MergedProgram`/`BoundFile` can move it into per-file binders
    /// via `Arc::clone` instead of deep-cloning. Mutated only during
    /// `merge_lib_contexts_into_binder` (refcount=1 → free).
    pub lib_symbol_reverse_remap: Arc<FxHashMap<SymbolId, (usize, SymbolId)>>,
    /// Flow nodes for control flow analysis.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver
    /// can share the underlying `FlowNodeArena` via `Arc::clone` (atomic
    /// increment) instead of deep-cloning `Vec<FlowNode>` — each
    /// `FlowNode` owns a `Vec<FlowNodeId>` antecedents, so the deep clone
    /// was allocation-heavy on large projects. Mutations during binding
    /// go through `Arc::make_mut` (free when refcount=1, the case during
    /// a single file's binding).
    pub flow_nodes: Arc<FlowNodeArena>,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node.
    ///
    /// Shared via `Arc` because the binder owns it as `Arc<FxHashMap<...>>`
    /// to avoid deep clones when reconstructing per-file binders for the
    /// cross-file lookup pipeline. The Arc is moved out of the binder when
    /// finalizing the bind result. See PR #1202 (`semantic_defs`) and
    /// PR #1227 (`node_symbols`); this is the same template applied to
    /// `node_flow`.
    pub node_flow: Arc<FxHashMap<u32, FlowNodeId>>,
    /// Map from switch clause `NodeIndex` to parent switch statement `NodeIndex`
    /// Used by control flow analysis for switch exhaustiveness checking.
    ///
    /// `Arc`-wrapped so per-file binders share via `Arc::clone` instead of
    /// deep-cloning. Read-only after binding completes.
    pub switch_clause_to_switch: Arc<FxHashMap<u32, NodeIndex>>,
    /// Whether this file is an external module (has imports/exports)
    pub is_external_module: bool,
    /// Expando property assignments detected during binding.
    ///
    /// `Arc`-wrapped to mirror `BinderState.expando_properties` so the
    /// merge can move it into per-file binders without deep-cloning.
    pub expando_properties: Arc<FxHashMap<String, FxHashSet<String>>>,
    /// Per-file alias partners from binder (`TYPE_ALIAS` → `ALIAS` mapping, pre-remap)
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,
    pub file_features: crate::binder::FileFeatures,
    /// Binder-captured semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Maps pre-remap `SymbolId` → `SemanticDefEntry`.
    ///
    /// Shared via `Arc` because the binder owns it as `Arc<FxHashMap<...>>` to
    /// avoid deep clones in cross-file binder reconstruction. The Arc is moved
    /// out of the binder when finalizing the bind result.
    pub semantic_defs: Arc<FxHashMap<SymbolId, crate::binder::SemanticDefEntry>>,
    /// Static import/export-from module specifiers collected during binding.
    pub file_import_sources: Vec<String>,
}

impl BindResult {
    /// Estimate the heap memory footprint of this bind result in bytes.
    ///
    /// Accounts for the struct itself plus all heap-allocated strings, vecs,
    /// hash map entries, and arena contents. Used for memory pressure tracking
    /// and eviction decisions in the LSP.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // file_name
        size += self.file_name.capacity();

        // arena (NodeArena behind Arc — count the Arc overhead, not the shared data)
        size += std::mem::size_of::<NodeArena>();

        // symbols (SymbolArena: Vec<Symbol> + name_index)
        size += self.symbols.len() * std::mem::size_of::<crate::binder::Symbol>();
        for sym in self.symbols.iter() {
            size += sym.escaped_name.capacity();
            size += sym.declarations.capacity() * std::mem::size_of::<NodeIndex>();
            if let Some(ref exports) = sym.exports {
                size += std::mem::size_of::<SymbolTable>();
                size += exports.len() * (32 + std::mem::size_of::<SymbolId>());
            }
            if let Some(ref members) = sym.members {
                size += std::mem::size_of::<SymbolTable>();
                size += members.len() * (32 + std::mem::size_of::<SymbolId>());
            }
            if let Some(ref s) = sym.import_module {
                size += s.capacity();
            }
            if let Some(ref s) = sym.import_name {
                size += s.capacity();
            }
        }

        // file_locals (SymbolTable)
        size += self.file_locals.len() * (32 + std::mem::size_of::<SymbolId>());

        // declared_modules
        for s in &self.declared_modules {
            size += s.capacity() + std::mem::size_of::<u64>();
        }

        // module_exports
        for (k, v) in self.module_exports.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += std::mem::size_of::<SymbolTable>();
            size += v.len() * (32 + std::mem::size_of::<SymbolId>());
        }

        // node_symbols
        size += self.node_symbols.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<SymbolId>() + 8);

        // module_declaration_exports_publicly
        size += self.module_declaration_exports_publicly.capacity()
            * (std::mem::size_of::<u32>() + 1 + 8);

        // symbol_arenas (map overhead; shared Arc data not counted)
        size += self.symbol_arenas.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<Arc<NodeArena>>() + 8);

        // declaration_arenas
        size +=
            self.declaration_arenas.len() * (std::mem::size_of::<(SymbolId, NodeIndex)>() + 32 + 8);

        // scopes
        size += self.scopes.capacity() * std::mem::size_of::<Scope>();
        for scope in self.scopes.iter() {
            size += scope.table.len() * (32 + std::mem::size_of::<SymbolId>());
        }

        // node_scope_ids
        size += self.node_scope_ids.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<ScopeId>() + 8);

        // parse_diagnostics
        size += self.parse_diagnostics.capacity() * std::mem::size_of::<ParseDiagnostic>();
        for diag in &self.parse_diagnostics {
            size += diag.message.capacity();
        }

        // shorthand_ambient_modules
        for s in self.shorthand_ambient_modules.iter() {
            size += s.capacity() + std::mem::size_of::<u64>();
        }

        // global_augmentations
        for (k, v) in self.global_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::GlobalAugmentation>();
        }

        // module_augmentations
        for (k, v) in self.module_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::ModuleAugmentation>();
            for aug in v {
                size += aug.name.capacity();
            }
        }

        // augmentation_target_modules
        for v in self.augmentation_target_modules.values() {
            size += std::mem::size_of::<SymbolId>() + v.capacity() + 8;
        }

        // reexports (FxHashMap<String, FxHashMap<String, (String, Option<String>)>>)
        for (k, inner) in self.reexports.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for (ik, (s1, s2)) in inner {
                size += ik.capacity() + s1.capacity() + 8;
                if let Some(s) = s2 {
                    size += s.capacity();
                }
            }
        }

        // wildcard_reexports
        for (k, v) in self.wildcard_reexports.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for s in v {
                size += s.capacity();
            }
        }

        // wildcard_reexports_type_only
        for (k, v) in self.wildcard_reexports_type_only.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for (s, _) in v {
                size += s.capacity() + 1;
            }
        }

        // lib_binders (Arc overhead only)
        size += self.lib_binders.capacity() * std::mem::size_of::<Arc<BinderState>>();

        // lib_arenas (Arc overhead only)
        size += self.lib_arenas.capacity() * std::mem::size_of::<Arc<NodeArena>>();

        // lib_symbol_ids
        size += self.lib_symbol_ids.len() * (std::mem::size_of::<SymbolId>() + 8);

        // lib_symbol_reverse_remap
        size += self.lib_symbol_reverse_remap.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<(usize, SymbolId)>() + 8);

        // flow_nodes
        size += self.flow_nodes.len() * std::mem::size_of::<crate::binder::FlowNode>();
        for flow_node in self.flow_nodes.iter() {
            size += flow_node.antecedent.capacity() * std::mem::size_of::<FlowNodeId>();
        }

        // node_flow
        size += self.node_flow.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<FlowNodeId>() + 8);

        // switch_clause_to_switch
        size += self.switch_clause_to_switch.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<NodeIndex>() + 8);

        // expando_properties
        for (k, v) in self.expando_properties.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for s in v {
                size += s.capacity() + std::mem::size_of::<u64>();
            }
        }

        // alias_partners
        size += self.alias_partners.capacity() * (std::mem::size_of::<SymbolId>() * 2 + 8);

        // semantic_defs
        for def in self.semantic_defs.values() {
            size += std::mem::size_of::<SymbolId>()
                + std::mem::size_of::<crate::binder::SemanticDefEntry>()
                + 8;
            size += def.name.capacity();
            for h in &def.extends_names {
                size += h.capacity();
            }
            for h in &def.implements_names {
                size += h.capacity();
            }
        }

        // file_import_sources
        size += self.file_import_sources.capacity() * std::mem::size_of::<String>();
        for s in &self.file_import_sources {
            size += s.capacity();
        }

        size
    }
}

/// Parse and bind multiple files in parallel
///
/// Each file is parsed and bound independently. The binding creates
/// file-local symbols which can later be merged into a global scope.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel(files: Vec<(String, String)>) -> Vec<BindResult> {
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            // Skip parsing .json files - they should not be parsed as TypeScript.
            // JSON module imports should be resolved during module resolution and
            // emit TS2732 if resolveJsonModule is false.
            if file_name.ends_with(".json") {
                return synthesize_json_bind_result(file_name, source_text);
            }

            // Parse
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            let (arena, parse_diagnostics) = parser.into_parts();

            // Bind
            let mut binder = BinderState::new();
            binder.set_debug_file(&file_name);
            binder.bind_source_file(&arena, source_file);

            BindResult {
                file_name,
                source_file,
                arena: Arc::new(arena),
                symbols: binder.symbols,
                file_locals: binder.file_locals,
                declared_modules: binder.declared_modules,
                module_exports: binder.module_exports,
                node_symbols: binder.node_symbols,
                module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
                symbol_arenas: binder.symbol_arenas,
                declaration_arenas: binder.declaration_arenas,
                scopes: binder.scopes,
                node_scope_ids: binder.node_scope_ids,
                parse_diagnostics,
                shorthand_ambient_modules: binder.shorthand_ambient_modules,
                global_augmentations: binder.global_augmentations,
                module_augmentations: binder.module_augmentations,
                augmentation_target_modules: binder.augmentation_target_modules,
                reexports: binder.reexports,
                wildcard_reexports: binder.wildcard_reexports,
                wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
                lib_binders: Arc::new(Vec::new()), // No libs in this path
                lib_arenas: Vec::new(),
                lib_symbol_ids: binder.lib_symbol_ids,
                lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
                flow_nodes: binder.flow_nodes,
                node_flow: binder.node_flow,
                switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
                is_external_module: binder.is_external_module,
                expando_properties: std::mem::take(&mut binder.expando_properties),
                alias_partners: binder.alias_partners,
                file_features: binder.file_features,
                semantic_defs: binder.semantic_defs,
                file_import_sources: binder.file_import_sources,
            }
        })
        .collect()
}

/// Bind a single file (for comparison/testing)
pub fn parse_and_bind_single(file_name: String, source_text: String) -> BindResult {
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);
    binder.bind_source_file(&arena, source_file);

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders: Arc::new(Vec::new()), // No libs in this path
        lib_arenas: Vec::new(),
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
        file_import_sources: binder.file_import_sources,
    }
}

/// Statistics about parallel binding performance
#[derive(Debug, Clone)]
pub struct BindStats {
    /// Number of files bound
    pub file_count: usize,
    /// Total nodes across all files
    pub total_nodes: usize,
    /// Total symbols created
    pub total_symbols: usize,
    /// Number of parse errors
    pub parse_error_count: usize,
}

/// Parse and bind files with statistics
pub fn parse_and_bind_with_stats(files: Vec<(String, String)>) -> (Vec<BindResult>, BindStats) {
    let file_count = files.len();
    let results = parse_and_bind_parallel(files);

    let total_nodes: usize = results.iter().map(|r| r.arena.len()).sum();
    let total_symbols: usize = results.iter().map(|r| r.symbols.len()).sum();
    let parse_error_count: usize = results.iter().map(|r| r.parse_diagnostics.len()).sum();

    let stats = BindStats {
        file_count,
        total_nodes,
        total_symbols,
        parse_error_count,
    };

    (results, stats)
}

/// Load lib.d.ts files and create `LibContext` objects for the binder.
///
/// This function loads the specified lib.d.ts files (e.g., lib.dom.d.ts, lib.es*.d.ts)
/// and returns `LibContext` objects that can be used during binding to resolve global
/// symbols like `console`, `Array`, `Promise`, etc.
///
/// This is similar to `load_lib_files_for_contexts` in driver.rs but returns
/// Arc<LibFile> objects for use with `merge_lib_symbols`.
pub fn load_lib_files_for_binding(lib_files: &[&Path]) -> Vec<Arc<lib_loader::LibFile>> {
    use crate::parser::ParserState;
    use rayon::prelude::{IntoParallelIterator, ParallelIterator};

    if lib_files.is_empty() {
        return Vec::new();
    }

    // Collect paths that exist
    let files_to_load: Vec<_> = lib_files
        .iter()
        .filter_map(|p| {
            let path = p.to_path_buf();
            path.exists().then_some(path)
        })
        .collect();

    // Parse and bind lib files in parallel for faster startup
    files_to_load
        .into_par_iter()
        .filter_map(|lib_path| {
            // Read the lib file content
            let source_text = std::fs::read_to_string(&lib_path).ok()?;

            // Parse the lib file
            let file_name = lib_path.to_string_lossy().to_string();
            let mut lib_parser = ParserState::new(file_name.clone(), source_text);
            let source_file_idx = lib_parser.parse_source_file();

            // Skip if there are parse errors
            if !lib_parser.get_diagnostics().is_empty() {
                return None;
            }

            // Bind the lib file
            let mut lib_binder = BinderState::new();
            lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

            // Create the LibFile
            let arena = Arc::new(lib_parser.into_arena());
            let binder = Arc::new(lib_binder);

            Some(Arc::new(lib_loader::LibFile::new(
                file_name,
                arena,
                binder,
                source_file_idx,
            )))
        })
        .collect()
}

/// Load lib.d.ts files from disk for binding, failing on any load/parse error.
///
/// Unlike `load_lib_files_for_binding`, this enforces strict disk-loading semantics:
/// missing files, unreadable files, and parse errors are surfaced as hard errors.
pub fn load_lib_files_for_binding_strict(
    lib_files: &[&Path],
) -> Result<Vec<Arc<lib_loader::LibFile>>> {
    if lib_files.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: Read all files and resolve references.
    //
    // OPTIMIZATION: Pre-read ALL .d.ts files in the lib directory into memory
    // before processing references. This batches all file I/O upfront instead
    // of interleaving reads with reference resolution, reducing the impact of
    // I/O contention under system load. On a loaded system (load avg 20+),
    // individual file reads can take 10-20ms each due to scheduling delays.
    // Batch reading brings this down to ~2-5ms total for the entire directory.
    // Check if all requested lib files are available as embedded content.
    // If so, skip the batch disk read entirely (zero I/O startup).
    // Embedded files use bare names (e.g., "dom.d.ts") while on-disk files
    // have a "lib." prefix (e.g., "lib.dom.d.ts"). Strip the prefix for lookup.
    let all_embedded = lib_files.iter().all(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|name| name.strip_prefix("lib.").unwrap_or(name))
            .is_some_and(crate::embedded_libs::is_embedded_lib)
    });

    let lib_dir = lib_files
        .first()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."));
    let mut file_cache: FxHashMap<PathBuf, String> = FxHashMap::default();
    if !all_embedded {
        // Custom lib dir or non-embedded files — read from disk
        if let Ok(entries) = std::fs::read_dir(lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "ts")
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    file_cache.insert(path, content);
                }
            }
        }
    }

    let mut loaded = FxHashSet::default();
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for path in lib_files {
        collect_lib_files_recursive_cached(path, &mut loaded, &mut file_contents, &file_cache)?;
    }

    if file_contents.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 2: Parse and bind all files in parallel (CPU bound — the expensive part).
    // Sort largest files first so rayon's work-stealing starts them early.
    // dom.d.ts (40K lines, 2MB) dominates parse time — without this sort it's
    // file #81 of 87 and becomes the critical-path bottleneck.
    file_contents.sort_by_key(|b| std::cmp::Reverse(b.1.len()));

    // Parse and bind all lib files in parallel using the global rayon pool.
    // The global pool threads are already warm (no thread creation overhead).
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    let results: Vec<Result<Arc<lib_loader::LibFile>>> = maybe_parallel_into!(file_contents)
        .map(|(file_name, source_text)| parse_and_bind_lib_file(file_name, source_text))
        .collect();

    // Collect results, propagating any parse errors
    results.into_iter().collect()
}

/// Clone lib files into fresh checker-only binders using the already-loaded source text.
///
/// The binders used during program construction are mutated while merging lib symbols into
/// user-file binders. Checker-facing lib contexts and lib-file checks need fresh binder state
/// so declaration merging and semantic lookups run against clean lib binders.
#[must_use]
pub fn clone_lib_files_for_checker(
    lib_files: &[Arc<lib_loader::LibFile>],
) -> Vec<Arc<lib_loader::LibFile>> {
    lib_files
        .iter()
        .map(|lib| {
            let source = lib
                .arena
                .get_source_file_at(lib.root_index)
                .unwrap_or_else(|| panic!("missing source text for lib file {}", lib.file_name));
            Arc::new(lib_loader::LibFile::from_source(
                lib.file_name.clone(),
                source.text.to_string(),
            ))
        })
        .collect()
}

/// Parse and bind a single lib file, returning a `LibFile` or error.
fn parse_and_bind_lib_file(
    file_name: String,
    source_text: String,
) -> Result<Arc<lib_loader::LibFile>> {
    let mut lib_parser = ParserState::new(file_name.clone(), source_text);
    let source_file_idx = lib_parser.parse_source_file();
    let diagnostics = lib_parser.get_diagnostics();
    if !diagnostics.is_empty() {
        let first = &diagnostics[0];
        bail!(
            "failed to parse lib file {} ({}:{}): {}",
            file_name,
            first.start,
            first.length,
            first.message
        );
    }

    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);
    Ok(Arc::new(lib_loader::LibFile::new(
        file_name,
        arena,
        binder,
        source_file_idx,
    )))
}

/// Phase 1 helper with pre-loaded file cache. Uses embedded lib contents
/// first (zero I/O), then pre-read file cache, then disk as last resort.
fn collect_lib_files_recursive_cached(
    path: &Path,
    loaded: &mut FxHashSet<PathBuf>,
    file_contents: &mut Vec<(String, String)>,
    file_cache: &FxHashMap<PathBuf, String>,
) -> Result<()> {
    // Skip canonicalize (stat syscall) when using embedded content.
    // Embedded files use bare names (e.g., "dom.d.ts") while on-disk files
    // have a "lib." prefix (e.g., "lib.dom.d.ts"). Strip the prefix for lookup.
    let raw_basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let embedded_name = raw_basename.strip_prefix("lib.").unwrap_or(raw_basename);
    let lib_path = if crate::embedded_libs::is_embedded_lib(embedded_name) && file_cache.is_empty()
    {
        path.to_path_buf()
    } else {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    };
    if !loaded.insert(lib_path.clone()) {
        return Ok(());
    }

    // Prefer physical lib files when they exist so diagnostic offsets match the
    // TypeScript lib baselines. Fall back to embedded libs only when running
    // without an on-disk TypeScript lib directory.
    let basename = lib_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let embedded_key = basename.strip_prefix("lib.").unwrap_or(basename);
    let source_text = if let Some(cached) = file_cache.get(&lib_path) {
        // File was read from disk (custom lib dir with non-standard files) — use it
        cached.clone()
    } else if lib_path.exists() {
        std::fs::read_to_string(&lib_path)
            .with_context(|| format!("failed to read lib file {}", lib_path.display()))?
    } else if let Some(embedded) = crate::embedded_libs::get_lib_content(embedded_key) {
        // Built-in embedded content — zero I/O, comment-stripped for faster parsing
        embedded.to_string()
    } else {
        // Fallback to disk read
        std::fs::read_to_string(&lib_path)
            .with_context(|| format!("failed to read lib file {}", lib_path.display()))?
    };

    // Resolve references before adding this file (dependencies come first)
    for ref_lib in parse_lib_references(&source_text) {
        if let Some(ref_path) = resolve_lib_reference_path(&lib_path, &ref_lib) {
            collect_lib_files_recursive_cached(&ref_path, loaded, file_contents, file_cache)?;
        }
    }

    let file_name = lib_path.to_string_lossy().to_string();
    file_contents.push((file_name, source_text));
    Ok(())
}

fn parse_lib_references(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // References are always at the top of lib files. Once we see a line
        // that isn't a comment, empty, or copyright header, stop scanning.
        // This avoids iterating through 40K+ lines for dom.d.ts.
        if !trimmed.is_empty()
            && !trimmed.starts_with("///")
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
        {
            break;
        }
        if !trimmed.starts_with("///") {
            continue;
        }
        if let Some(start) = trimmed.find("<reference") {
            let rest = &trimmed[start..];
            if let Some(lib_start) = rest.find("lib=") {
                let after_lib = &rest[lib_start + 4..];
                let quote = after_lib.chars().next();
                if quote == Some('"') || quote == Some('\'') {
                    let quote_char = quote
                        .expect("guarded by quote == Some('\"') || quote == Some('\\'') check");
                    let value_start = 1;
                    if let Some(end) = after_lib[value_start..].find(quote_char) {
                        refs.push(
                            after_lib[value_start..value_start + end]
                                .trim()
                                .to_lowercase(),
                        );
                    }
                }
            }
        }
    }
    refs
}

fn resolve_lib_reference_path(base_path: &Path, lib_name: &str) -> Option<PathBuf> {
    let lib_dir = base_path.parent()?;
    let normalized = normalize_lib_reference_name(lib_name);
    let mut candidate_names = vec![normalized.clone()];
    match normalized.as_str() {
        // Source-tree libs use *.generated.d.ts while built/local and npm libs use plain names.
        "dom" => candidate_names.push("dom.generated".to_string()),
        "dom.iterable" => candidate_names.push("dom.iterable.generated".to_string()),
        "dom.asynciterable" => candidate_names.push("dom.asynciterable.generated".to_string()),
        "dom.generated" => candidate_names.push("dom".to_string()),
        "dom.iterable.generated" => candidate_names.push("dom.iterable".to_string()),
        "dom.asynciterable.generated" => candidate_names.push("dom.asynciterable".to_string()),
        _ => {}
    }
    let candidates: Vec<PathBuf> = candidate_names
        .into_iter()
        .flat_map(|name| {
            [
                lib_dir.join(format!("lib.{name}.d.ts")),
                lib_dir.join(format!("{name}.d.ts")),
            ]
        })
        .collect();
    // Check embedded libs first (no syscall), then fall back to disk stat.
    candidates.into_iter().find(|candidate| {
        candidate
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::embedded_libs::is_embedded_lib)
            || candidate.exists()
    })
}

fn normalize_lib_reference_name(name: &str) -> String {
    match name.to_lowercase().trim() {
        "es6" => "es6".to_string(),
        "es7" => "es2016".to_string(),
        "lib" | "lib.d.ts" => "es5".to_string(),
        // Modern TypeScript (6.x) uses lib.dom.d.ts directly, not .generated suffix.
        // Pass through as-is — the file candidates already include lib.{name}.d.ts.
        "dom" | "dom.iterable" | "dom.asynciterable" => name.to_lowercase(),
        s if s.starts_with("lib.") && s.ends_with(".d.ts") => {
            let inner = &s[4..s.len() - 5];
            normalize_lib_reference_name(inner)
        }
        other => other.to_string(),
    }
}

/// Parse and bind multiple files in parallel with lib symbol injection.
///
/// This is the main entry point for compilation that includes lib.d.ts symbols.
/// Lib files are loaded first, then each file is parsed and bound with lib symbols
/// merged into its binder.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Optional list of lib file paths to load
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_lib_files(
    files: Vec<(String, String)>,
    lib_files: &[&Path],
) -> Vec<BindResult> {
    // Load lib files for binding.
    // This path is intentionally strict so missing/unreadable lib files are not ignored.
    let lib_contexts = load_lib_files_for_binding_strict(lib_files)
        .unwrap_or_else(|err| panic!("failed to load lib files from disk: {err}"));

    // Parse and bind with lib symbols
    parse_and_bind_parallel_with_libs(files, &lib_contexts)
}

/// Parse and bind multiple files in parallel with lib contexts.
///
/// Lib symbols are injected into each file's binder during binding,
/// enabling resolution of global symbols like `console`, `Array`, etc.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Lib files to merge into each binder
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> Vec<BindResult> {
    if files.len() <= 1 {
        return files
            .into_iter()
            .map(|(file_name, source_text)| bind_file_with_libs(file_name, source_text, lib_files))
            .collect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| bind_file_with_libs(file_name, source_text, lib_files))
        .collect()
}

fn bind_file_with_libs(
    file_name: String,
    source_text: String,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> BindResult {
    // Skip parsing .json files - they should not be parsed as TypeScript.
    // JSON module imports should be resolved during module resolution and
    // emit TS2732 if resolveJsonModule is false.
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    // Parse
    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    // Bind with lib symbols
    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);

    // IMPORTANT: Merge lib symbols BEFORE binding source file
    // so that symbols like console, Array, Promise are available during binding
    if !lib_files.is_empty() {
        binder.merge_lib_symbols(lib_files);
    }

    binder.bind_source_file(&arena, source_file);

    // Extract lib_binders and lib_arenas from binder before it's moved
    let lib_binders = binder.lib_binders.clone();
    let lib_arenas: Vec<Arc<NodeArena>> =
        lib_files.iter().map(|lf| Arc::clone(&lf.arena)).collect();

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders,
        lib_arenas,
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
        file_import_sources: binder.file_import_sources,
    }
}

// =============================================================================
// File Skeleton IR
// =============================================================================

// Skeleton types are in the skeleton submodule
use super::skeleton::*;
// Dependency graph built from skeleton import_sources
use super::dep_graph::DepGraph;

/// A bound file ready for type checking
pub struct BoundFile {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes (owned by this file)
    pub arena: Arc<NodeArena>,
    /// Node-to-symbol mapping (symbol IDs are global after merge).
    ///
    /// Shared via `Arc` so cross-file lookup binders (one per file in the
    /// parallel CLI pipeline) can take an O(1) reference to this file's
    /// per-file map instead of deep-cloning the underlying `FxHashMap`. On
    /// large repos (6086 files), the deep clone of `node_symbols` was one
    /// of the largest per-binder allocations. PR #1202 applied the same
    /// template to `semantic_defs`; this extends it to `node_symbols`.
    pub node_symbols: Arc<FxHashMap<u32, SymbolId>>,
    /// Per-file symbol-to-arena mapping captured during binding.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver and
    /// the parallel checker can share via `Arc::clone` (atomic increment)
    /// instead of deep-cloning the underlying `FxHashMap`. Same template as
    /// the recently-merged BoundFile field Arc-wraps (#1399 / #1404 / #1409
    /// / #1416 / #1428 / #1535 / #1559).
    pub symbol_arenas: Arc<FxHashMap<SymbolId, Arc<NodeArena>>>,
    /// Per-file declaration-to-arena mapping captured during binding.
    ///
    /// `Arc`-wrapped to mirror `BinderState.declaration_arenas` (same field)
    /// so per-file binders share via `Arc::clone` instead of deep-cloning
    /// the underlying map.
    pub declaration_arenas: Arc<DeclarationArenaMap>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: Arc<FxHashMap<u32, bool>>,
    /// Persistent scopes (symbol IDs are global after merge).
    ///
    /// `Arc`-wrapped to mirror `BinderState.scopes` so per-file binders
    /// constructed in the cross-file lookup pipeline share via
    /// `Arc::clone` instead of deep-cloning. Same pattern as the recently-
    /// merged BoundFile field Arc-wraps (#1399 / #1404 / #1409 / #1416 /
    /// #1428 / #1535).
    pub scopes: Arc<Vec<Scope>>,
    /// Map from AST node to scope ID.
    ///
    /// `Arc`-wrapped to mirror `BinderState.node_scope_ids` so per-file
    /// binders share via `Arc::clone` instead of deep-cloning. Read-only
    /// after binding completes.
    pub node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Global augmentations (interface declarations inside `declare global` blocks).
    ///
    /// `Arc`-wrapped to mirror `BinderState.global_augmentations` so per-file
    /// binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub global_augmentations: Arc<FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>>,
    /// Module augmentations (interface/type declarations inside `declare module 'x'` blocks).
    ///
    /// `Arc`-wrapped to mirror `BinderState.module_augmentations` so per-file
    /// binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub module_augmentations: Arc<FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>>,
    /// Maps symbols declared inside module augmentation blocks to their target module specifier.
    ///
    /// `Arc`-wrapped to mirror `BinderState.augmentation_target_modules` so
    /// per-file binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub augmentation_target_modules: Arc<FxHashMap<SymbolId, String>>,
    /// Flow nodes for control flow analysis.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver can
    /// share this file's flow graph via `Arc::clone` (atomic increment)
    /// instead of deep-cloning the underlying `Vec<FlowNode>` (each
    /// `FlowNode` owns a `Vec<FlowNodeId>` antecedents). The driver builds
    /// ~2×N per-file binders (cross-file lookup + per-file checking), so
    /// on N-file projects this previously cost 2N deep clones of the
    /// per-file flow graph.
    pub flow_nodes: Arc<FlowNodeArena>,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node.
    ///
    /// Shared via `Arc` so cross-file lookup binders (one per file in the
    /// parallel CLI pipeline) can take an O(1) reference to this file's
    /// per-file map instead of deep-cloning the underlying `FxHashMap`. On
    /// large repos (6086 files), the deep clone of `node_flow` was one of
    /// the largest per-binder allocations after the `semantic_defs` (#1202)
    /// and `node_symbols` (#1227) Arc migrations.
    pub node_flow: Arc<FxHashMap<u32, FlowNodeId>>,
    /// Map from switch clause `NodeIndex` to parent switch statement `NodeIndex`
    /// Used by control flow analysis for switch exhaustiveness checking.
    ///
    /// `Arc`-wrapped so per-file binders share via `Arc::clone` instead of
    /// deep-cloning. Read-only after binding completes.
    pub switch_clause_to_switch: Arc<FxHashMap<u32, NodeIndex>>,
    /// Whether this file is an external module (has imports/exports)
    pub is_external_module: bool,
    /// Expando property assignments detected during binding.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver
    /// (cross-file lookup + primary checker, ~2N for N files) share via
    /// `Arc::clone` instead of deep-cloning the nested map. Read-only
    /// after `bind_source_file` completes.
    pub expando_properties: Arc<FxHashMap<String, FxHashSet<String>>>,
    pub file_features: crate::binder::FileFeatures,
    /// Reverse mapping for merged lib symbols: remapped `SymbolId` ->
    /// (`lib_binder_idx`, original lib-local `SymbolId`).
    /// Reconstructed binders need this to keep lib delegation caches from
    /// polluting file-local symbol state.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver
    /// (one cross-file lookup binder + one primary checker binder per
    /// file) can share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap`. Read-only after
    /// `merge_lib_contexts_into_binder` completes; the merge path uses
    /// `Arc::make_mut`, which is free when refcount=1.
    pub lib_symbol_reverse_remap: Arc<FxHashMap<SymbolId, (usize, SymbolId)>>,
    /// Per-file semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Contains only entries that originated in this file (post-remap `SymbolIds`).
    /// This enables file-scoped identity without cloning the entire global map.
    ///
    /// Shared via `Arc` so cross-file lookup binders can take an O(1) reference
    /// instead of deep-cloning the underlying map per file. See
    /// `tsz_cli::driver::check_utils::create_cross_file_lookup_binder_with_augmentations`.
    pub semantic_defs: Arc<FxHashMap<SymbolId, crate::binder::SemanticDefEntry>>,
}

impl BoundFile {
    /// Estimate the heap memory footprint of this bound file in bytes.
    ///
    /// Accounts for the struct itself plus all heap-allocated strings, vecs,
    /// hash map entries, and flow arena contents. The `NodeArena` behind
    /// `Arc` counts only the `Arc` overhead (shared data is tracked
    /// separately via unique-arena deduplication in `MergedProgramResidencyStats`).
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // file_name
        size += self.file_name.capacity();

        // arena (Arc overhead only — shared data not double-counted)
        size += std::mem::size_of::<NodeArena>();

        // node_symbols
        size += self.node_symbols.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<SymbolId>() + 8);

        // symbol_arenas
        size += self.symbol_arenas.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<Arc<NodeArena>>() + 8);

        // declaration_arenas
        size += self.declaration_arenas.capacity()
            * (std::mem::size_of::<(SymbolId, NodeIndex)>()
                + std::mem::size_of::<Vec<Arc<NodeArena>>>()
                + 8);

        // module_declaration_exports_publicly
        size += self.module_declaration_exports_publicly.capacity()
            * (std::mem::size_of::<u32>() + 1 + 8);

        // scopes
        size += self.scopes.capacity() * std::mem::size_of::<Scope>();
        for scope in self.scopes.iter() {
            size += scope.table.len() * (32 + std::mem::size_of::<SymbolId>());
        }

        // node_scope_ids
        size += self.node_scope_ids.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<ScopeId>() + 8);

        // parse_diagnostics
        size += self.parse_diagnostics.capacity() * std::mem::size_of::<ParseDiagnostic>();
        for diag in &self.parse_diagnostics {
            size += diag.message.capacity();
        }

        // global_augmentations
        for (k, v) in self.global_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::GlobalAugmentation>();
        }

        // module_augmentations
        for (k, v) in self.module_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::ModuleAugmentation>();
            for aug in v {
                size += aug.name.capacity();
            }
        }

        // augmentation_target_modules
        for v in self.augmentation_target_modules.values() {
            size += std::mem::size_of::<SymbolId>() + v.capacity() + 8;
        }

        // flow_nodes
        size += self.flow_nodes.len() * std::mem::size_of::<crate::binder::FlowNode>();
        for flow_node in self.flow_nodes.iter() {
            size += flow_node.antecedent.capacity() * std::mem::size_of::<FlowNodeId>();
        }

        // node_flow
        size += self.node_flow.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<FlowNodeId>() + 8);

        // switch_clause_to_switch
        size += self.switch_clause_to_switch.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<NodeIndex>() + 8);

        // expando_properties
        for (k, v) in self.expando_properties.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for s in v {
                size += s.capacity() + std::mem::size_of::<u64>();
            }
        }

        // lib_symbol_reverse_remap
        size += self.lib_symbol_reverse_remap.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<(usize, SymbolId)>() + 8);

        // semantic_defs (per-file)
        size += self.semantic_defs.capacity()
            * (std::mem::size_of::<SymbolId>()
                + std::mem::size_of::<crate::binder::SemanticDefEntry>()
                + 8);
        for entry in self.semantic_defs.values() {
            size += entry.name.capacity();
            size += entry.enum_member_names.capacity() * 24; // String overhead
            for m in &entry.enum_member_names {
                size += m.capacity();
            }
            size += entry.extends_names.capacity() * 24;
            for h in &entry.extends_names {
                size += h.capacity();
            }
            size += entry.implements_names.capacity() * 24;
            for h in &entry.implements_names {
                size += h.capacity();
            }
        }

        size
    }
}

use tsz_solver::TypeInterner;
use tsz_solver::def::DefinitionStore;

/// Merged program state after parallel binding
pub struct MergedProgram {
    /// All bound files
    pub files: Vec<BoundFile>,
    /// Global symbol arena (all symbols from all files, with remapped IDs)
    pub symbols: SymbolArena,
    /// Symbol-to-arena mapping for declaration lookup (legacy, stores last arena).
    ///
    /// Wrapped in `Arc` so per-file checker binders can share the merged map
    /// via `Arc::clone` (O(1)) instead of building a per-file derived map.
    pub symbol_arenas: Arc<FxHashMap<SymbolId, Arc<NodeArena>>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (`SymbolId`, `NodeIndex` of declaration) -> Arena(s) containing that declaration.
    ///
    /// `Arc`-wrapped so per-file `BinderState.declaration_arenas` reconstruction
    /// is a cheap atomic increment instead of iterating the entire program-wide
    /// map per file. On large projects this map holds ~100K entries and the
    /// CLI driver builds ~12K per-file binders; the previous per-file materialization
    /// iterated ~100K entries × ~12K binders ≈ 1.2B entry visits at startup.
    pub declaration_arenas: Arc<DeclarationArenaMap>,
    /// Secondary index: `SymbolId` → every `NodeIndex` that appears as a
    /// declaration key for that symbol. Built once at merge time so checker
    /// paths that need to enumerate a symbol's declarations can do a point
    /// lookup instead of iterating the program-wide `declaration_arenas`.
    pub sym_to_decl_indices: Arc<SymToDeclIndicesMap>,
    /// Cross-file `node_symbols`: maps arena pointer → `node_symbols` for that arena.
    /// Enables resolving type references in cross-file interface declarations.
    pub cross_file_node_symbols: CrossFileNodeSymbols,
    /// Global symbol table (exports from all files)
    pub globals: SymbolTable,
    /// Per-file symbol tables (file-local symbols, symbol IDs remapped)
    pub file_locals: Vec<SymbolTable>,
    /// Ambient module declarations across all files
    pub declared_modules: FxHashSet<String>,
    /// Shorthand ambient modules (`declare module "foo"` without body) - imports from these are `any`
    pub shorthand_ambient_modules: Arc<FxHashSet<String>>,
    /// Module exports: maps file name (or module specifier) to its exported symbols
    /// This enables cross-file module resolution: import { X } from './file' can find X's symbol
    /// `Arc`-wrapped so per-file `BinderState` reconstruction is a cheap atomic
    /// increment instead of a deep clone of the merged map.
    pub module_exports: Arc<FxHashMap<String, SymbolTable>>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    /// Maps (`current_file`, `exported_name`) -> (`source_module`, `original_name`)
    pub reexports: Arc<Reexports>,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps `current_file` -> Vec of `source_modules`
    /// `Arc`-wrapped so per-file `BinderState` reconstruction is a
    /// cheap atomic increment instead of a deep clone of the merged
    /// `FxHashMap`. Mutations during binding go through `Arc::make_mut`.
    pub wildcard_reexports: Arc<WildcardReexportsMap>,
    /// Wildcard re-export type-only provenance per entry.
    pub wildcard_reexports_type_only: Arc<WildcardReexportsTypeOnlyMap>,
    /// Lib binders for global type resolution (Array, String, Promise, etc.)
    /// These contain symbols from lib.d.ts files and enable resolution of built-in types
    pub lib_binders: Arc<Vec<Arc<BinderState>>>,
    /// Global symbol IDs that originated from lib files (remapped to global arena IDs).
    /// `Arc`-wrapped so the CLI driver can install the same set into
    /// every per-file `BinderState.lib_symbol_ids` via `Arc::clone`
    /// (cheap atomic increment) instead of deep-cloning the
    /// `FxHashSet` for each of N per-file binders.
    pub lib_symbol_ids: Arc<FxHashSet<SymbolId>>,
    /// Global type interner - shared across all threads for type deduplication
    pub type_interner: TypeInterner,
    /// Alias partners: maps `TYPE_ALIAS` `SymbolId` → `ALIAS` `SymbolId` for merged type+namespace exports.
    /// When `export type X = ...` and `export * as X from "..."` coexist, the exports table
    /// holds the `TYPE_ALIAS` symbol and this map links it to the ALIAS symbol for value resolution.
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,
    /// Binder-captured semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Maps post-remap `SymbolId` → `SemanticDefEntry` across all files.
    /// The checker reads this during construction to pre-create solver `DefIds`.
    ///
    /// `Arc`-wrapped so the parallel checker's lib-check pass and the
    /// per-file binder reconstruction paths can share via `Arc::clone`
    /// (atomic increment) instead of deep-cloning the underlying
    /// `FxHashMap`. The lib-check overlays an always-empty per-lib map
    /// on top of this (`build_lib_bound_file_for_interface_checks`
    /// returns an empty `semantic_defs`), so for the lib path the
    /// `Arc::clone` is the entire cost.
    pub semantic_defs: Arc<FxHashMap<SymbolId, crate::binder::SemanticDefEntry>>,
    /// Shared `DefinitionStore` pre-populated with `DefId`s for all top-level
    /// semantic definitions during the merge phase. This moves identity creation
    /// from checker pre-population (per-file, order-dependent) to merge time
    /// (single pass, deterministic). Checker contexts receive this via
    /// `with_options_and_shared_def_store` and only need to warm local caches.
    pub definition_store: std::sync::Arc<DefinitionStore>,
    /// Skeleton index computed alongside the legacy merge path.
    ///
    /// This captures the same merge-relevant topology (symbol merging, augmentation
    /// targets, re-export graph) without retaining any arena or binder state.
    /// It is computed from pre-merge `BindResult`s during `merge_bind_results_ref`
    /// and stored here so downstream consumers can begin migrating off arena-backed
    /// lookups toward skeleton-based queries.
    pub skeleton_index: Option<SkeletonIndex>,
    /// Dependency graph derived from skeleton `import_sources`.
    ///
    /// Built using `DepGraph::build_simple` during merge (name-matching heuristic).
    /// Provides topological ordering for incremental invalidation and ordered
    /// checking. `None` only if no skeletons were extracted (should not happen
    /// in the normal pipeline).
    pub dep_graph: Option<DepGraph>,
    /// Sum of `BindResult::estimated_size_bytes()` across all input files, computed
    /// before the merge consumes per-file data. This captures the pre-merge memory
    /// footprint so it can be compared to the post-merge `MergedProgram` residency.
    pub pre_merge_bind_total_bytes: usize,
}

impl MergedProgram {
    /// Return the topological file ordering from the dependency graph.
    ///
    /// Dependencies come before dependents. Files in cycles are appended
    /// in stable (input) order. Returns `None` if no dep graph was computed.
    #[must_use]
    pub fn topological_file_order(&self) -> Option<super::dep_graph::TopoResult> {
        self.dep_graph.as_ref().map(|dg| dg.topological_order())
    }

    /// Return the set of file indices that directly depend on the given file.
    ///
    /// These are files that `import` from the target file. Useful for
    /// incremental invalidation: when `file_idx` changes, its dependents
    /// may need re-checking.
    #[must_use]
    pub fn dependents_of(&self, file_idx: usize) -> Option<&rustc_hash::FxHashSet<usize>> {
        self.dep_graph.as_ref().map(|dg| dg.dependents(file_idx))
    }

    /// Return the set of file indices that the given file depends on.
    #[must_use]
    pub fn dependencies_of(&self, file_idx: usize) -> Option<&rustc_hash::FxHashSet<usize>> {
        self.dep_graph.as_ref().map(|dg| dg.dependencies(file_idx))
    }
}

/// Check if two symbols can be merged across multiple files.
///
/// TypeScript allows merging:
/// - Interface + Interface (declaration merging)
/// - Namespace + Namespace (declaration merging)
/// - Class + Interface (merging for class declarations)
/// - Function + Function (overloads - handled per-file)
pub(super) const fn can_merge_symbols_cross_file(existing_flags: u32, new_flags: u32) -> bool {
    use crate::binder::symbol_flags;

    // Interface can merge with interface
    if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::INTERFACE) != 0
    {
        return true;
    }

    // Class can merge with interface
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Interface can merge with variable (e.g., `interface Promise<T>` + `declare var Promise: PromiseConstructor`)
    // This is fundamental to how TypeScript lib declarations work: types have both an interface
    // (type side) and a variable declaration (value side).
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Interface can merge with function (e.g., `interface Array<T>` + `declare function Array(...)`)
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::FUNCTION) != 0)
        || ((existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Namespace/module can merge with namespace/module
    if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
        return true;
    }

    // Variable can merge with variable cross-file (so we can detect and report cross-file redeclarations of let/const)
    if (existing_flags & symbol_flags::VARIABLE) != 0 && (new_flags & symbol_flags::VARIABLE) != 0 {
        return true;
    }

    // Class can merge with Class cross-file (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::CLASS) != 0 {
        return true;
    }

    // Class can merge with Type Alias (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
        || ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Type Alias (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::TYPE_ALIAS) != 0
    {
        return true;
    }

    // Type Alias can merge with Interface (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Class can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Namespace can merge with class, function, enum, or variable
    if (existing_flags & symbol_flags::MODULE) != 0
        && (new_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }
    if (new_flags & symbol_flags::MODULE) != 0
        && (existing_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }

    // Enum can merge with enum
    if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
        return true;
    }

    false
}

/// Append declarations from `incoming` into `existing` without duplicates.
///
/// Small declaration lists are common, so use linear scans there to avoid
/// hash set allocation overhead. Switch to a set only for larger collections.
fn append_unique_declarations(existing: &mut Vec<NodeIndex>, incoming: &[NodeIndex]) {
    for &decl in incoming {
        if !existing.contains(&decl) {
            existing.push(decl);
        }
    }
}

/// Remap `__unique_{SymbolId}` keys in `expando_properties` to use global `SymbolIds`.
///
/// During binding, expando property tracking stores unique symbol keys as
/// `__unique_{local_SymbolId}`. After `merge_bind_results` remaps all `SymbolIds`
/// to a global arena, these encoded IDs become stale. This function updates
/// them so the checker's `UniqueSymbol` types (which use global IDs) match.
fn remap_expando_properties(
    expando: &FxHashMap<String, FxHashSet<String>>,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> Arc<FxHashMap<String, FxHashSet<String>>> {
    Arc::new(
        expando
            .iter()
            .map(|(obj_name, props)| {
                let remapped_props = props
                    .iter()
                    .map(|prop| {
                        if let Some(old_id_str) = prop.strip_prefix("__unique_")
                            && let Ok(old_id) = old_id_str.parse::<u32>()
                            && let Some(&new_id) = id_remap.get(&SymbolId(old_id))
                        {
                            return format!("__unique_{}", new_id.0);
                        }
                        prop.clone()
                    })
                    .collect();
                (obj_name.clone(), remapped_props)
            })
            .collect(),
    )
}

/// Pre-populate a `DefinitionStore` from the merged `semantic_defs` map.
///
/// This converts each `SemanticDefEntry` into a solver `DefinitionInfo`,
/// registers it in the store, and records the `(SymbolId, file_id) → DefId`
/// mapping. The resulting store is shared across all checker contexts so
/// that `DefId` allocation happens once (at merge time) rather than
/// per-file during checker pre-population.
///
/// Delegates to `DefinitionStore::from_semantic_defs` — the canonical
/// solver-owned factory for converting binder identity to solver `DefId`s.
pub fn pre_populate_definition_store(
    semantic_defs: &FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    interner: &TypeInterner,
) -> DefinitionStore {
    DefinitionStore::from_semantic_defs(semantic_defs, |s| interner.intern_string(s))
}

/// Resolve heritage names to `DefId`s in a pre-populated `DefinitionStore`.
///
/// For each class/interface with `extends_names` or `implements_names`, look up
/// the target by name in the store's `name_to_defs` index and wire:
/// - `extends`: first `extends_name` matching a Class or Interface (for classes,
///   this is the parent class; for interfaces, the first extended interface)
/// - `implements`: all `implements_names` matching an Interface
///
/// Only simple identifier names are resolved. Property-access names (e.g.,
/// `ns.Base`) contain dots and cannot match any DefId name, so they are
/// silently skipped (the checker resolves them during type checking).
///
/// This is called as Pass 3 of `pre_populate_definition_store` and can also
/// be called standalone for cross-batch heritage resolution.
pub fn resolve_heritage_in_store(
    semantic_defs: &FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    store: &DefinitionStore,
    interner: &TypeInterner,
) {
    use tsz_solver::def::DefKind;

    for (&sym_id, entry) in semantic_defs {
        let def_id = match store.find_def_by_symbol(sym_id.0) {
            Some(id) => id,
            None => continue,
        };

        // Resolve extends_names → DefinitionInfo.extends
        if !entry.extends_names.is_empty() {
            for name_str in &entry.extends_names {
                // Skip property-access names (contain dots) — checker resolves these
                if name_str.contains('.') {
                    continue;
                }
                let name_atom = interner.intern_string(name_str);
                if let Some(candidates) = store.find_defs_by_name(name_atom) {
                    for &candidate_id in &candidates {
                        if candidate_id == def_id {
                            continue; // skip self
                        }
                        if let Some(candidate_info) = store.get(candidate_id)
                            && matches!(candidate_info.kind, DefKind::Class | DefKind::Interface)
                        {
                            store.set_extends(def_id, candidate_id);
                            break;
                        }
                    }
                }
                // Only use the first extends name for the `extends` field
                // (classes have at most one extends target)
                break;
            }
        }

        // Resolve implements_names → DefinitionInfo.implements
        if !entry.implements_names.is_empty() {
            let mut resolved_implements = Vec::new();
            for name_str in &entry.implements_names {
                if name_str.contains('.') {
                    continue;
                }
                let name_atom = interner.intern_string(name_str);
                if let Some(candidates) = store.find_defs_by_name(name_atom) {
                    for &candidate_id in &candidates {
                        if candidate_id == def_id {
                            continue;
                        }
                        if let Some(candidate_info) = store.get(candidate_id)
                            && matches!(candidate_info.kind, DefKind::Interface | DefKind::Class)
                        {
                            resolved_implements.push(candidate_id);
                            break;
                        }
                    }
                }
            }
            if !resolved_implements.is_empty() {
                store.set_implements(def_id, resolved_implements);
            }
        }
    }
}

/// Create a `DefinitionStore` from a single binder's `semantic_defs`.
///
/// This is the single-file equivalent of `pre_populate_definition_store`.
/// It allows single-file checker contexts to receive a pre-populated store
/// rather than creating an empty one and relying on checker-side
/// `pre_populate_def_ids_from_binder()` repair.
///
/// The resulting store can be shared via `Arc` and passed to
/// `CheckerState::new_with_shared_def_store`.
pub fn create_definition_store_from_binder(
    binder: &crate::binder::BinderState,
    interner: &TypeInterner,
) -> DefinitionStore {
    pre_populate_definition_store(&binder.semantic_defs, interner)
}

/// Merge bind results into a unified program state
///
/// This is a sequential operation that combines:
/// - All symbol arenas into a single global arena
/// - Merges symbols with the same name across files (for interfaces, namespaces, etc.)
/// - Remaps symbol IDs in `node_symbols` to use global IDs
///
/// # Arguments
/// * `results` - Vector of `BindResult` from parallel binding
///
/// # Returns
/// `MergedProgram` with unified symbol space
pub fn merge_bind_results(results: Vec<BindResult>) -> MergedProgram {
    let refs: Vec<&BindResult> = results.iter().collect();
    merge_bind_results_ref(&refs)
}

pub fn merge_bind_results_ref(results: &[&BindResult]) -> MergedProgram {
    // Extract file skeletons from pre-merge bind results and reduce them into a
    // global index. This runs before the legacy merge so we capture the original
    // per-file symbol/augmentation/re-export data without any remapping.
    let skeletons: Vec<FileSkeleton> = results.iter().map(|r| extract_skeleton(r)).collect();
    let skeleton_index = reduce_skeletons(&skeletons);
    let dep_graph = DepGraph::build_simple(&skeletons);

    // Capture aggregate pre-merge memory footprint before we start consuming data.
    let pre_merge_bind_total_bytes: usize = results.iter().map(|r| r.estimated_size_bytes()).sum();

    // Collect lib_binders from all results (deduplicated by address), paired with their arenas
    let mut lib_binders: Vec<Arc<BinderState>> = Vec::new();
    let mut lib_binder_set: FxHashSet<usize> = FxHashSet::default();
    let mut lib_binder_arena_map: FxHashMap<usize, Arc<NodeArena>> = FxHashMap::default();
    for result in results {
        for (lib_binder, lib_arena) in result.lib_binders.iter().zip(result.lib_arenas.iter()) {
            let binder_addr = Arc::as_ptr(lib_binder) as usize;
            if lib_binder_set.insert(binder_addr) {
                lib_binders.push(Arc::clone(lib_binder));
                lib_binder_arena_map.insert(binder_addr, Arc::clone(lib_arena));
            }
        }
    }

    // Calculate total symbols needed (including lib symbols)
    let lib_symbol_count: usize = lib_binders.iter().map(|b| b.symbols.len()).sum();
    let user_symbol_count: usize = results.iter().map(|r| r.symbols.len()).sum();
    let total_symbols = lib_symbol_count + user_symbol_count;

    // Create global symbol arena with pre-allocated capacity
    let mut global_symbols = SymbolArena::with_capacity(total_symbols);
    let mut symbol_arenas = FxHashMap::default();
    let mut declaration_arenas: DeclarationArenaMap = FxHashMap::default();
    let mut cross_file_node_symbols: CrossFileNodeSymbols = FxHashMap::default();
    let estimated_global_count: usize = results.iter().map(|r| r.file_locals.len()).sum();
    let mut globals = SymbolTable::with_capacity(estimated_global_count);
    let mut files = Vec::with_capacity(results.len());
    let mut file_locals_list = Vec::with_capacity(results.len());
    let mut declared_modules = FxHashSet::default();
    let mut shorthand_ambient_modules = FxHashSet::default();
    let mut module_exports: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut alias_partners: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
    let mut semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry> =
        FxHashMap::default();
    let mut reexports: Reexports = FxHashMap::default();
    let mut wildcard_reexports: FxHashMap<String, Vec<String>> = FxHashMap::default();
    let mut wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>> =
        FxHashMap::default();
    let mut global_lib_symbol_ids: FxHashSet<SymbolId> = FxHashSet::default();

    // Track which symbols have been merged to avoid duplicate processing.
    // Use interned atoms to avoid repeated String hashing/cloning on hot merge paths.
    let mut name_interner = Interner::new();
    // Track which symbols have been merged to avoid duplicate processing
    // IMPORTANT: This map is ONLY for symbols in the ROOT scope (ScopeId(0))
    // Symbols from nested scopes should NEVER be merged across files/scopes
    let mut merged_symbols: FxHashMap<Atom, SymbolId> = FxHashMap::default();

    // ==========================================================================
    // PHASE 1: Remap lib symbols to global arena
    // ==========================================================================
    // This creates a mapping from (lib_binder_ptr, local_id) -> global_id
    // so that file_locals can reference lib symbols using global IDs
    let mut lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();

    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;

        // Pre-build a set of top-level symbol IDs from file_locals for O(1) lookup.
        // This avoids an O(N*F) quadratic scan where each symbol would linearly
        // search file_locals to check if it's top-level.
        let top_level_ids: FxHashSet<SymbolId> =
            lib_binder.file_locals.iter().map(|(_, id)| *id).collect();

        // For external module lib files (e.g. esnext.iterator.d.ts with
        // `export {}`), build a set of declaration NodeIndices from
        // `declare global { ... }` blocks. Module-scoped declarations
        // must NOT be merged into existing global symbols.
        let global_aug_nodes: Option<FxHashSet<NodeIndex>> = if lib_binder.is_external_module {
            let mut nodes = FxHashSet::default();
            for augs in lib_binder.global_augmentations.values() {
                for aug in augs {
                    nodes.insert(aug.node);
                }
            }
            Some(nodes)
        } else {
            None
        };

        // Process all symbols in this lib binder
        for i in 0..lib_binder.symbols.len() {
            let local_id = SymbolId(i as u32);
            if let Some(lib_sym) = lib_binder.symbols.get(local_id) {
                // Determine if this is a top-level symbol by checking file_locals.
                // In lib files, declarations like `declare namespace Reflect` may appear
                // in a child scope (e.g., ScopeId(1)) even though they're conceptually
                // top-level. Using file_locals is more reliable than the scope check
                // for determining which lib symbols should be globally merged.
                let is_top_level = top_level_ids.contains(&local_id);

                // Check if a symbol with this name already exists (cross-lib merging)
                // IMPORTANT: Only merge top-level symbols (those in file_locals)
                // Nested symbols (namespace members, etc.) should NEVER be merged across scopes
                let global_id = if is_top_level {
                    // For external module lib binders (e.g. esnext.iterator.d.ts
                    // with `export {}`), do NOT merge top-level symbols into
                    // the global symbol table. Their module-scoped declarations
                    // (class/interface) would contaminate global symbols with
                    // the same name. Global contributions come solely via
                    let name_atom = name_interner.intern(&lib_sym.escaped_name);
                    if let Some(&existing_id) = merged_symbols.get(&name_atom) {
                        // Symbol already exists - check if we can merge
                        if let Some(existing_sym) = global_symbols.get(existing_id) {
                            if can_merge_symbols_cross_file(existing_sym.flags, lib_sym.flags) {
                                // Merge: reuse existing symbol ID
                                // Merge declarations from this lib
                                if let Some(existing_mut) = global_symbols.get_mut(existing_id) {
                                    // For external module lib binders, only merge
                                    // declarations from `declare global` blocks.
                                    // Module-scoped declarations would contaminate
                                    // global symbols (e.g. module-scoped class Iterator
                                    // in esnext.iterator.d.ts vs global interface Iterator).
                                    if let Some(ref aug_nodes) = global_aug_nodes {
                                        let filtered: Vec<_> = lib_sym
                                            .declarations
                                            .iter()
                                            .copied()
                                            .filter(|d| aug_nodes.contains(d))
                                            .collect();
                                        if !filtered.is_empty() {
                                            append_unique_declarations(
                                                &mut existing_mut.declarations,
                                                &filtered,
                                            );
                                        }
                                        // Do NOT merge flags from external module symbols
                                        // to avoid contaminating global types with
                                        // module-scoped CLASS flag etc.
                                    } else {
                                        existing_mut.flags |= lib_sym.flags;
                                        append_unique_declarations(
                                            &mut existing_mut.declarations,
                                            &lib_sym.declarations,
                                        );
                                    }
                                }
                                existing_id
                            } else {
                                // Cannot merge - allocate new (shadowing)
                                let new_id = global_symbols.alloc_from(lib_sym);
                                merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // Shouldn't happen - allocate new
                            let new_id = global_symbols.alloc_from(lib_sym);
                            merged_symbols.insert(name_atom, new_id);
                            new_id
                        }
                    } else {
                        // New symbol - allocate in global arena
                        let new_id = global_symbols.alloc_from(lib_sym);
                        merged_symbols.insert(name_atom, new_id);
                        new_id
                    }
                } else {
                    // Nested symbol - always allocate new, never merge

                    // NOTE: Don't add to merged_symbols - nested symbols should never be cross-file merged
                    global_symbols.alloc_from(lib_sym)
                };

                // Store the remapping
                lib_symbol_remap.insert((lib_binder_ptr, local_id), global_id);

                // Set arena mappings for this lib symbol using the lib file's arena.
                // The original lib binder's symbol_arenas/declaration_arenas are empty
                // (only populated during per-file merge which uses a different binder).
                // We use lib_binder_arena_map to get the correct arena for this lib file.
                if let Some(lib_arena) = lib_binder_arena_map.get(&lib_binder_ptr) {
                    symbol_arenas
                        .entry(global_id)
                        .or_insert_with(|| Arc::clone(lib_arena));
                    for &decl in &lib_sym.declarations {
                        declaration_arenas
                            .entry((global_id, decl))
                            .or_default()
                            .push(Arc::clone(lib_arena));
                    }
                }
            }
        }
    }

    // ==========================================================================
    // PHASE 1.25: Clear un-remapped exports/members from global symbols
    // ==========================================================================
    // Phase 1's `alloc_from()` copies symbols including their exports/members
    // tables, but those tables contain lib-LOCAL SymbolIds. In the global arena,
    // those same numeric IDs map to DIFFERENT symbols (e.g., lib-local SymbolId(2)
    // might be DateTimeFormat in es5.d.ts, but SymbolId(2) in the global arena is
    // cancelIdleCallback from dom.d.ts). Phase 1.5 will rebuild exports/members
    // with correctly remapped global IDs, so we must clear the corrupt data first.
    {
        let lib_global_ids: FxHashSet<SymbolId> = lib_symbol_remap.values().copied().collect();
        for &global_id in &lib_global_ids {
            if let Some(sym) = global_symbols.get_mut(global_id) {
                sym.exports = None;
                sym.members = None;
            }
        }
    }

    // ==========================================================================
    // PHASE 1.5: Remap internal references (parent, exports, members)
    // ==========================================================================
    // After all lib symbols have been allocated in the global arena, we need a
    // second pass to fix up internal SymbolId references. The `alloc_from()` call
    // copies the symbol data including members/exports/parent, but those fields
    // still contain LOCAL SymbolIds from the original lib binder. We must remap
    // them to the corresponding global IDs using lib_symbol_remap.
    // (This mirrors Phase 2 in state.rs merge_lib_contexts_into_binder.)
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;

        for i in 0..lib_binder.symbols.len() {
            let local_id = SymbolId(i as u32);
            let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) else {
                continue;
            };
            let Some(lib_sym) = lib_binder.symbols.get(local_id) else {
                continue;
            };

            // Remap parent
            if lib_sym.parent.is_some()
                && let Some(&new_parent) = lib_symbol_remap.get(&(lib_binder_ptr, lib_sym.parent))
                && let Some(sym) = global_symbols.get_mut(global_id)
            {
                sym.parent = new_parent;
            }

            // Remap exports: replace local IDs with global IDs.
            // When an export name was already remapped by a previous lib binder,
            // merge the new symbol's flags/declarations into the existing one
            // (e.g., INTERFACE from one lib + VALUE from another, like
            // DateTimeFormat in Intl across es5.d.ts and es2017.intl.d.ts).
            if let Some(exports) = &lib_sym.exports {
                let mut new_exports: Vec<(String, SymbolId)> = Vec::new();
                let mut merge_targets: Vec<(SymbolId, SymbolId)> = Vec::new();

                if let Some(sym) = global_symbols.get(global_id) {
                    let existing_exports = sym.exports.as_ref();
                    for (name, &export_id) in exports.iter() {
                        if let Some(&new_export_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, export_id))
                        {
                            let prev = existing_exports.and_then(|e| e.get(name));
                            if let Some(prev_export_id) = prev {
                                if prev_export_id != new_export_id {
                                    merge_targets.push((prev_export_id, new_export_id));
                                }
                            } else {
                                new_exports.push((name.clone(), new_export_id));
                            }
                        }
                    }
                }

                for (dst_id, src_id) in merge_targets {
                    let src_data = global_symbols
                        .get(src_id)
                        .map(|s| (s.flags, s.declarations.clone(), s.value_declaration));
                    if let Some((src_flags, src_decls, src_value_decl)) = src_data
                        && let Some(dst) = global_symbols.get_mut(dst_id)
                    {
                        dst.flags |= src_flags;
                        for d in src_decls {
                            if !dst.declarations.contains(&d) {
                                dst.declarations.push(d);
                            }
                        }
                        if dst.value_declaration.is_none() && src_value_decl.is_some() {
                            dst.value_declaration = src_value_decl;
                        }
                    }

                    // Copy declaration_arenas and symbol_arenas entries from src to dst.
                    // When interface symbols inside namespaces are merged (e.g.,
                    // Intl.DateTimeFormat from es5.d.ts + es2017.intl.d.ts), the dst
                    // symbol gets src's declarations appended, but the checker needs
                    // declaration_arenas[(dst_id, decl)] to find the correct arena
                    // for each declaration. Without this, merged declarations are
                    // invisible and the interface type is incomplete.
                    if let Some(src_sym) = global_symbols.get(src_id) {
                        let src_decls = src_sym.declarations.clone();
                        for decl_idx in src_decls {
                            if let Some(arenas) =
                                declaration_arenas.get(&(src_id, decl_idx)).cloned()
                            {
                                declaration_arenas
                                    .entry((dst_id, decl_idx))
                                    .or_default()
                                    .extend(arenas);
                            }
                        }
                    }
                    if let Some(src_arena) = symbol_arenas.get(&src_id).cloned() {
                        symbol_arenas.entry(dst_id).or_insert(src_arena);
                    }
                }

                if !new_exports.is_empty()
                    && let Some(sym) = global_symbols.get_mut(global_id)
                {
                    if sym.exports.is_none() {
                        sym.exports = Some(Box::new(SymbolTable::with_capacity(new_exports.len())));
                    }
                    if let Some(existing) = sym.exports.as_mut() {
                        for (name, id) in new_exports {
                            existing.set(name, id);
                        }
                    }
                }
            }

            // Remap members: replace local IDs with global IDs.
            // Same merge-instead-of-overwrite logic as exports.
            if let Some(members) = &lib_sym.members {
                let mut new_members: Vec<(String, SymbolId)> = Vec::new();
                let mut merge_targets: Vec<(SymbolId, SymbolId)> = Vec::new();

                if let Some(sym) = global_symbols.get(global_id) {
                    let existing_members = sym.members.as_ref();
                    for (name, &member_id) in members.iter() {
                        if let Some(&new_member_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, member_id))
                        {
                            let prev = existing_members.and_then(|m| m.get(name));
                            if let Some(prev_member_id) = prev {
                                if prev_member_id != new_member_id {
                                    merge_targets.push((prev_member_id, new_member_id));
                                }
                            } else {
                                new_members.push((name.clone(), new_member_id));
                            }
                        }
                    }
                }

                for (dst_id, src_id) in merge_targets {
                    let src_data = global_symbols
                        .get(src_id)
                        .map(|s| (s.flags, s.declarations.clone(), s.value_declaration));
                    if let Some((src_flags, src_decls, src_value_decl)) = src_data
                        && let Some(dst) = global_symbols.get_mut(dst_id)
                    {
                        dst.flags |= src_flags;
                        for d in src_decls {
                            if !dst.declarations.contains(&d) {
                                dst.declarations.push(d);
                            }
                        }
                        if dst.value_declaration.is_none() && src_value_decl.is_some() {
                            dst.value_declaration = src_value_decl;
                        }
                    }

                    // Copy declaration_arenas and symbol_arenas (same as exports above)
                    if let Some(src_sym) = global_symbols.get(src_id) {
                        let src_decls = src_sym.declarations.clone();
                        for decl_idx in src_decls {
                            if let Some(arenas) =
                                declaration_arenas.get(&(src_id, decl_idx)).cloned()
                            {
                                declaration_arenas
                                    .entry((dst_id, decl_idx))
                                    .or_default()
                                    .extend(arenas);
                            }
                        }
                    }
                    if let Some(src_arena) = symbol_arenas.get(&src_id).cloned() {
                        symbol_arenas.entry(dst_id).or_insert(src_arena);
                    }
                }

                if !new_members.is_empty()
                    && let Some(sym) = global_symbols.get_mut(global_id)
                {
                    if sym.members.is_none() {
                        sym.members = Some(Box::new(SymbolTable::with_capacity(new_members.len())));
                    }
                    if let Some(existing) = sym.members.as_mut() {
                        for (name, id) in new_members {
                            existing.set(name, id);
                        }
                    }
                }
            }
        }
    }

    // Also remap lib file_locals entries that reference symbols by name
    // (for exported lib symbols like Array, Object, console)
    let mut lib_name_to_global: FxHashMap<Atom, SymbolId> = FxHashMap::default();
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;
        for (name, &local_id) in lib_binder.file_locals.iter() {
            // When a lib file is an external module (has `export {}`), its
            // file_locals contain module-scoped declarations that must NOT
            // pollute the global scope. Only include symbols that originate
            // from `declare global { ... }` blocks.
            if lib_binder.is_external_module && !lib_binder.global_augmentations.contains_key(name)
            {
                continue;
            }
            if let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                // Only keep the first mapping for each name (lib files are processed in order)
                let name_atom = name_interner.intern(name);
                lib_name_to_global.entry(name_atom).or_insert(global_id);
            }
        }
    }

    // ==========================================================================
    // PHASE 1.6: Propagate lib semantic_defs directly to global semantic_defs
    // ==========================================================================
    // Lib binders record `semantic_defs` for their top-level declarations during
    // binding (TypeAlias, Interface, Class, Enum, Namespace, Function, Variable).
    // Phase 1 already remapped lib SymbolIds to global IDs. We propagate the
    // semantic_defs using that remap so the checker can pre-create solver DefIds
    // for ALL lib symbols at construction time.
    //
    // Previously, lib semantic_defs only reached the global map indirectly through
    // per-file binders (which ran `merge_lib_symbols` Phase 4). That path is
    // redundant and order-dependent — by propagating directly here, the merge is
    // self-contained and deterministic.
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;
        for (&old_sym_id, entry) in lib_binder.semantic_defs.iter() {
            if let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, old_sym_id)) {
                // Keep first occurrence (declaration merging keeps first identity).
                semantic_defs.entry(global_id).or_insert_with(|| {
                    let mut remapped = entry.clone();
                    // Update file_id to match the global symbol's decl_file_idx
                    // so DefinitionStore composite key lookups stay consistent.
                    remapped.file_id = global_symbols
                        .get(global_id)
                        .map_or(entry.file_id, |s| s.decl_file_idx);
                    // Remap parent_namespace to global SymbolId
                    remapped.parent_namespace = entry.parent_namespace.and_then(|old_parent| {
                        lib_symbol_remap.get(&(lib_binder_ptr, old_parent)).copied()
                    });
                    remapped
                });
            }
        }
    }

    // ==========================================================================
    // PHASE 2: Process user files
    // ==========================================================================

    for (file_idx, result) in results.iter().enumerate() {
        declared_modules.extend(result.declared_modules.iter().cloned());
        shorthand_ambient_modules.extend(result.shorthand_ambient_modules.iter().cloned());

        // Merge reexports from this file
        for (file_name, file_reexports) in result.reexports.iter() {
            let entry = reexports.entry(file_name.clone()).or_default();
            for (export_name, mapping) in file_reexports {
                entry.insert(export_name.clone(), mapping.clone());
            }
        }

        // Merge wildcard reexports from this file
        for (file_name, source_modules) in result.wildcard_reexports.iter() {
            let entry = wildcard_reexports.entry(file_name.clone()).or_default();
            let type_only_entry = wildcard_reexports_type_only
                .entry(file_name.clone())
                .or_default();
            let source_type_only = result.wildcard_reexports_type_only.get(file_name);

            if entry.len() + source_modules.len() <= 16 {
                for (i, source_module) in source_modules.iter().enumerate() {
                    // Use index-based access to get the correct type-only flag
                    let source_is_type_only = source_type_only
                        .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                        .unwrap_or(false);

                    if let Some(pos) = entry.iter().position(|m| m == source_module) {
                        // Already have this source — if this path is non-type-only,
                        // override the existing flag (value re-export takes priority).
                        if !source_is_type_only {
                            type_only_entry[pos].1 = false;
                        }
                    } else {
                        entry.push(source_module.clone());
                        type_only_entry.push((source_module.clone(), source_is_type_only));
                    }
                }
            } else {
                let mut seen: FxHashMap<String, usize> = entry.iter().cloned().zip(0..).collect();
                for (i, source_module) in source_modules.iter().enumerate() {
                    let source_is_type_only = source_type_only
                        .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                        .unwrap_or(false);

                    if let Some(&pos) = seen.get(source_module) {
                        if !source_is_type_only {
                            type_only_entry[pos].1 = false;
                        }
                    } else {
                        let pos = entry.len();
                        seen.insert(source_module.clone(), pos);
                        entry.push(source_module.clone());
                        type_only_entry.push((source_module.clone(), source_is_type_only));
                    }
                }
            }
        }
        // Copy symbols from this file to global arena, getting new IDs
        let mut id_remap: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
        for i in 0..result.symbols.len() {
            let old_id = SymbolId(i as u32);
            if let Some(sym) = result.symbols.get(old_id) {
                // For lib-originated symbols, reuse the Phase 1 global IDs rather than
                // allocating new ones. This prevents duplicate lib symbols and ensures
                // the Phase 1.5 remapped exports/members are preserved.
                if result.lib_symbol_ids.contains(&old_id) {
                    // For lib-originated symbols, use the reverse remap to find the
                    // original (lib_binder_ptr, local_id), then look up the Phase 1
                    // global ID via lib_symbol_remap. This ensures all lib symbols
                    // (both top-level and nested) map to their Phase 1 global IDs,
                    // preserving the Phase 1.5 export/member remapping.
                    let mut resolved_global_id = None;
                    if let Some(&(binder_ptr, original_local_id)) =
                        result.lib_symbol_reverse_remap.get(&old_id)
                        && let Some(&global_id) =
                            lib_symbol_remap.get(&(binder_ptr, original_local_id))
                    {
                        resolved_global_id = Some(global_id);
                    }
                    // Fallback: look up by name in merged_symbols or lib_name_to_global
                    if resolved_global_id.is_none() {
                        let name_atom = name_interner.intern(&sym.escaped_name);
                        if let Some(&global_id) = merged_symbols.get(&name_atom) {
                            resolved_global_id = Some(global_id);
                        }
                        if resolved_global_id.is_none()
                            && let Some(&global_id) = lib_name_to_global.get(&name_atom)
                        {
                            resolved_global_id = Some(global_id);
                        }
                    }
                    if let Some(global_id) = resolved_global_id {
                        // The user binder may have merged additional flags and declarations
                        // into this lib symbol (e.g., user `interface Event<T>` augments
                        // lib's non-generic `Event`, or user `type Proxy<T>` adds TYPE_ALIAS
                        // to lib's `declare var Proxy`). Always propagate extra flags and
                        // user-local declarations to the global symbol so that type parameter
                        // resolution can find them.
                        if let Some(global_sym) = global_symbols.get_mut(global_id) {
                            let extra_flags = sym.flags & !global_sym.flags;
                            if extra_flags != 0 {
                                global_sym.flags |= extra_flags;
                            }
                            // Always copy user declarations that were merged into this symbol,
                            // even when flags are identical. Without this, user declarations
                            // (e.g., a generic `interface Event<T>`) are lost and
                            // get_type_params_for_symbol won't find their type parameters.
                            append_unique_declarations(
                                &mut global_sym.declarations,
                                &sym.declarations,
                            );
                        }
                        id_remap.insert(old_id, global_id);
                        continue;
                    }
                    // Last resort: allocate a new ID (shouldn't happen normally)
                    let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                    symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                    id_remap.insert(old_id, new_id);
                    continue;
                }

                // Check if this symbol is from a nested scope.
                // We check whether this symbol ID appears in the ROOT scope table
                // (ScopeId(0) = SourceFile scope). This is more reliable than checking
                // node_scope_ids because not all declaration types create scopes
                // (e.g., InterfaceDeclaration does not create a scope, so its node
                // won't appear in node_scope_ids, causing false negatives).
                let is_nested_symbol = !result.scopes.first().is_some_and(|root_scope| {
                    root_scope
                        .table
                        .get(&sym.escaped_name)
                        .is_some_and(|root_sym_id| root_sym_id == old_id)
                });

                // Check if symbol already exists in globals (cross-file merging)
                // IMPORTANT: Only merge symbols from ROOT scope (ScopeId(0))
                // Nested scope symbols should NEVER be merged across scopes
                let new_id = if !is_nested_symbol && !result.is_external_module {
                    let name_atom = name_interner.intern(&sym.escaped_name);
                    if let Some(&existing_id) = merged_symbols.get(&name_atom) {
                        // Symbol exists - check if we can merge
                        if let Some(existing_sym) = global_symbols.get(existing_id) {
                            // Check if symbols can merge (interface+interface, namespace+namespace, etc.)
                            if can_merge_symbols_cross_file(existing_sym.flags, sym.flags) {
                                // Merge: reuse existing symbol ID, will merge declarations below
                                existing_id
                            } else {
                                // Cannot merge - allocate new symbol (shadowing or duplicate)
                                let new_id =
                                    global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                                symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                                merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // Shouldn't happen - allocate new
                            let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                            symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                            merged_symbols.insert(name_atom, new_id);
                            new_id
                        }
                    } else {
                        // New symbol - allocate
                        let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                        symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                        merged_symbols.insert(name_atom, new_id);
                        new_id
                    }
                } else {
                    // Nested symbol - always allocate new, never merge or add to merged_symbols
                    let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                    symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                    // NOTE: Don't add to merged_symbols - nested symbols should never be cross-file merged
                    new_id
                };
                id_remap.insert(old_id, new_id);
            }
        }

        // Track remapped lib symbol IDs for unused-checking exclusion
        for &old_lib_id in result.lib_symbol_ids.iter() {
            if let Some(&new_id) = id_remap.get(&old_lib_id) {
                global_lib_symbol_ids.insert(new_id);
            }
        }

        // Copy symbol_arenas entries from user file, remapping IDs
        // This propagates lib symbol arena mappings that were created during merge_lib_symbols
        for (&old_sym_id, arena) in result.symbol_arenas.iter() {
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                symbol_arenas
                    .entry(new_sym_id)
                    .or_insert_with(|| Arc::clone(arena));
            }
        }

        // Copy declaration_arenas entries from user file, remapping symbol IDs.
        // Skip lib-originated symbols: their declaration_arenas were already set up
        // in Phase 1 from the original lib binder. The per-file binder has duplicate
        // arenas for the same declarations (from merge_lib_contexts_into_binder),
        // which would cause interface members to be lowered multiple times.
        for (&(old_sym_id, decl_idx), arenas) in result.declaration_arenas.iter() {
            if result.lib_symbol_ids.contains(&old_sym_id) {
                continue;
            }
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                let target = declaration_arenas
                    .entry((new_sym_id, decl_idx))
                    .or_default();
                for arena in arenas {
                    target.push(Arc::clone(arena));
                }
            }
        }

        // Collect exported symbols for this file (for module_exports map).
        //
        // Note: `export default ...` must be represented under the `"default"` export name
        // so that `import X from "./mod"` can resolve correctly.
        //
        // We intentionally do *not* depend solely on `sym.is_exported` for determining whether
        // a file is an external module, because default exports may not correspond to a named
        // export in `file_locals`.
        let mut exports = SymbolTable::with_capacity(result.file_locals.len().saturating_add(1));
        let mut export_equals_old: Option<SymbolId> = None;

        // 1) Named exports collected from file_locals.
        for (name, &sym_id) in result.file_locals.iter() {
            // Skip lib/global symbols (e.g. `escape`, `unescape`) that were merged
            // into file_locals from lib.d.ts. These are global builtins that should
            // not appear in a user module's module_exports.
            if result.lib_symbol_ids.contains(&sym_id) {
                continue;
            }
            if name == "export=" {
                export_equals_old = Some(sym_id);
            }
            if let Some(sym) = result.symbols.get(sym_id)
                && (sym.is_exported || name == "export=")
                && let Some(&remapped_id) = id_remap.get(&sym_id)
            {
                exports.set(name.clone(), remapped_id);
            }
        }

        // 1b) `export = target` should also expose namespace members from `target`.
        if let Some(old_export_equals_sym) = export_equals_old
            && let Some(target_symbol) = result.symbols.get(old_export_equals_sym)
        {
            if let Some(target_exports) = target_symbol.exports.as_ref() {
                for (export_name, old_sym_id) in target_exports.iter() {
                    // Skip "default" — the `export =` target itself IS the default
                    // export. A static member named `default` (e.g. `static default: "foo"`)
                    // must not shadow the `export=` symbol in module_exports.
                    if export_name == "default" {
                        continue;
                    }
                    if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                        exports.set(export_name.clone(), remapped_id);
                    }
                }
            }
            if let Some(target_members) = target_symbol.members.as_ref() {
                for (member_name, old_sym_id) in target_members.iter() {
                    if member_name == "default" {
                        continue;
                    }
                    if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                        exports.set(member_name.clone(), remapped_id);
                    }
                }
            }
        }

        // 2) Default export: add `"default"` entry when present.
        let mut default_export_old: Option<SymbolId> = None;
        if let Some(root_node) = result.arena.get(result.source_file)
            && let Some(source) = result.arena.get_source_file(root_node)
        {
            for &stmt_idx in &source.statements.nodes {
                let Some(stmt_node) = result.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }
                let Some(export_decl) = result.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if !export_decl.is_default_export {
                    continue;
                }

                // `export default <expr>;`
                let Some(clause_node) = result.arena.get(export_decl.export_clause) else {
                    continue;
                };

                // Best-effort: if the default export is a reference to a named declaration
                // (identifier/class/function), map `"default"` to that symbol.
                //
                // This matches the needs of `import X from "./mod"` and keeps the symbol ID
                // stable across files without synthesizing a new symbol.
                if clause_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
                    if let Some(ident) = result.arena.get_identifier(clause_node) {
                        default_export_old = result.file_locals.get(&ident.escaped_text);
                    }
                } else if let Some(func) = result.arena.get_function(clause_node) {
                    if let Some(name_node) = result.arena.get(func.name)
                        && let Some(ident) = result.arena.get_identifier(name_node)
                    {
                        default_export_old = result.file_locals.get(&ident.escaped_text);
                    }
                } else if let Some(class) = result.arena.get_class(clause_node)
                    && let Some(name_node) = result.arena.get(class.name)
                    && let Some(ident) = result.arena.get_identifier(name_node)
                {
                    default_export_old = result.file_locals.get(&ident.escaped_text);
                }

                // Only one default export per module.
                break;
            }
        }

        if let Some(old_sym_id) = default_export_old
            && let Some(&remapped_id) = id_remap.get(&old_sym_id)
        {
            exports.set("default".to_string(), remapped_id);
        }

        let remap_symbol_table =
            |table: &SymbolTable, id_remap: &FxHashMap<SymbolId, SymbolId>| -> SymbolTable {
                let mut remapped = SymbolTable::with_capacity(table.len());
                for (name, old_sym_id) in table.iter() {
                    if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                        remapped.set(name.clone(), new_sym_id);
                    }
                }
                remapped
            };

        let merge_symbol_table = |dst: &mut SymbolTable, src: &SymbolTable| {
            for (name, sym_id) in src.iter() {
                if !dst.has(name) {
                    dst.set(name.clone(), *sym_id);
                }
            }
        };

        if !exports.is_empty() {
            module_exports.insert(result.file_name.clone(), exports);
        }

        for (module_key, exports_table) in result.module_exports.iter() {
            let remapped = remap_symbol_table(exports_table, &id_remap);
            if !remapped.is_empty() {
                merge_symbol_table(
                    module_exports.entry(module_key.clone()).or_default(),
                    &remapped,
                );
            }
        }

        // Remap binder's per-file alias_partners to global SymbolIds
        for (&type_alias_id, &alias_id) in &result.alias_partners {
            if let (Some(&new_ta), Some(&new_alias)) =
                (id_remap.get(&type_alias_id), id_remap.get(&alias_id))
            {
                alias_partners.insert(new_ta, new_alias);
            }
        }

        // Remap binder's per-file semantic_defs to global SymbolIds (Phase 1 DefId-first).
        // Skip lib-originated symbols — they were already propagated in Phase 1.6.
        // Also collect per-file entries for BoundFile.semantic_defs (file-scoped identity).
        let mut file_semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry> =
            FxHashMap::default();
        for (old_sym_id, entry) in result.semantic_defs.iter() {
            if result.lib_symbol_ids.contains(old_sym_id) {
                continue;
            }
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                // Update file_id to use the global file index
                let mut remapped_entry = entry.clone();
                remapped_entry.file_id = file_idx as u32;
                // Remap parent_namespace to global SymbolId
                remapped_entry.parent_namespace = entry
                    .parent_namespace
                    .and_then(|old_parent| id_remap.get(&old_parent).copied());
                // Collect per-file entry (always insert — no cross-file merging here)
                file_semantic_defs.insert(new_sym_id, remapped_entry.clone());
                // Insert the first occurrence, or accumulate heritage/metadata from
                // later files via merge_cross_file (e.g., cross-file interface merging,
                // class + interface merging).
                semantic_defs
                    .entry(new_sym_id)
                    .and_modify(|existing| existing.merge_cross_file(&remapped_entry))
                    .or_insert(remapped_entry);
            }
        }

        // Collect all nested merge pairs across all symbols in this file,
        // then process them AFTER all symbols have their data populated.
        // This is critical because HashMap iteration order is random — if a
        // parent symbol is processed before its children, the children won't
        // have their exports populated yet, making recursive merge ineffective.
        let mut all_nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();

        // Sort id_remap entries by old_id (ascending) so that symbol processing
        // order is deterministic regardless of FxHashMap iteration order. This
        // ensures declaration_arenas entries and nested merge pairs are always
        // collected in the same order across runs, producing identical merged
        // output for identical inputs.
        let mut sorted_remap: Vec<(SymbolId, SymbolId)> =
            id_remap.iter().map(|(&old, &new)| (old, new)).collect();
        sorted_remap.sort_unstable_by_key(|(old, _)| old.0);

        for &(old_id, new_id) in &sorted_remap {
            // Skip lib-originated symbols - they were already set up by Phase 1 + 1.5
            if result.lib_symbol_ids.contains(&old_id) {
                continue;
            }
            let Some(old_sym) = result.symbols.get(old_id) else {
                continue;
            };

            // CRITICAL: Populate declaration_arenas for user symbols
            for &decl_idx in &old_sym.declarations {
                declaration_arenas
                    .entry((new_id, decl_idx))
                    .or_default()
                    .push(Arc::clone(&result.arena));
            }

            let mut nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();
            if let Some(new_sym) = global_symbols.get_mut(new_id) {
                // Check if this is a cross-file merge (same symbol already has data)
                let is_cross_file_merge = !new_sym.declarations.is_empty();

                if is_cross_file_merge {
                    // Cross-file merge: append declarations and merge flags
                    new_sym.flags |= old_sym.flags;
                    // Append new declarations from this file, but skip NodeIndex values
                    // that already exist from a DIFFERENT arena (cross-file NodeIndex
                    // collision). When two files produce the same NodeIndex for different
                    // declarations, adding duplicates causes the checker to look up the
                    // wrong arena and misidentify declaration kinds (e.g., treating a
                    // remote interface as a local class, triggering false TS2300).
                    // The declaration_arenas entry already contains both arenas for the
                    // colliding NodeIndex, so the checker can iterate all arenas there.
                    {
                        let mut filtered_decls: Vec<NodeIndex> = Vec::new();
                        for &decl_idx in &old_sym.declarations {
                            if new_sym.declarations.contains(&decl_idx) {
                                // NodeIndex collision: this index already exists in the
                                // merged symbol from a previous file. Check if the
                                // declaration_arenas entry has a different arena (meaning
                                // it's from a different file, not a true duplicate).
                                if let Some(arenas) = declaration_arenas.get(&(new_id, decl_idx)) {
                                    let has_different_arena = arenas.iter().any(|a| {
                                        !std::ptr::eq(Arc::as_ptr(a), Arc::as_ptr(&result.arena))
                                    });
                                    if has_different_arena {
                                        // Skip: this is a cross-file collision, not a
                                        // true duplicate declaration within the same file.
                                        continue;
                                    }
                                }
                            }
                            filtered_decls.push(decl_idx);
                        }
                        append_unique_declarations(&mut new_sym.declarations, &filtered_decls);
                    }
                    // Update value_declaration if the old one was NONE
                    if new_sym.value_declaration.is_none() && old_sym.value_declaration.is_some() {
                        new_sym.value_declaration = old_sym.value_declaration;
                    }
                    // Merge exports (if both have exports)
                    // First pass: add missing exports, collect nested merge targets
                    if let (Some(old_exports), Some(new_exports)) =
                        (old_sym.exports.as_ref(), new_sym.exports.as_mut())
                    {
                        for (name, sym_id) in old_exports.iter() {
                            if !new_exports.has(name) {
                                // Remap the symbol ID and add to exports
                                if let Some(&remapped_id) = id_remap.get(sym_id) {
                                    new_exports.set(name.clone(), remapped_id);
                                }
                            } else if let Some(&remapped_new_id) = id_remap.get(sym_id) {
                                // Both files export the same name (e.g., nested namespace Utils).
                                // Record for deferred merge outside the get_mut borrow scope.
                                let existing_export_id = new_exports
                                    .get(name)
                                    .expect("else branch guarantees name exists in new_exports");
                                if existing_export_id != remapped_new_id {
                                    nested_merges.push((existing_export_id, remapped_new_id));
                                }
                            }
                        }
                    }
                    // Handle case where old symbol has exports but new doesn't yet
                    if old_sym.exports.is_some() && new_sym.exports.is_none() {
                        new_sym.exports = old_sym
                            .exports
                            .as_ref()
                            .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    }
                    // Merge members (if both have members)
                    if let (Some(old_members), Some(new_members)) =
                        (old_sym.members.as_ref(), new_sym.members.as_mut())
                    {
                        for (name, sym_id) in old_members.iter() {
                            if !new_members.has(name) {
                                // Remap the symbol ID and add to members
                                if let Some(&remapped_id) = id_remap.get(sym_id) {
                                    new_members.set(name.clone(), remapped_id);
                                }
                            }
                        }
                    }
                } else {
                    // First time seeing this symbol - full update
                    let mut updated = old_sym.clone();
                    updated.id = new_id;
                    updated.parent = id_remap
                        .get(&old_sym.parent)
                        .copied()
                        .unwrap_or(SymbolId::NONE);
                    updated.value_declaration = old_sym.value_declaration;
                    updated.declarations = old_sym.declarations.clone();
                    updated.is_exported = old_sym.is_exported;
                    updated.is_umd_export = old_sym.is_umd_export;
                    // Track which file this symbol was declared in for TDZ cross-file detection
                    updated.decl_file_idx = file_idx as u32;
                    // Finalize file index on stable declaration locations that
                    // were recorded by per-file binders with `u32::MAX` (the
                    // parallel pipeline does not call `BinderState::set_file_idx`
                    // before binding). This keeps the Phase 1 stable-location
                    // invariants consistent with `decl_file_idx`.
                    let stamped = file_idx as u32;
                    for stable in &mut updated.stable_declarations {
                        stable.set_file_idx_if_unassigned(stamped);
                    }
                    updated
                        .stable_value_declaration
                        .set_file_idx_if_unassigned(stamped);
                    updated.exports = old_sym
                        .exports
                        .as_ref()
                        .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    updated.members = old_sym
                        .members
                        .as_ref()
                        .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    *new_sym = updated;
                }
            }

            // Collect nested merges for processing AFTER all symbols are populated
            all_nested_merges.extend(nested_merges);
        }

        // Process all nested merges now that every symbol has its data populated.
        // Uses a work queue to handle arbitrarily deep nesting (e.g.,
        // namespace A.B.C.D declared across files needs recursive merge).
        while let Some((existing_id, source_id)) = all_nested_merges.pop() {
            // Collect data from source symbol first
            let merge_data = global_symbols.get(source_id).map(|src| {
                (
                    src.flags,
                    src.declarations.clone(),
                    src.value_declaration,
                    src.exports.as_ref().cloned(),
                    src.members.as_ref().cloned(),
                )
            });
            if let Some((src_flags, src_decls, src_val_decl, src_exports, src_members)) = merge_data
                && let Some(dst) = global_symbols.get_mut(existing_id)
            {
                let can_merge = can_merge_symbols_cross_file(dst.flags, src_flags);
                if !can_merge {
                    continue;
                }
                dst.flags |= src_flags;
                // Propagate declaration_arenas from source to destination
                // so the checker can find declarations from the merged file
                for &decl_idx in &src_decls {
                    let cloned_arenas: Option<Vec<Arc<NodeArena>>> = declaration_arenas
                        .get(&(source_id, decl_idx))
                        .map(|a| a.iter().cloned().collect());
                    if let Some(arenas) = cloned_arenas {
                        let target = declaration_arenas
                            .entry((existing_id, decl_idx))
                            .or_default();
                        for arena in arenas {
                            target.push(arena);
                        }
                    }
                }
                // Also propagate symbol_arenas if source has one
                let cloned_arena = symbol_arenas.get(&source_id).cloned();
                if let Some(arena) = cloned_arena {
                    symbol_arenas.entry(existing_id).or_insert(arena);
                }
                append_unique_declarations(&mut dst.declarations, &src_decls);
                if dst.value_declaration.is_none() && src_val_decl.is_some() {
                    dst.value_declaration = src_val_decl;
                }
                if let Some(src_exp) = src_exports {
                    let dst_exp = dst
                        .exports
                        .get_or_insert_with(|| Box::new(SymbolTable::with_capacity(src_exp.len())));
                    for (ename, &esym) in src_exp.iter() {
                        if !dst_exp.has(ename) {
                            dst_exp.set(ename.clone(), esym);
                        } else {
                            // Both sides export the same name — queue recursive merge
                            let existing_export_id = dst_exp
                                .get(ename)
                                .expect("else branch guarantees ename exists in dst_exp");
                            if existing_export_id != esym {
                                all_nested_merges.push((existing_export_id, esym));
                            }
                        }
                    }
                }
                if let Some(src_mem) = src_members {
                    let dst_mem = dst
                        .members
                        .get_or_insert_with(|| Box::new(SymbolTable::with_capacity(src_mem.len())));
                    for (mname, &msym) in src_mem.iter() {
                        if !dst_mem.has(mname) {
                            dst_mem.set(mname.clone(), msym);
                        } else {
                            // Both sides have the same member — queue recursive merge
                            let existing_member_id = dst_mem
                                .get(mname)
                                .expect("else branch guarantees mname exists in dst_mem");
                            if existing_member_id != msym {
                                all_nested_merges.push((existing_member_id, msym));
                            }
                        }
                    }
                }
            }
        }

        // Remap node_symbols to use global IDs
        // Note: node_symbols primarily maps user file nodes to user symbols,
        // but lib symbols referenced in user code need remapping too
        let mut remapped_node_symbols = FxHashMap::default();
        for (node_idx, old_sym_id) in result.node_symbols.iter() {
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                remapped_node_symbols.insert(*node_idx, new_sym_id);
            }
            // Note: We don't need to check lib_symbol_remap here because
            // node_symbols are created during binding of user files, and at that point
            // lib symbols are accessed by name lookup (file_locals), not by node mapping
        }

        // Remap file_locals to use global IDs
        // This handles both user symbols (from id_remap) and lib symbols (from lib_name_to_global)
        let mut remapped_file_locals = SymbolTable::with_capacity(result.file_locals.len());
        for (name, old_sym_id) in result.file_locals.iter() {
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                // User symbol - use remapped ID
                remapped_file_locals.set(name.clone(), new_sym_id);
                // Script-file top-levels are globally visible by default. For ordinary
                // external modules, keep pure type-only top-level declarations file-scoped so
                // unimported type aliases/interfaces do not leak across files. Value-bearing
                // exports still stay visible because CommonJS/export-assignment and declaration
                // emit paths rely on them being reachable cross-file.
                let sym_info = global_symbols.get(new_sym_id);
                let is_alias =
                    sym_info.is_some_and(|s| s.flags & crate::binder::symbol_flags::ALIAS != 0);
                let is_umd = sym_info.is_some_and(|s| s.is_umd_export);
                let is_declaration_file = result
                    .arena
                    .source_files
                    .first()
                    .is_some_and(|sf| sf.is_declaration_file);
                // Only top-level VALUE declarations that are actually exported
                // from the module should remain globally visible. Otherwise a
                // module-private const/let/var/function leaks into other files'
                // scopes via `program.globals` seeding of `file_locals`,
                // causing missing TS2304 ("Cannot find name") diagnostics for
                // references that should be unresolved.
                let has_value = sym_info.is_some_and(|s| {
                    s.flags & crate::binder::symbol_flags::VALUE != 0 && s.is_exported
                });
                let is_module_decl = sym_info.is_some_and(|s| {
                    s.flags
                        & (crate::binder::symbol_flags::VALUE_MODULE
                            | crate::binder::symbol_flags::NAMESPACE_MODULE)
                        != 0
                });
                let is_global_augmentation = result.global_augmentations.contains_key(name);
                let is_truly_global = (!is_alias
                    && (!result.is_external_module
                        || is_declaration_file
                        || has_value
                        || is_module_decl))
                    || is_umd
                    || is_global_augmentation;
                if is_truly_global {
                    // UMD namespace exports (`export as namespace Foo`) use
                    // "first in wins" semantics: when multiple modules declare
                    // the same UMD global name, the first one encountered is
                    // kept and subsequent ones are ignored. This matches tsc
                    // behavior. Non-UMD globals can safely overwrite because
                    // they are already merged to a single SymbolId by the
                    // merge phase.
                    if !is_umd || !globals.has(name) {
                        globals.set(name.clone(), new_sym_id);
                    }
                }
            } else {
                let name_atom = name_interner.intern(name);
                if let Some(&global_id) = lib_name_to_global.get(&name_atom) {
                    // Lib symbol - use the pre-remapped global ID
                    // Only add to file_locals, NOT to globals (lib symbols are accessed
                    // through lib_contexts in the checker, not through globals)
                    remapped_file_locals.set(name.clone(), global_id);
                }
            }
        }

        let mut remapped_scopes = Vec::with_capacity(result.scopes.len());
        for scope in result.scopes.iter() {
            let mut table = SymbolTable::with_capacity(scope.table.len());
            for (name, old_sym_id) in scope.table.iter() {
                if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                    // User symbol - include in scope.
                    table.set(name.clone(), new_sym_id);
                } else {
                    let name_atom = name_interner.intern(name);
                    if let Some(&global_id) = lib_name_to_global.get(&name_atom) {
                        // Preserve lib-backed scope entries exactly when they were present in
                        // the original binder. Dropping them during merge weakens same-file
                        // identifier resolution and forces later checker repair.
                        table.set(name.clone(), global_id);
                    }
                }
            }
            remapped_scopes.push(Scope {
                parent: scope.parent,
                table,
                kind: scope.kind,
                container_node: scope.container_node,
            });
        }

        file_locals_list.push(remapped_file_locals);

        let mut remapped_declaration_arenas: DeclarationArenaMap = FxHashMap::default();
        for (&(old_sym_id, decl_idx), arenas) in result.declaration_arenas.iter() {
            if result.lib_symbol_ids.contains(&old_sym_id) {
                continue;
            }
            let has_non_local_arena = arenas
                .iter()
                .any(|arena| !Arc::ptr_eq(arena, &result.arena));
            if !has_non_local_arena {
                continue;
            }
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                remapped_declaration_arenas.insert((new_sym_id, decl_idx), arenas.clone());
            }
        }

        let symbols_with_non_local_declarations: FxHashSet<SymbolId> = remapped_declaration_arenas
            .keys()
            .map(|&(sym_id, _)| sym_id)
            .collect();

        let mut remapped_symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>> = FxHashMap::default();
        for (&old_sym_id, arena) in result.symbol_arenas.iter() {
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                let has_non_local_decl = symbols_with_non_local_declarations.contains(&new_sym_id);
                if has_non_local_decl || !Arc::ptr_eq(arena, &result.arena) {
                    remapped_symbol_arenas.insert(new_sym_id, Arc::clone(arena));
                }
            }
        }

        // Populate arena context for module augmentations
        let module_augmentations: FxHashMap<String, Vec<crate::binder::ModuleAugmentation>> =
            result
                .module_augmentations
                .iter()
                .map(|(spec, augs)| {
                    let arena = Arc::clone(&result.arena);
                    (
                        spec.clone(),
                        augs.iter()
                            .map(|aug| {
                                crate::binder::ModuleAugmentation::with_arena(
                                    aug.name.clone(),
                                    aug.node,
                                    Arc::clone(&arena),
                                )
                            })
                            .collect(),
                    )
                })
                .collect();

        files.push(BoundFile {
            file_name: result.file_name.clone(),
            source_file: result.source_file,
            arena: Arc::clone(&result.arena),
            // Wrap once here. `cross_file_node_symbols` (built later) takes
            // an Arc::clone of `file.node_symbols`, so the underlying
            // `FxHashMap<u32, SymbolId>` is shared via refcount instead of
            // deep-cloned per consumer.
            node_symbols: Arc::new(remapped_node_symbols),
            symbol_arenas: Arc::new(remapped_symbol_arenas),
            declaration_arenas: Arc::new(remapped_declaration_arenas),
            module_declaration_exports_publicly: result.module_declaration_exports_publicly.clone(),
            scopes: Arc::new(remapped_scopes),
            node_scope_ids: result.node_scope_ids.clone(),
            parse_diagnostics: result.parse_diagnostics.clone(),
            global_augmentations: Arc::clone(&result.global_augmentations),
            module_augmentations: Arc::new(module_augmentations),
            augmentation_target_modules: Arc::new(
                result
                    .augmentation_target_modules
                    .iter()
                    .map(|(&old_sym, name)| {
                        let new_sym = id_remap.get(&old_sym).copied().unwrap_or(old_sym);
                        (new_sym, name.clone())
                    })
                    .collect(),
            ),
            flow_nodes: result.flow_nodes.clone(),
            // Arc::clone is O(1); per-file `BoundFile` shares the same
            // `node_flow` map as later `cross_file_*` binder constructions.
            node_flow: Arc::clone(&result.node_flow),
            switch_clause_to_switch: result.switch_clause_to_switch.clone(),
            is_external_module: result.is_external_module,
            expando_properties: remap_expando_properties(&result.expando_properties, &id_remap),
            file_features: result.file_features,
            lib_symbol_reverse_remap: Arc::new(
                result
                    .lib_symbol_reverse_remap
                    .iter()
                    .filter_map(|(&old_sym, &(lib_idx, lib_local_sym))| {
                        id_remap
                            .get(&old_sym)
                            .copied()
                            .map(|new_sym| (new_sym, (lib_idx, lib_local_sym)))
                    })
                    .collect(),
            ),
            semantic_defs: Arc::new(file_semantic_defs),
        });
    }

    // Build cross_file_node_symbols: map each arena pointer to its remapped node_symbols.
    // This enables the checker to resolve type references in cross-file interface declarations.
    // `file.node_symbols` is now `Arc<FxHashMap<...>>`, so cloning the Arc is an
    // O(1) refcount bump that shares the underlying map with the per-file
    // `BoundFile` instead of deep-cloning it.
    for file in &files {
        let arena_ptr = Arc::as_ptr(&file.arena) as usize;
        cross_file_node_symbols.insert(arena_ptr, Arc::clone(&file.node_symbols));
    }

    // Validate skeleton data against legacy merge state before construction.
    // This runs only in debug builds and proves skeleton captures all
    // merge-relevant ambient module topology.
    {
        let user_file_names: FxHashSet<String> =
            files.iter().map(|f| f.file_name.clone()).collect();
        let module_export_keys: FxHashSet<String> = module_exports.keys().cloned().collect();
        skeleton_index.validate_against_merged(
            &declared_modules,
            &shorthand_ambient_modules,
            &module_export_keys,
            &user_file_names,
        );
    }

    // Pre-populate a shared DefinitionStore with DefIds for all semantic definitions.
    // This moves identity creation from the checker's per-file pre-population phase
    // (order-dependent, per-context) to merge time (single pass, deterministic).
    let type_interner = TypeInterner::new();
    let definition_store = std::sync::Arc::new(pre_populate_definition_store(
        &semantic_defs,
        &type_interner,
    ));

    // Patch program-wide `declaration_arenas` so user script-file interface
    // declarations that augment a same-named lib symbol (e.g.
    // `interface Node { kind: SyntaxKind; }` in a script) carry the user
    // file's arena. Phase 2 above appends the user declaration's `NodeIndex`
    // onto the lib symbol's `declarations` list, but the binder's
    // `add_declaration` does not record an arena mapping for it. Without this
    // patch, downstream lookups (e.g. lib-interface re-checks for TS2430) try
    // to read the user `NodeIndex` from the lib's arena and silently miss the
    // augmented members.
    {
        use crate::parser::syntax_kind_ext as sk_ext;
        for (file_idx, result) in results.iter().enumerate() {
            if result.is_external_module {
                continue;
            }
            let Some(source_file) = result.arena.get_source_file_at(result.source_file) else {
                continue;
            };
            let Some(remapped_locals) = file_locals_list.get(file_idx) else {
                continue;
            };
            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = result.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != sk_ext::INTERFACE_DECLARATION {
                    continue;
                }
                let Some(iface) = result.arena.get_interface(stmt_node) else {
                    continue;
                };
                let Some(name_node) = result.arena.get(iface.name) else {
                    continue;
                };
                let Some(ident) = result.arena.get_identifier(name_node) else {
                    continue;
                };
                let name = ident.escaped_text.as_str();
                let sym_id = remapped_locals.get(name).or_else(|| globals.get(name));
                let Some(sym_id) = sym_id else {
                    continue;
                };
                if !global_lib_symbol_ids.contains(&sym_id) {
                    continue;
                }
                let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
                if !target.iter().any(|arena| Arc::ptr_eq(arena, &result.arena)) {
                    target.push(Arc::clone(&result.arena));
                }
            }
        }
    }

    // Build the secondary `sym_to_decl_indices` index over the program-wide
    // `declaration_arenas`. Checker paths that previously iterated every entry
    // filtering by `entry_sym_id == sym_id` use this to do a point lookup.
    let mut sym_to_decl_indices: SymToDeclIndicesMap = FxHashMap::default();
    for &(sym_id, decl_idx) in declaration_arenas.keys() {
        sym_to_decl_indices
            .entry(sym_id)
            .or_default()
            .push(decl_idx);
    }

    MergedProgram {
        files,
        symbols: global_symbols,
        symbol_arenas: Arc::new(symbol_arenas),
        declaration_arenas: Arc::new(declaration_arenas),
        sym_to_decl_indices: Arc::new(sym_to_decl_indices),
        cross_file_node_symbols,
        globals,
        file_locals: file_locals_list,
        declared_modules,
        shorthand_ambient_modules: Arc::new(shorthand_ambient_modules),
        module_exports: Arc::new(module_exports),
        reexports: Arc::new(reexports),
        wildcard_reexports: Arc::new(wildcard_reexports),
        wildcard_reexports_type_only: Arc::new(wildcard_reexports_type_only),
        lib_binders: Arc::new(lib_binders),
        lib_symbol_ids: Arc::new(global_lib_symbol_ids),
        type_interner,
        alias_partners,
        semantic_defs: Arc::new(semantic_defs),
        definition_store,
        skeleton_index: Some(skeleton_index),
        dep_graph: Some(dep_graph),
        pre_merge_bind_total_bytes,
    }
}

/// Full pipeline: Parse → Bind (parallel) → Merge (sequential)
///
/// This is the main entry point for multi-file compilation.
/// Lib files are automatically loaded and merged during binding.
pub fn compile_files(files: Vec<(String, String)>) -> MergedProgram {
    let lib_files = resolve_default_lib_files(ScriptTarget::ESNext)
        .unwrap_or_else(|err| panic!("failed to resolve default lib files: {err}"));
    compile_files_with_libs(files, &lib_files)
}

/// Full pipeline with explicit lib files.
///
/// Callers are responsible for providing the resolved lib file paths.
pub fn compile_files_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[PathBuf],
) -> MergedProgram {
    let lib_paths: Vec<&Path> = lib_files.iter().map(PathBuf::as_path).collect();
    let bind_results = parse_and_bind_parallel_with_lib_files(files, &lib_paths);
    merge_bind_results(bind_results)
}

// =============================================================================
// Parallel Type Checking
// =============================================================================

use crate::checker::context::{CheckerOptions, LibContext};
use crate::checker::diagnostics::Diagnostic;
use crate::checker::state::CheckerState;
use crate::lib_loader::LibFile;
use crate::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Result of type checking a single function body
#[derive(Debug)]
pub struct FunctionCheckResult {
    /// Function node index within its file
    pub function_idx: NodeIndex,
    /// File index in the program
    pub file_idx: usize,
    /// Inferred return type
    pub return_type: TypeId,
    /// Diagnostics produced
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of type checking all function bodies in a file
#[derive(Debug)]
pub struct FileCheckResult {
    /// File index
    pub file_idx: usize,
    /// File name
    pub file_name: String,
    /// Function check results
    pub function_results: Vec<FunctionCheckResult>,
    /// File-level diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

fn collect_lib_interface_node_symbols(
    arena: &NodeArena,
    statements: &[NodeIndex],
    globals: &SymbolTable,
    affected_interfaces: &FxHashSet<String>,
    node_symbols: &mut FxHashMap<u32, SymbolId>,
) {
    for &stmt_idx in statements {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            continue;
        };

        if stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(interface) = arena.get_interface(stmt_node)
                && let Some(name) = arena.get_identifier_at(interface.name)
                && affected_interfaces.contains(&name.escaped_text)
                && let Some(sym_id) = globals.get(&name.escaped_text)
            {
                node_symbols.insert(stmt_idx.0, sym_id);
                node_symbols.insert(interface.name.0, sym_id);
                if let Some(heritage_clauses) = &interface.heritage_clauses {
                    for &clause_idx in &heritage_clauses.nodes {
                        let Some(clause_node) = arena.get(clause_idx) else {
                            continue;
                        };
                        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                            continue;
                        };
                        if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &type_idx in &heritage.types.nodes {
                            let Some(type_node) = arena.get(type_idx) else {
                                continue;
                            };
                            let expr_idx =
                                if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                                    expr_type_args.expression
                                } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                    arena
                                        .get_type_ref(type_node)
                                        .map_or(type_idx, |type_ref| type_ref.type_name)
                                } else {
                                    type_idx
                                };
                            if let Some(base_name) = entity_name_text_in_arena(arena, expr_idx)
                                && let Some(base_sym_id) = globals.get(&base_name)
                            {
                                node_symbols.insert(expr_idx.0, base_sym_id);
                            }
                        }
                    }
                }
            }
            continue;
        }

        if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            continue;
        }

        let Some(module_decl) = arena.get_module(stmt_node) else {
            continue;
        };
        if module_decl.body.is_none() {
            continue;
        }
        let Some(body_node) = arena.get(module_decl.body) else {
            continue;
        };
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            continue;
        }
        let Some(block) = arena.get_module_block(body_node) else {
            continue;
        };
        let Some(inner) = &block.statements else {
            continue;
        };
        collect_lib_interface_node_symbols(
            arena,
            &inner.nodes,
            globals,
            affected_interfaces,
            node_symbols,
        );
    }
}

fn interface_name_text(arena: &NodeArena, stmt_idx: NodeIndex) -> Option<String> {
    let node = arena.get(stmt_idx)?;
    let interface = arena.get_interface(node)?;
    let ident = arena.get_identifier_at(interface.name)?;
    Some(ident.escaped_text.clone())
}

fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        return entity_name_text_in_arena(arena, type_ref.type_name);
    }
    if node.kind == SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }
    if node.kind == syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }
    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }
    None
}

fn collect_direct_base_names(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
) -> Vec<String> {
    let Some(heritage_clauses) = &interface.heritage_clauses else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for &clause_idx in &heritage_clauses.nodes {
        let Some(clause_node) = arena.get(clause_idx) else {
            continue;
        };
        let Some(heritage) = arena.get_heritage_clause(clause_node) else {
            continue;
        };
        if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
            continue;
        }
        for &type_idx in &heritage.types.nodes {
            let Some(type_node) = arena.get(type_idx) else {
                continue;
            };
            let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args(type_node) {
                expr_type_args.expression
            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                arena
                    .get_type_ref(type_node)
                    .map_or(type_idx, |type_ref| type_ref.type_name)
            } else {
                type_idx
            };
            if let Some(name) = entity_name_text_in_arena(arena, expr_idx) {
                names.push(name);
            }
        }
    }
    names
}

fn collect_user_global_interface_seeds(program: &MergedProgram) -> FxHashSet<String> {
    let mut seeds = FxHashSet::default();

    for file in &program.files {
        if !file.is_external_module
            && let Some(source_file) = file.arena.get_source_file_at(file.source_file)
        {
            for &stmt_idx in &source_file.statements.nodes {
                if let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) {
                    seeds.insert(name);
                }
            }
        }

        for name in file.global_augmentations.keys() {
            seeds.insert(name.clone());
        }
    }

    seeds
}

fn member_name_text(arena: &NodeArena, member_idx: NodeIndex) -> Option<String> {
    let member_node = arena.get(member_idx)?;
    if let Some(sig) = arena.get_signature(member_node) {
        return arena
            .get(sig.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    if let Some(accessor) = arena.get_accessor(member_node) {
        return arena
            .get(accessor.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone());
    }
    None
}

fn collect_user_global_interface_member_names(program: &MergedProgram) -> FxHashSet<String> {
    let mut member_names = FxHashSet::default();

    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = file.arena.get_interface(stmt_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                if let Some(name) = member_name_text(file.arena.as_ref(), member_idx) {
                    member_names.insert(name);
                }
            }
        }
    }

    member_names
}

fn add_user_global_interface_declaration_arenas(
    program: &MergedProgram,
    declaration_arenas: &mut DeclarationArenaMap,
) {
    for file in &program.files {
        if file.is_external_module {
            continue;
        }
        let Some(source_file) = file.arena.get_source_file_at(file.source_file) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = interface_name_text(file.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let Some(sym_id) = program.globals.get(&name) else {
                continue;
            };
            let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
            if !target.iter().any(|arena| Arc::ptr_eq(arena, &file.arena)) {
                target.push(Arc::clone(&file.arena));
            }
        }
    }
}

fn type_node_contains_tag_name_map_indexed_access(
    arena: &NodeArena,
    type_idx: NodeIndex,
    fuel: &mut u32,
) -> bool {
    if type_idx == NodeIndex::NONE || *fuel == 0 {
        return false;
    }
    *fuel -= 1;

    let Some(node) = arena.get(type_idx) else {
        return false;
    };
    if node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
        return arena
            .get_indexed_access_type(node)
            .and_then(|indexed| entity_name_text_in_arena(arena, indexed.object_type))
            .is_some_and(|name| name.contains("TagNameMap"));
    }

    if let Some(type_ref) = arena.get_type_ref(node) {
        return type_ref.type_arguments.as_ref().is_some_and(|args| {
            args.nodes
                .iter()
                .any(|&arg| type_node_contains_tag_name_map_indexed_access(arena, arg, fuel))
        });
    }
    if let Some(composite) = arena.get_composite_type(node) {
        return composite
            .types
            .nodes
            .iter()
            .any(|&ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }
    if let Some(array) = arena.get_array_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, array.element_type, fuel);
    }
    if let Some(wrapped) = arena.get_wrapped_type(node) {
        return type_node_contains_tag_name_map_indexed_access(arena, wrapped.type_node, fuel);
    }
    if let Some(type_operator) = arena.get_type_operator(node) {
        return type_node_contains_tag_name_map_indexed_access(
            arena,
            type_operator.type_node,
            fuel,
        );
    }
    if let Some(function_type) = arena.get_function_type(node) {
        if type_node_contains_tag_name_map_indexed_access(
            arena,
            function_type.type_annotation,
            fuel,
        ) {
            return true;
        }
        for &param_idx in &function_type.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            if type_node_contains_tag_name_map_indexed_access(arena, param.type_annotation, fuel) {
                return true;
            }
        }
    }
    if let Some(conditional) = arena.get_conditional_type(node) {
        return [
            conditional.check_type,
            conditional.extends_type,
            conditional.true_type,
            conditional.false_type,
        ]
        .into_iter()
        .any(|ty| type_node_contains_tag_name_map_indexed_access(arena, ty, fuel));
    }

    false
}

fn interface_declares_member_named(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
    member_names: &FxHashSet<String>,
) -> bool {
    !member_names.is_empty()
        && interface.members.nodes.iter().any(|&member_idx| {
            member_name_text(arena, member_idx).is_some_and(|name| member_names.contains(&name))
        })
}

fn interface_has_indexed_access_member_type(
    arena: &NodeArena,
    interface: &crate::parser::node::InterfaceData,
) -> bool {
    for &member_idx in &interface.members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if let Some(sig) = arena.get_signature(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(arena, sig.type_annotation, &mut fuel)
            {
                return true;
            }
            for &param_idx in sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes) {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                let mut fuel = 256;
                if type_node_contains_tag_name_map_indexed_access(
                    arena,
                    param.type_annotation,
                    &mut fuel,
                ) {
                    return true;
                }
            }
        }
        if let Some(accessor) = arena.get_accessor(member_node) {
            let mut fuel = 256;
            if type_node_contains_tag_name_map_indexed_access(
                arena,
                accessor.type_annotation,
                &mut fuel,
            ) {
                return true;
            }
        }
    }

    false
}

fn affected_lib_interface_names(
    program: &MergedProgram,
    checker_lib_files: &[Arc<LibFile>],
) -> FxHashSet<String> {
    let seed_interfaces = collect_user_global_interface_seeds(program);
    let mut affected = seed_interfaces.clone();
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut inheritance_graph: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            let bases = collect_direct_base_names(lib.arena.as_ref(), interface);
            inheritance_graph.entry(name).or_default().extend(bases);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (name, bases) in &inheritance_graph {
            if affected.contains(name) {
                continue;
            }
            if bases.iter().any(|base| affected.contains(base)) {
                changed = affected.insert(name.clone());
            }
        }
    }

    let mut relevant = FxHashSet::default();
    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if !affected.contains(&name) {
                continue;
            }
            if interface_declares_member_named(lib.arena.as_ref(), interface, &user_member_names)
                || interface_has_indexed_access_member_type(lib.arena.as_ref(), interface)
            {
                relevant.insert(name);
            }
        }
    }

    relevant.extend(seed_interfaces);
    let mut ancestor_queue: Vec<String> = relevant.iter().cloned().collect();
    while let Some(name) = ancestor_queue.pop() {
        let Some(bases) = inheritance_graph.get(&name) else {
            continue;
        };
        for base in bases {
            if relevant.insert(base.clone()) {
                ancestor_queue.push(base.clone());
            }
        }
    }

    if relevant.is_empty() {
        affected
    } else {
        relevant
    }
}

fn affected_lib_extension_interface_names(
    program: &MergedProgram,
    checker_lib_files: &[Arc<LibFile>],
    affected_interfaces: &FxHashSet<String>,
) -> FxHashSet<String> {
    let user_member_names = collect_user_global_interface_member_names(program);
    let mut extension_interfaces = FxHashSet::default();

    for lib in checker_lib_files {
        let Some(source_file) = lib.arena.get_source_file_at(lib.root_index) else {
            continue;
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = lib.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = lib.arena.get_interface(stmt_node) else {
                continue;
            };
            let Some(name) = interface_name_text(lib.arena.as_ref(), stmt_idx) else {
                continue;
            };
            if affected_interfaces.contains(&name)
                && interface_declares_member_named(
                    lib.arena.as_ref(),
                    interface,
                    &user_member_names,
                )
            {
                extension_interfaces.insert(name);
            }
        }
    }

    extension_interfaces
}

fn build_lib_bound_file_for_interface_checks(
    program: &MergedProgram,
    lib_file: &Arc<LibFile>,
    affected_interfaces: &FxHashSet<String>,
) -> BoundFile {
    let mut node_symbols = FxHashMap::default();
    if let Some(source_file) = lib_file.arena.get_source_file_at(lib_file.root_index) {
        collect_lib_interface_node_symbols(
            lib_file.arena.as_ref(),
            &source_file.statements.nodes,
            &program.globals,
            affected_interfaces,
            &mut node_symbols,
        );
    }

    // Deep-clone the program-wide `declaration_arenas` into a mutable map so
    // we can add user-global-interface entries below. `program.declaration_arenas`
    // is `Arc`-shared; dereferencing before `.clone()` produces an owned inner
    // map without disturbing the shared data.
    let mut declaration_arenas: DeclarationArenaMap = (*program.declaration_arenas).clone();
    add_user_global_interface_declaration_arenas(program, &mut declaration_arenas);

    BoundFile {
        file_name: lib_file.file_name.clone(),
        source_file: lib_file.root_index,
        arena: Arc::clone(&lib_file.arena),
        node_symbols: Arc::new(node_symbols),
        symbol_arenas: Arc::clone(&program.symbol_arenas),
        declaration_arenas: Arc::new(declaration_arenas),
        module_declaration_exports_publicly: Arc::new(FxHashMap::default()),
        scopes: Arc::new(Vec::new()),
        node_scope_ids: Arc::new(FxHashMap::default()),
        parse_diagnostics: Vec::new(),
        global_augmentations: Arc::new(FxHashMap::default()),
        module_augmentations: Arc::new(FxHashMap::default()),
        augmentation_target_modules: Arc::new(FxHashMap::default()),
        flow_nodes: Arc::new(FlowNodeArena::default()),
        node_flow: Arc::new(FxHashMap::default()),
        switch_clause_to_switch: Arc::new(FxHashMap::default()),
        is_external_module: lib_file.binder.is_external_module,
        expando_properties: Arc::new(FxHashMap::default()),
        file_features: crate::binder::FileFeatures::NONE,
        lib_symbol_reverse_remap: Arc::new(FxHashMap::default()),
        semantic_defs: Arc::new(FxHashMap::default()),
    }
}

/// Result of parallel type checking
#[derive(Debug)]
pub struct CheckResult {
    /// Per-file check results
    pub file_results: Vec<FileCheckResult>,
    /// Total functions checked
    pub function_count: usize,
    /// Total diagnostics
    pub diagnostic_count: usize,
}

fn suppress_parallel_ts2339_cascade_diagnostics(
    arena: &NodeArena,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ts2454_starts: FxHashSet<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2454)
        .map(|diag| diag.start)
        .collect();
    if ts2454_starts.is_empty() {
        return;
    }

    let ts2339_starts: FxHashSet<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2339)
        .map(|diag| diag.start)
        .collect();
    if ts2339_starts.is_empty() {
        return;
    }

    let mut suppressed_ts2339_starts = FxHashSet::default();
    for raw_idx in 0..arena.len() {
        let idx = NodeIndex(raw_idx as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            continue;
        }

        let Some(access) = arena.get_access_expr(node) else {
            continue;
        };
        let Some(name_node) = arena.get(access.name_or_argument) else {
            continue;
        };
        if !ts2339_starts.contains(&name_node.pos) {
            continue;
        }

        let receiver_start = arena.get(access.expression).map(|expr| expr.pos);
        if !receiver_start.is_some_and(|start| ts2454_starts.contains(&start)) {
            continue;
        }

        let Some(ext) = arena.get_extended(idx) else {
            continue;
        };
        let parent = ext.parent;
        let Some(parent_node) = arena.get(parent) else {
            continue;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            continue;
        }

        let Some(var_decl) = arena.get_variable_declaration_at(parent) else {
            continue;
        };
        if var_decl.initializer != idx {
            continue;
        }

        suppressed_ts2339_starts.insert(name_node.pos);
    }

    diagnostics
        .retain(|diag| !(diag.code == 2339 && suppressed_ts2339_starts.contains(&diag.start)));
}

/// Collect all function declarations from a source file
fn collect_functions(arena: &NodeArena, source_file: NodeIndex) -> Vec<NodeIndex> {
    let mut functions = Vec::new();

    let Some(sf) = arena.get_source_file_at(source_file) else {
        return functions;
    };

    for &stmt_idx in &sf.statements.nodes {
        collect_functions_from_node(arena, stmt_idx, &mut functions);
    }

    functions
}

/// Recursively collect functions from a node
fn collect_functions_from_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    functions: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };

    match node.kind {
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION =>
        {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(func) = arena.get_function(node)
                && func.body.is_some()
            {
                collect_functions_from_node(arena, func.body, functions);
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(method) = arena.get_method_decl(node)
                && method.body.is_some()
            {
                collect_functions_from_node(arena, method.body, functions);
            }
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(node) {
                for &member_idx in &class.members.nodes {
                    collect_functions_from_node(arena, member_idx, functions);
                }
            }
        }
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    collect_functions_from_node(arena, stmt_idx, &mut *functions);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            // Variable statement contains a declaration list which contains declarations
            if let Some(var_stmt) = arena.get_variable(node) {
                // var_stmt.declarations contains the VARIABLE_DECLARATION_LIST node(s)
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = arena.get(decl_list_idx) {
                        // The declaration list also uses VariableData
                        if let Some(decl_list) = arena.get_variable(decl_list_node) {
                            // Now decl_list.declarations contains the actual VARIABLE_DECLARATION nodes
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = arena.get(decl_idx)
                                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                                    && decl.initializer.is_some()
                                {
                                    collect_functions_from_node(arena, decl.initializer, functions);
                                }
                            }
                        }
                    }
                }
            }
        }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            // Export declarations may contain function/class declarations
            if let Some(export) = arena.get_export_decl(node)
                && export.export_clause.is_some()
            {
                collect_functions_from_node(arena, export.export_clause, functions);
            }
        }
        _ => {}
    }
}

/// Type check function bodies in parallel
///
/// After binding is complete and symbols are merged, function bodies
/// can be type-checked in parallel because:
/// 1. Each function body only uses local variables and global symbols
/// 2. Local type inference doesn't modify global state
/// 3. Each function is independent
///
/// # Arguments
/// * `program` - The merged program with global symbols
///
/// # Returns
/// `CheckResult` with diagnostics from all functions
pub fn check_functions_parallel(program: &MergedProgram) -> CheckResult {
    ensure_rayon_global_pool();

    let file_names: Vec<String> = program
        .files
        .iter()
        .map(|file| file.file_name.clone())
        .collect();
    let (resolved_module_paths, resolved_modules) =
        crate::checker::module_resolution::build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);

    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| Arc::new(create_binder_from_bound_file(file, program, file_idx)))
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    // PERF: Build arena-pointer -> file-index reverse lookup map first (O(F)),
    // then map each symbol to its file index in O(1) per symbol.
    // Total: O(S + F) instead of the previous O(S * F) nested iteration.
    let arena_to_file_idx: FxHashMap<usize, usize> = all_arenas
        .iter()
        .enumerate()
        .map(|(idx, arena)| (Arc::as_ptr(arena) as usize, idx))
        .collect();
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            arena_to_file_idx
                .get(&(Arc::as_ptr(arena) as usize))
                .map(|&file_idx| (*sym_id, file_idx))
        })
        .collect();

    // Pre-compute the symbol->file index as a shared read-only map.
    // Each checker gets an Arc clone (O(1)) instead of O(N) per-checker insertion.
    let global_symbol_file_index: Arc<FxHashMap<tsz_binder::SymbolId, usize>> = Arc::new(
        symbol_file_targets
            .iter()
            .copied()
            .collect::<FxHashMap<_, _>>(),
    );

    // First, collect all functions from all files (sequential)
    let mut all_functions: Vec<(usize, NodeIndex)> = Vec::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        let functions = collect_functions(&file.arena, file.source_file);
        for func_idx in functions {
            all_functions.push((file_idx, func_idx));
        }
    }

    let function_count = all_functions.len();

    // Check functions in parallel
    // Note: We need to be careful here - CheckerState holds mutable references
    // For now, we group by file and check each file's functions together
    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let functions = collect_functions(&file.arena, file.source_file);

            let binder = Arc::clone(&shared_binders[file_idx]);

            // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
            // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
            let query_cache = tsz_solver::QueryCache::new(&program.type_interner);

            // Create checker for this file, using the shared type interner
            let compiler_options = crate::checker::context::CheckerOptions::default();
            let mut checker = CheckerState::new_with_shared_def_store(
                &file.arena,
                binder.as_ref(),
                &query_cache,
                file.file_name.clone(),
                compiler_options, // default options for internal operations
                std::sync::Arc::clone(&program.definition_store),
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);
            checker
                .ctx
                .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
            checker.ctx.set_resolved_modules(resolved_modules.clone());
            checker
                .ctx
                .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

            let mut function_results = Vec::new();

            for func_idx in functions {
                // Check the function
                let return_type = checker.get_type_of_node(func_idx);

                function_results.push(FunctionCheckResult {
                    function_idx: func_idx,
                    file_idx,
                    return_type,
                    diagnostics: Vec::new(), // Diagnostics are collected at file level
                });
            }

            // Collect diagnostics from checker
            let diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

            FileCheckResult {
                file_idx,
                file_name: file.file_name.clone(),
                function_results,
                diagnostics,
            }
        })
        .collect();

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count,
        diagnostic_count,
    }
}

/// Type check full source files in parallel using Rayon.
///
/// Each file gets its own `CheckerState` with file-local mutable state, sharing
/// only thread-safe structures (`Arc`-wrapped arenas/binders, `DashMap`-backed
/// `TypeInterner` and `DefinitionStore`). Per-thread `QueryCache` instances use
/// `RefCell`/`Cell` for zero-overhead single-threaded caching within each file.
///
/// Diagnostics are sorted by `(start, code)` within each file and deduplicated
/// by `(start, code)` after collection, ensuring deterministic output regardless
/// of thread scheduling.
pub fn check_files_parallel(
    program: &MergedProgram,
    checker_options: &CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> CheckResult {
    // Ensure Rayon global pool has adequate stack size for deep type-checking recursion.
    ensure_rayon_global_pool();

    let file_names: Vec<String> = program
        .files
        .iter()
        .map(|file| file.file_name.clone())
        .collect();
    let (resolved_module_paths, resolved_modules) =
        crate::checker::module_resolution::build_module_resolution_maps(&file_names);
    let resolved_module_paths = Arc::new(resolved_module_paths);

    let checker_lib_files = clone_lib_files_for_checker(lib_files);

    // Create fresh checker lib contexts from cloned lib files (contains both arena and binder).
    // Wrapped in Arc so that per-file checkers and child delegations share
    // the same Vec with O(1) clone cost (single atomic refcount increment).
    let lib_contexts: Arc<Vec<LibContext>> = Arc::new(
        checker_lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect(),
    );

    // PERF: Pre-compute merged augmentation data ONCE instead of per-file.
    // This reduces augmentation merging from O(N_files^2) to O(N_files).
    let shared_binder_data = SharedBinderData::from_program(&program.files);

    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| {
            Arc::new(create_binder_from_bound_file_with_shared(
                file,
                program,
                file_idx,
                &shared_binder_data,
            ))
        })
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    // PERF: Build arena-pointer -> file-index reverse lookup map first (O(F)),
    // then map each symbol to its file index in O(1) per symbol.
    // Total: O(S + F) instead of the previous O(S * F) nested iteration.
    let arena_to_file_idx: FxHashMap<usize, usize> = all_arenas
        .iter()
        .enumerate()
        .map(|(idx, arena)| (Arc::as_ptr(arena) as usize, idx))
        .collect();
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            arena_to_file_idx
                .get(&(Arc::as_ptr(arena) as usize))
                .map(|&file_idx| (*sym_id, file_idx))
        })
        .collect();

    // Pre-compute the symbol->file index as a shared read-only map.
    // Each checker gets an Arc clone (O(1)) instead of O(N) per-checker insertion.
    let global_symbol_file_index: Arc<FxHashMap<tsz_binder::SymbolId, usize>> = Arc::new(
        symbol_file_targets
            .iter()
            .copied()
            .collect::<FxHashMap<_, _>>(),
    );

    // Pre-compute skeleton-derived declared modules ONCE and share via Arc.
    // Previously this was computed per-file inside the closure, rebuilding the
    // same FxHashSet/Vec on every file (O(N_files * N_modules) total work).
    let shared_declared_modules: Option<Arc<crate::checker::context::GlobalDeclaredModules>> =
        program.skeleton_index.as_ref().map(|skel| {
            let (exact, patterns) = skel.build_declared_module_sets();
            Arc::new(crate::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns))
        });

    // Initialize per-file delegation locks for parallel correctness.
    program
        .definition_store
        .init_file_locks(program.files.len());

    // Create a shared cross-file query cache for multi-file projects.
    // In projects like ts-toolbelt (242 files), the same type evaluations and
    // subtype checks are performed across many files. The shared cache uses
    // DashMap for thread-safe concurrent access and eliminates redundant
    // computation across parallel file checkers.
    let shared_query_cache = if program.files.len() > 1 {
        Some(tsz_solver::SharedQueryCache::new())
    } else {
        None
    };

    // Closure that checks a single file and returns its result.
    // Extracted so both sequential and parallel paths use identical logic.
    let check_one_file = |file_idx: usize, file: &BoundFile| -> FileCheckResult {
        let binder = Arc::clone(&shared_binders[file_idx]);

        // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
        // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
        // For multi-file projects, the shared cache provides L2 cross-file caching.
        let query_cache = if let Some(ref shared) = shared_query_cache {
            tsz_solver::QueryCache::new_with_shared(&program.type_interner, shared)
        } else {
            tsz_solver::QueryCache::new(&program.type_interner)
        };

        let mut checker = CheckerState::with_options_and_shared_def_store(
            &file.arena,
            binder.as_ref(),
            &query_cache,
            file.file_name.clone(),
            checker_options,
            std::sync::Arc::clone(&program.definition_store),
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));

        // Use pre-computed skeleton-derived declared modules (shared via Arc::clone).
        if let Some(ref modules) = shared_declared_modules {
            checker
                .ctx
                .set_declared_modules_from_skeleton(Arc::clone(modules));
        }

        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(file_idx);
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker
            .ctx
            .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

        if !lib_contexts.is_empty() {
            checker
                .ctx
                .set_lib_contexts_shared(Arc::clone(&lib_contexts));
            checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        }

        checker.check_source_file(file.source_file);

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

        // Sort diagnostics by position for deterministic output within each file.
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));

        suppress_parallel_ts2339_cascade_diagnostics(file.arena.as_ref(), &mut diagnostics);

        // Deduplicate within each file: same (start, code) = same diagnostic.
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx,
            file_name: file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let affected_lib_interfaces = affected_lib_interface_names(program, &checker_lib_files);
    let affected_lib_extension_interfaces = affected_lib_extension_interface_names(
        program,
        &checker_lib_files,
        &affected_lib_interfaces,
    );

    let check_one_lib = |lib_idx: usize, lib_file: &Arc<LibFile>| -> FileCheckResult {
        let query_cache = if let Some(ref shared) = shared_query_cache {
            tsz_solver::QueryCache::new_with_shared(&program.type_interner, shared)
        } else {
            tsz_solver::QueryCache::new(&program.type_interner)
        };

        let lib_bound_file =
            build_lib_bound_file_for_interface_checks(program, lib_file, &affected_lib_interfaces);
        let mut binder =
            create_binder_from_bound_file(&lib_bound_file, program, program.files.len());
        // PERF: `build_lib_bound_file_for_interface_checks` always seeds
        // `lib_bound_file.semantic_defs` as empty, so the previous
        // clone-then-overlay collapsed to a deep clone of `program.semantic_defs`
        // and Arc-wrapping the result. With `program.semantic_defs` now
        // `Arc`-shared, the fast path is one atomic refcount bump plus a
        // potential `Arc::make_mut` only when the per-lib map actually
        // contributes entries (currently never).
        if lib_bound_file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in lib_bound_file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }

        let mut checker = CheckerState::with_options(
            &lib_bound_file.arena,
            &binder,
            &query_cache,
            lib_bound_file.file_name.clone(),
            checker_options,
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        if let Some(ref modules) = shared_declared_modules {
            checker
                .ctx
                .set_declared_modules_from_skeleton(Arc::clone(modules));
        }
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker
            .ctx
            .set_resolved_module_paths(Arc::clone(&resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules.clone());
        checker
            .ctx
            .set_global_symbol_file_index(Arc::clone(&global_symbol_file_index));

        let other_lib_contexts: Vec<LibContext> = lib_contexts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != lib_idx)
            .map(|(_, ctx)| ctx.clone())
            .collect();
        checker.ctx.set_lib_contexts(other_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.prime_boxed_types();

        checker.check_source_file_interfaces_only_filtered_post_merge(
            lib_bound_file.source_file,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
        );

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx: program.files.len() + lib_idx,
            file_name: lib_file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let check_one_lib_baseline = |lib_idx: usize, lib_file: &Arc<LibFile>| -> FileCheckResult {
        let query_cache = tsz_solver::QueryCache::new(&program.type_interner);

        let mut checker = CheckerState::with_options(
            &lib_file.arena,
            lib_file.binder.as_ref(),
            &query_cache,
            lib_file.file_name.clone(),
            checker_options,
        );
        let other_lib_contexts: Vec<LibContext> = lib_contexts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != lib_idx)
            .map(|(_, ctx)| ctx.clone())
            .collect();
        checker.ctx.set_lib_contexts(other_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_contexts.len());
        checker.prime_boxed_types();
        checker.check_source_file_interfaces_only_filtered_post_merge(
            lib_file.root_index,
            &affected_lib_interfaces,
            &affected_lib_extension_interfaces,
        );

        let mut diagnostics = std::mem::take(&mut checker.ctx.diagnostics);
        diagnostics.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.code.cmp(&b.code)));
        diagnostics.dedup_by(|a, b| a.start == b.start && a.code == b.code);

        FileCheckResult {
            file_idx: program.files.len() + lib_idx,
            file_name: lib_file.file_name.clone(),
            function_results: Vec::new(),
            diagnostics,
        }
    };

    let fingerprint = |file_name: &str, diag: &Diagnostic| {
        (
            file_name.to_owned(),
            diag.start,
            diag.code,
            diag.message_text.clone(),
        )
    };
    let baseline_lib_diagnostics: FxHashSet<(String, u32, u32, String)> = checker_lib_files
        .iter()
        .enumerate()
        .flat_map(|(lib_idx, lib_file)| {
            let file_result = check_one_lib_baseline(lib_idx, lib_file);
            let file_name = file_result.file_name.clone();
            file_result
                .diagnostics
                .into_iter()
                .map(move |diag| fingerprint(&file_name, &diag))
        })
        .collect();

    // Single-file optimization: skip Rayon overhead when there's only one file.
    // For multi-file projects, use parallel iteration via Rayon's work-stealing
    // scheduler. `par_iter().enumerate()` preserves input ordering (file_idx) so
    // results are deterministic regardless of which thread completes first.
    let mut file_results: Vec<FileCheckResult> = if program.files.len() <= 1 {
        program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| check_one_file(file_idx, file))
            .collect()
    } else {
        maybe_parallel_iter!(program.files)
            .enumerate()
            .map(|(file_idx, file)| check_one_file(file_idx, file))
            .collect()
    };

    file_results.extend(
        checker_lib_files
            .iter()
            .enumerate()
            .map(|(lib_idx, lib_file)| {
                let mut file_result = check_one_lib(lib_idx, lib_file);
                let file_name = file_result.file_name.clone();
                file_result.diagnostics.retain(|diag| {
                    !baseline_lib_diagnostics.contains(&fingerprint(&file_name, diag))
                });
                file_result
            }),
    );

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count: 0,
        diagnostic_count,
    }
}

/// Pre-computed data shared across all file binders in a parallel check.
///
/// These are computed ONCE from the program's files and shared via Arc,
/// eliminating `O(N_files^2)` redundant iteration in `create_binder_from_bound_file()`.
pub struct SharedBinderData {
    /// Merged module augmentations from all files.
    pub merged_module_augmentations:
        rustc_hash::FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>,
    /// Merged augmentation target modules from all files.
    pub merged_augmentation_target_modules: rustc_hash::FxHashMap<crate::binder::SymbolId, String>,
    /// Merged global augmentations from all files.
    pub merged_global_augmentations:
        rustc_hash::FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>,
}

impl SharedBinderData {
    /// Build shared binder data from all files in one pass.
    pub fn from_program(files: &[BoundFile]) -> Self {
        let mut merged_module_augmentations = rustc_hash::FxHashMap::default();
        let mut merged_augmentation_target_modules = rustc_hash::FxHashMap::default();
        let mut merged_global_augmentations = rustc_hash::FxHashMap::default();

        for file in files {
            for (spec, augs) in file.module_augmentations.iter() {
                merged_module_augmentations
                    .entry(spec.clone())
                    .or_insert_with(Vec::new)
                    .extend(augs.iter().map(|aug| {
                        crate::binder::ModuleAugmentation::with_arena(
                            aug.name.clone(),
                            aug.node,
                            Arc::clone(&file.arena),
                        )
                    }));
            }
            for (&sym_id, module_spec) in file.augmentation_target_modules.iter() {
                merged_augmentation_target_modules.insert(sym_id, module_spec.clone());
            }
            for (name, decls) in file.global_augmentations.iter() {
                merged_global_augmentations
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .extend(decls.iter().map(|aug| {
                        crate::binder::GlobalAugmentation::with_arena(
                            aug.node,
                            Arc::clone(&file.arena),
                            aug.flags,
                        )
                    }));
            }
        }

        Self {
            merged_module_augmentations,
            merged_augmentation_target_modules,
            merged_global_augmentations,
        }
    }
}

/// Create a `BinderState` from a `BoundFile` for type checking.
///
/// This path is retained for tsz-core callers that want the legacy per-file
/// subset of `declaration_arenas` (only non-local, non-lib-originated entries,
/// as captured in `BoundFile.declaration_arenas`). The CLI driver uses its own
/// path (`create_binder_from_bound_file_with_augmentations`) which shares the
/// program-wide map via `Arc::clone` — see the perf follow-up doc §3.2.
pub fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    let declaration_arenas = Arc::clone(&file.declaration_arenas);
    // The per-file subset is small; build a local `sym_to_decl_indices` from it
    // so consumers that go through the secondary index still see the same set.
    let mut sym_to_decl_indices_local: SymToDeclIndicesMap = FxHashMap::default();
    for &(sym_id, decl_idx) in declaration_arenas.keys() {
        sym_to_decl_indices_local
            .entry(sym_id)
            .or_default()
            .push(decl_idx);
    }
    let sym_to_decl_indices = Arc::new(sym_to_decl_indices_local);
    let symbol_arenas = Arc::clone(&file.symbol_arenas);

    // Get file locals for this specific file
    let mut file_locals = SymbolTable::new();

    // Copy from program.file_locals if available
    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Also add globals (for cross-file references)
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1); cross-file lookup binders share the per-file
        // map by reference instead of deep-cloning it.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: Arc::clone(&file.global_augmentations),
            module_augmentations: Arc::clone(&file.module_augmentations),
            augmentation_target_modules: Arc::clone(&file.augmentation_target_modules),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols: program.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: FxHashSet::default(),
            flow_nodes: file.flow_nodes.clone(),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // node_flow map by reference instead of deep-cloning it.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();

    // Compose semantic_defs: start with the global map (cross-file + lib entries)
    // then overlay the file's own entries. Per-file entries take precedence for
    // symbols declared in this file, ensuring file-scoped identity is authoritative.
    //
    // PERF: When the shared DefinitionStore is fully populated (parallel path),
    // semantic_defs are never read by the checker (warm_local_caches_from_shared_store
    // and resolve_cross_batch_heritage both skip when fully_populated=true).
    // Skip the expensive clone+overlay to avoid O(files * total_defs) work.
    if !program.definition_store.is_fully_populated() {
        if file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }
    }
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = crate::binder::ScopeId(0);
    }

    binder.declared_modules = program.declared_modules.clone();

    // Mark lib symbols as merged since the MergedProgram's symbol arena
    // contains all remapped lib symbols with unique global IDs.
    // This enables the fast path in get_symbol() that avoids cross-binder lookups.
    binder.set_lib_symbols_merged(true);

    binder
}

/// Create a `BinderState` from a `BoundFile` using pre-computed shared augmentation data.
///
/// This avoids the `O(N_files)` augmentation merge per file by reusing data computed once
/// via `SharedBinderData::from_program`. For ts-toolbelt (242 files), this eliminates
/// ~242 * 242 = 58,564 redundant augmentation iterations.
pub fn create_binder_from_bound_file_with_shared(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
    _shared: &SharedBinderData,
) -> BinderState {
    // Keep the legacy per-file subset behavior here (see `create_binder_from_bound_file`):
    // these paths are used by `check_files_parallel` and tests that expect the
    // binder's `declaration_arenas` to exclude lib-originated symbols.
    let declaration_arenas = Arc::clone(&file.declaration_arenas);
    let mut sym_to_decl_indices_local: SymToDeclIndicesMap = FxHashMap::default();
    for &(sym_id, decl_idx) in declaration_arenas.keys() {
        sym_to_decl_indices_local
            .entry(sym_id)
            .or_default()
            .push(decl_idx);
    }
    let sym_to_decl_indices = Arc::new(sym_to_decl_indices_local);
    let symbol_arenas = Arc::clone(&file.symbol_arenas);

    let mut file_locals = SymbolTable::new();
    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        // Arc::clone is O(1); cross-file lookup binders share the per-file
        // map by reference instead of deep-cloning it.
        Arc::clone(&file.node_symbols),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: Arc::clone(&file.global_augmentations),
            module_augmentations: Arc::clone(&file.module_augmentations),
            augmentation_target_modules: Arc::clone(&file.augmentation_target_modules),
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas,
            declaration_arenas,
            sym_to_decl_indices,
            cross_file_node_symbols: program.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: FxHashSet::default(),
            flow_nodes: file.flow_nodes.clone(),
            // Arc::clone is O(1); cross-file lookup binders share the per-file
            // node_flow map by reference instead of deep-cloning it.
            node_flow: Arc::clone(&file.node_flow),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.lib_symbol_reverse_remap = file.lib_symbol_reverse_remap.clone();
    binder.lib_binders = program.lib_binders.clone();
    binder.lib_symbol_ids = program.lib_symbol_ids.clone();

    if !program.definition_store.is_fully_populated() {
        if file.semantic_defs.is_empty() {
            binder.semantic_defs = Arc::clone(&program.semantic_defs);
        } else {
            let mut composed_semantic_defs = (*program.semantic_defs).clone();
            for (sym_id, entry) in file.semantic_defs.iter() {
                composed_semantic_defs.insert(*sym_id, entry.clone());
            }
            binder.semantic_defs = Arc::new(composed_semantic_defs);
        }
    }
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = crate::binder::ScopeId(0);
    }

    binder.declared_modules = program.declared_modules.clone();
    binder.set_lib_symbols_merged(true);

    binder
}

/// Check function bodies with statistics
pub fn check_functions_with_stats(program: &MergedProgram) -> (CheckResult, CheckStats) {
    let result = check_functions_parallel(program);

    let stats = CheckStats {
        file_count: result.file_results.len(),
        function_count: result.function_count,
        diagnostic_count: result.diagnostic_count,
    };

    (result, stats)
}

/// Statistics about parallel type checking
#[derive(Debug, Clone)]
pub struct CheckStats {
    /// Number of files checked
    pub file_count: usize,
    /// Number of functions checked
    pub function_count: usize,
    /// Number of diagnostics produced
    pub diagnostic_count: usize,
}

/// Parse files and collect statistics
pub fn parse_files_with_stats(files: Vec<(String, String)>) -> (Vec<ParseResult>, ParallelStats) {
    let total_bytes: usize = files.iter().map(|(_, src)| src.len()).sum();
    let file_count = files.len();

    let results = parse_files_parallel(files);

    let total_nodes: usize = results.iter().map(|r| r.arena.len()).sum();
    let error_count: usize = results.iter().map(|r| r.parse_diagnostics.len()).sum();

    let stats = ParallelStats {
        file_count,
        total_bytes,
        total_nodes,
        error_count,
    };

    (results, stats)
}

#[cfg(test)]
#[path = "../../tests/parallel_tests.rs"]
mod tests;
