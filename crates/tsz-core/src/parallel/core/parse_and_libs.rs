type ModuleExportEntry = FxHashMap<String, (String, Option<String>)>;
type Reexports = FxHashMap<String, ModuleExportEntry>;

enum LibSourceText {
    Owned(String),
    Static {
        text: &'static str,
        content_hash: u64,
    },
}

impl LibSourceText {
    fn as_str(&self) -> &str {
        match self {
            Self::Owned(text) => text,
            Self::Static { text, .. } => text,
        }
    }

    fn len(&self) -> usize {
        self.as_str().len()
    }
}

pub(crate) fn build_sym_to_decl_indices(
    declaration_arenas: &DeclarationArenaMap,
) -> SymToDeclIndicesMap {
    let mut sym_to_decl_indices: SymToDeclIndicesMap = FxHashMap::default();
    for &(sym_id, decl_idx) in declaration_arenas.keys() {
        sym_to_decl_indices
            .entry(sym_id)
            .or_default()
            .push(decl_idx);
    }
    sym_to_decl_indices
}

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

#[cfg(not(target_arch = "wasm32"))]
const SMALL_WORKLOAD_RAYON_MAX_ITEMS: usize = 32;
#[cfg(not(target_arch = "wasm32"))]
const SMALL_WORKLOAD_RAYON_THREADS: usize = 4;

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
        let builder =
            rayon::ThreadPoolBuilder::new().stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES);
        let _ = builder.build_global();
    });
}

/// Run work on a scoped Rayon pool sized for a known source workload.
///
/// Tiny generated app projects have enough independent parse/bind work to
/// benefit from Rayon, but on high-core machines a full-width pool spends
/// disproportionate time in worker startup and scheduler/system overhead. Use
/// a scoped local pool for this small-workload regime so the process-global
/// Rayon pool remains available at its default width for later larger projects.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_with_rayon_pool_for_work_items<R>(
    work_item_count: usize,
    f: impl FnOnce() -> R + Send,
) -> R
where
    R: Send,
{
    let worker_count = rayon_worker_count_for_work_items(
        work_item_count,
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1),
        std::env::var_os("RAYON_NUM_THREADS").is_some(),
    );
    if let Some(worker_count) = worker_count
        && let Ok(pool) = rayon::ThreadPoolBuilder::new()
            .stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES)
            .num_threads(worker_count)
            .build()
    {
        return pool.install(f);
    }

    ensure_rayon_global_pool();
    f()
}

#[cfg(not(target_arch = "wasm32"))]
fn rayon_worker_count_for_work_items(
    work_item_count: usize,
    available_parallelism: usize,
    env_override_set: bool,
) -> Option<usize> {
    if env_override_set || work_item_count == 0 || work_item_count > SMALL_WORKLOAD_RAYON_MAX_ITEMS
    {
        return None;
    }

    Some(available_parallelism.clamp(1, SMALL_WORKLOAD_RAYON_THREADS))
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_rayon_global_pool() {}

#[cfg(target_arch = "wasm32")]
pub fn run_with_rayon_pool_for_work_items<R>(_work_item_count: usize, f: impl FnOnce() -> R) -> R {
    f()
}

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
    pub declared_modules: Arc<FxHashSet<String>>,
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
    pub alias_partners: Arc<FxHashMap<SymbolId, SymbolId>>,
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
        for s in self.declared_modules.iter() {
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
    let mut file_contents: Vec<(String, LibSourceText)> = Vec::new();
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

    let snapshot_keys: Vec<(&str, u64)> = file_contents
        .iter()
        .map(snapshot_key_for_lib_source)
        .collect();
    if let Some(cached) = super::lib_snapshot::try_load_many(&snapshot_keys) {
        return Ok(cached);
    }
    let snapshot_keys_for_store: Vec<(String, u64)> = snapshot_keys
        .iter()
        .map(|(file_name, content_hash)| ((*file_name).to_string(), *content_hash))
        .collect();
    drop(snapshot_keys);

    // Collect results, propagating any parse errors
    let results: Vec<Arc<lib_loader::LibFile>> = parse_and_bind_lib_files(file_contents)
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let snapshot_keys_for_store: Vec<(&str, u64)> = snapshot_keys_for_store
        .iter()
        .map(|(file_name, content_hash)| (file_name.as_str(), *content_hash))
        .collect();
    if let Err(err) = super::lib_snapshot::try_store_many(&snapshot_keys_for_store, &results) {
        tracing::debug!(
            target: "wasm::lib_snapshot",
            error = %err,
            "lib snapshot set write failed (compilation continues normally)",
        );
    }
    Ok(results)
}

fn parse_and_bind_lib_files(
    file_contents: Vec<(String, LibSourceText)>,
) -> Vec<Result<Arc<lib_loader::LibFile>>> {
    #[cfg(target_arch = "wasm32")]
    {
        return file_contents
            .into_iter()
            .map(|(file_name, source_text)| parse_and_bind_lib_source(file_name, source_text))
            .collect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let worker_count = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            .min(8)
            .min(file_contents.len().max(1));
        if worker_count <= 1 {
            return file_contents
                .into_iter()
                .map(|(file_name, source_text)| parse_and_bind_lib_source(file_name, source_text))
                .collect();
        }

        match rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .stack_size(tsz_common::limits::THREAD_STACK_SIZE_BYTES)
            .build()
        {
            Ok(pool) => pool.install(|| {
                file_contents
                    .into_par_iter()
                    .map(|(file_name, source_text)| {
                        parse_and_bind_lib_source(file_name, source_text)
                    })
                    .collect()
            }),
            Err(_) => file_contents
                .into_par_iter()
                .map(|(file_name, source_text)| parse_and_bind_lib_source(file_name, source_text))
                .collect(),
        }
    }
}

/// Clone lib files into fresh checker-only binders.
///
/// The binders used during program construction are mutated while merging lib symbols into
/// user-file binders. Checker-facing lib contexts and lib-file checks need fresh binder state
/// so declaration merging and semantic lookups run against clean lib binders.
///
/// The clone needs an independent parsed + bound copy of every lib file, but
/// the source lib files have already been loaded into clean parsed/bound state.
/// Deep-cloning that state in memory preserves distinct arena/binder identity
/// for checker resolution while avoiding a second pass through the disk-backed
/// lib snapshot cache. Output order matches input order via rayon's
/// order-preserving `collect`.
#[must_use]
pub fn clone_lib_files_for_checker(
    lib_files: &[Arc<lib_loader::LibFile>],
    should_clone_libs_in_parallel: bool,
) -> Vec<Arc<lib_loader::LibFile>> {
    let clone_lib_file = |lib: &Arc<lib_loader::LibFile>| {
        let mut binder = (*lib.binder).clone();
        binder.clear_resolution_caches();
        Arc::new(lib_loader::LibFile::new(
            lib.file_name.clone(),
            Arc::new((*lib.arena).clone()),
            Arc::new(binder),
            lib.root_index,
        ))
    };

    if should_clone_libs_in_parallel {
        #[cfg(not(target_arch = "wasm32"))]
        {
            ensure_rayon_global_pool();
            return maybe_parallel_iter!(lib_files)
                .map(clone_lib_file)
                .collect();
        }
        #[cfg(target_arch = "wasm32")]
        {
            return lib_files.iter().map(clone_lib_file).collect();
        }
    }

    lib_files.iter().map(clone_lib_file).collect()
}

/// Parse and bind a single lib file, returning a `LibFile` or error.
///
/// This consults the disk-backed snapshot cache before parsing unless
/// `TSZ_LIB_CACHE` explicitly disables it. On a hit the parsed arena and
/// bound state are loaded from disk, skipping both parse and bind. On a
/// miss the parse + bind result is written back. See
/// `crates/tsz-core/src/parallel/lib_snapshot.rs` and
/// `docs/plan/PERFORMANCE_PLAN.md`.
fn parse_and_bind_lib_file(
    file_name: String,
    source_text: String,
) -> Result<Arc<lib_loader::LibFile>> {
    parse_and_bind_lib_file_with_source(file_name, Cow::Owned(source_text))
}

fn parse_and_bind_lib_source(
    file_name: String,
    source_text: LibSourceText,
) -> Result<Arc<lib_loader::LibFile>> {
    match source_text {
        LibSourceText::Owned(source_text) => parse_and_bind_lib_file(file_name, source_text),
        LibSourceText::Static {
            text: source_text, ..
        } => parse_and_bind_lib_file_with_source(file_name, Cow::Borrowed(source_text)),
    }
}

fn snapshot_key_for_lib_source((file_name, source_text): &(String, LibSourceText)) -> (&str, u64) {
    let content_hash = match source_text {
        LibSourceText::Owned(source_text) => {
            super::lib_snapshot::content_hash(file_name, source_text.as_str())
        }
        LibSourceText::Static { content_hash, .. } => {
            super::lib_snapshot::content_hash_from_source_hash(file_name, *content_hash)
        }
    };
    (file_name.as_str(), content_hash)
}

fn parse_and_bind_lib_file_with_source(
    file_name: String,
    source_text: Cow<'_, str>,
) -> Result<Arc<lib_loader::LibFile>> {
    if let Some(cached) = super::lib_snapshot::try_load(&file_name, source_text.as_ref()) {
        return Ok(cached);
    }

    let source_text = source_text.into_owned();
    let mut lib_parser = ParserState::new(file_name.clone(), source_text.clone());
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
    let lib = Arc::new(lib_loader::LibFile::new(
        file_name.clone(),
        arena,
        binder,
        source_file_idx,
    ));

    if let Err(err) = super::lib_snapshot::try_store(&file_name, &source_text, &lib) {
        tracing::debug!(
            target: "wasm::lib_snapshot",
            file = %file_name,
            error = %err,
            "lib snapshot write failed (compilation continues normally)",
        );
    }

    Ok(lib)
}

/// Phase 1 helper with pre-loaded file cache. Uses embedded lib contents
/// first (zero I/O), then pre-read file cache, then disk as last resort.
fn collect_lib_files_recursive_cached(
    path: &Path,
    loaded: &mut FxHashSet<PathBuf>,
    file_contents: &mut Vec<(String, LibSourceText)>,
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
        LibSourceText::Owned(cached.clone())
    } else if lib_path.starts_with(Path::new("/embedded-lib"))
        && let Some(embedded) = crate::embedded_libs::get_lib_content(embedded_key)
    {
        // Embedded virtual-root paths are never real files; avoid a failed stat
        // before reading the built-in content.
        LibSourceText::Static {
            text: embedded,
            content_hash: crate::embedded_libs::get_lib_content_hash(embedded_key)
                .expect("embedded lib content hash missing"),
        }
    } else if lib_path.exists() {
        LibSourceText::Owned(
            std::fs::read_to_string(&lib_path)
                .with_context(|| format!("failed to read lib file {}", lib_path.display()))?,
        )
    } else if let Some(embedded) = crate::embedded_libs::get_lib_content(embedded_key) {
        // Built-in embedded content — zero I/O, comment-stripped for faster parsing
        LibSourceText::Static {
            text: embedded,
            content_hash: crate::embedded_libs::get_lib_content_hash(embedded_key)
                .expect("embedded lib content hash missing"),
        }
    } else {
        // Fallback to disk read
        LibSourceText::Owned(
            std::fs::read_to_string(&lib_path)
                .with_context(|| format!("failed to read lib file {}", lib_path.display()))?,
        )
    };

    // Resolve references before adding this file (dependencies come first).
    // Embedded libs have a generated reference table, so cache-hit startup
    // does not need to rescan source text just to rediscover lib headers.
    match &source_text {
        LibSourceText::Static { .. } => {
            for ref_lib in crate::embedded_libs::get_embedded_lib_references(embedded_key) {
                let ref_path = resolve_generated_embedded_lib_reference_path(ref_lib);
                collect_lib_files_recursive_cached(&ref_path, loaded, file_contents, file_cache)?;
            }
        }
        LibSourceText::Owned(_) => {
            for ref_lib in parse_lib_references(source_text.as_str()) {
                if let Some(ref_path) = resolve_lib_reference_path(&lib_path, &ref_lib) {
                    collect_lib_files_recursive_cached(
                        &ref_path,
                        loaded,
                        file_contents,
                        file_cache,
                    )?;
                }
            }
        }
    }

    let file_name = lib_path.to_string_lossy().to_string();
    file_contents.push((file_name, source_text));
    Ok(())
}

fn resolve_generated_embedded_lib_reference_path(lib_name: &str) -> PathBuf {
    let embedded_name = crate::embedded_libs::embedded_reference_filename(lib_name);
    PathBuf::from(format!("/embedded-lib/{embedded_name}"))
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
    if base_path.starts_with(Path::new("/embedded-lib")) {
        return candidate_names.into_iter().find_map(|name| {
            let embedded_name = format!("{name}.d.ts");
            crate::embedded_libs::is_embedded_lib(&embedded_name)
                .then(|| lib_dir.join(embedded_name))
        });
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
    parse_and_bind_parallel_with_libs_and_target(files, lib_files, ScriptTarget::default())
}

/// Parse and bind multiple files in parallel with lib contexts and a compiler target.
pub fn parse_and_bind_parallel_with_libs_and_target(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
    language_version: ScriptTarget,
) -> Vec<BindResult> {
    let premerged_lib_binder = if files.len() > 1 && !lib_files.is_empty() {
        let mut binder = BinderState::new();
        binder.merge_lib_symbols(lib_files);
        Some(Arc::new(binder))
    } else {
        None
    };

    if files.len() <= 1 {
        return files
            .into_iter()
            .map(|(file_name, source_text)| {
                bind_file_with_libs_with_language_version(
                    file_name,
                    source_text,
                    lib_files,
                    language_version,
                    premerged_lib_binder.as_deref(),
                )
            })
            .collect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            bind_file_with_libs_with_language_version(
                file_name,
                source_text,
                lib_files,
                language_version,
                premerged_lib_binder.as_deref(),
            )
        })
        .collect()
}

fn bind_file_with_libs_with_language_version(
    file_name: String,
    source_text: String,
    lib_files: &[Arc<lib_loader::LibFile>],
    language_version: ScriptTarget,
    premerged_lib_binder: Option<&BinderState>,
) -> BindResult {
    // Skip parsing .json files - they should not be parsed as TypeScript.
    // JSON module imports should be resolved during module resolution and
    // emit TS2732 if resolveJsonModule is false.
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    // Parse
    let mut parser =
        ParserState::new_with_language_version(file_name.clone(), source_text, language_version);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    // Bind with lib symbols
    let mut binder = premerged_lib_binder
        .cloned()
        .unwrap_or_else(BinderState::new);
    binder.set_debug_file(&file_name);

    // IMPORTANT: Merge lib symbols BEFORE binding source file
    // so that symbols like console, Array, Promise are available during binding
    if premerged_lib_binder.is_none() && !lib_files.is_empty() {
        binder.merge_lib_symbols(lib_files);
    }

    binder.bind_source_file(&arena, source_file);
    compact_premerged_lib_state(&mut binder);

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

fn compact_premerged_lib_state(binder: &mut BinderState) {
    if binder.lib_symbol_ids.is_empty() {
        return;
    }

    let lib_symbol_ids = Arc::clone(&binder.lib_symbol_ids);
    let mut retained_lib_symbols = FxHashSet::default();
    for &sym_id in binder.node_symbols.values() {
        if lib_symbol_ids.contains(&sym_id) {
            retained_lib_symbols.insert(sym_id);
        }
    }

    collect_retained_lib_symbol_refs(binder, &lib_symbol_ids, &mut retained_lib_symbols);

    binder.file_locals =
        strip_pure_lib_entries(&binder.file_locals, &lib_symbol_ids, &retained_lib_symbols);

    for scope in Arc::make_mut(&mut binder.scopes) {
        scope.table = strip_pure_lib_entries(&scope.table, &lib_symbol_ids, &retained_lib_symbols);
    }

    let id_remap = densify_bind_symbols(binder, &lib_symbol_ids, &retained_lib_symbols);
    remap_compacted_bind_state(binder, &id_remap);
}

fn strip_pure_lib_entries(
    table: &SymbolTable,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained_lib_symbols: &FxHashSet<SymbolId>,
) -> SymbolTable {
    let retained = table
        .iter()
        .filter(|(_, sym_id)| {
            !lib_symbol_ids.contains(sym_id) || retained_lib_symbols.contains(sym_id)
        })
        .count();
    let mut stripped = SymbolTable::with_capacity(retained);
    for (name, &sym_id) in table.iter() {
        if !lib_symbol_ids.contains(&sym_id) || retained_lib_symbols.contains(&sym_id) {
            stripped.set(name.clone(), sym_id);
        }
    }
    stripped
}

fn collect_retained_lib_symbol_refs(
    binder: &BinderState,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    for table in binder.module_exports.values() {
        collect_lib_ids_from_table(table, lib_symbol_ids, retained);
    }
    for (&key, value) in binder.alias_partners.iter() {
        retain_if_lib(key, lib_symbol_ids, retained);
        retain_if_lib(*value, lib_symbol_ids, retained);
    }
    for &sym_id in binder.augmentation_target_modules.keys() {
        retain_if_lib(sym_id, lib_symbol_ids, retained);
    }

    for sym in binder.symbols.iter() {
        if lib_symbol_ids.contains(&sym.id) {
            continue;
        }
        retain_if_lib(sym.parent, lib_symbol_ids, retained);
        if let Some(exports) = sym.exports.as_ref() {
            collect_lib_ids_from_table(exports, lib_symbol_ids, retained);
        }
        if let Some(members) = sym.members.as_ref() {
            collect_lib_ids_from_table(members, lib_symbol_ids, retained);
        }
    }

    for scope in binder.scopes.iter() {
        if scope.kind != crate::binder::ContainerKind::SourceFile {
            collect_lib_ids_from_table(&scope.table, lib_symbol_ids, retained);
        }
    }
}

fn collect_lib_ids_from_table(
    table: &SymbolTable,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    for (_, &sym_id) in table.iter() {
        retain_if_lib(sym_id, lib_symbol_ids, retained);
    }
}

fn retain_if_lib(
    sym_id: SymbolId,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained: &mut FxHashSet<SymbolId>,
) {
    if lib_symbol_ids.contains(&sym_id) {
        retained.insert(sym_id);
    }
}

fn densify_bind_symbols(
    binder: &mut BinderState,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained_lib_symbols: &FxHashSet<SymbolId>,
) -> FxHashMap<SymbolId, SymbolId> {
    let retained_count = binder
        .symbols
        .iter()
        .filter(|sym| !lib_symbol_ids.contains(&sym.id) || retained_lib_symbols.contains(&sym.id))
        .count();
    let mut compacted_symbols = SymbolArena::with_capacity(retained_count);
    let mut id_remap = FxHashMap::with_capacity_and_hasher(retained_count, Default::default());

    for sym in binder.symbols.iter() {
        if lib_symbol_ids.contains(&sym.id) && !retained_lib_symbols.contains(&sym.id) {
            continue;
        }
        let old_id = sym.id;
        let new_id = compacted_symbols.alloc_from(sym);
        id_remap.insert(old_id, new_id);
    }

    for sym in compacted_symbols.iter_mut() {
        sym.parent = id_remap.get(&sym.parent).copied().unwrap_or(SymbolId::NONE);
        if let Some(exports) = sym.exports.as_ref() {
            sym.exports = remap_symbol_table_option(exports, &id_remap).map(Box::new);
        }
        if let Some(members) = sym.members.as_ref() {
            sym.members = remap_symbol_table_option(members, &id_remap).map(Box::new);
        }
    }

    binder.symbols = compacted_symbols;
    id_remap
}

fn remap_compacted_bind_state(binder: &mut BinderState, id_remap: &FxHashMap<SymbolId, SymbolId>) {
    binder.file_locals = remap_symbol_table_required(&binder.file_locals, id_remap);

    for scope in Arc::make_mut(&mut binder.scopes) {
        scope.table = remap_symbol_table_required(&scope.table, id_remap);
    }

    binder.node_symbols = Arc::new(
        binder
            .node_symbols
            .iter()
            .filter_map(|(&node, sym_id)| {
                id_remap.get(sym_id).copied().map(|new_id| (node, new_id))
            })
            .collect(),
    );

    binder.module_exports = Arc::new(
        binder
            .module_exports
            .iter()
            .filter_map(|(key, table)| {
                remap_symbol_table_option(table, id_remap).map(|remapped| (key.clone(), remapped))
            })
            .collect(),
    );

    binder.symbol_arenas = Arc::new(
        binder
            .symbol_arenas
            .iter()
            .filter_map(|(sym_id, arena)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, Arc::clone(arena)))
            })
            .collect(),
    );

    binder.declaration_arenas = Arc::new(
        binder
            .declaration_arenas
            .iter()
            .filter_map(|(&(sym_id, decl_idx), arenas)| {
                id_remap
                    .get(&sym_id)
                    .copied()
                    .map(|new_id| ((new_id, decl_idx), arenas.clone()))
            })
            .collect(),
    );

    binder.augmentation_target_modules = Arc::new(
        binder
            .augmentation_target_modules
            .iter()
            .filter_map(|(sym_id, target)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, target.clone()))
            })
            .collect(),
    );

    binder.lib_symbol_ids = Arc::new(
        binder
            .lib_symbol_ids
            .iter()
            .filter_map(|sym_id| id_remap.get(sym_id).copied())
            .collect(),
    );
    binder.lib_symbol_reverse_remap = Arc::new(
        binder
            .lib_symbol_reverse_remap
            .iter()
            .filter_map(|(sym_id, target)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, *target))
            })
            .collect(),
    );

    binder.alias_partners = Arc::new(
        binder
            .alias_partners
            .iter()
            .filter_map(|(left, right)| {
                let new_left = id_remap.get(left).copied()?;
                let new_right = id_remap.get(right).copied()?;
                Some((new_left, new_right))
            })
            .collect(),
    );

    binder.semantic_defs = Arc::new(
        binder
            .semantic_defs
            .iter()
            .filter_map(|(sym_id, entry)| {
                id_remap
                    .get(sym_id)
                    .copied()
                    .map(|new_id| (new_id, remap_semantic_def_entry(entry, id_remap)))
            })
            .collect(),
    );

    binder.expando_properties = remap_expando_properties(&binder.expando_properties, id_remap);
}
