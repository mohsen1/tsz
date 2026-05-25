use super::*;

#[test]
fn semantic_defs_identity_stable_with_changed_bodies() {
    // Verify that changing function/class bodies does not affect the
    // binder's semantic_defs shape. Only the AST-level structure changes;
    // the semantic identity (kind, name, arity, heritage) should stay the same.
    let source_v1 = r"
class MyClass<T> { value: T; foo(): void {} }
interface MyInterface { x: number; y: string }
type MyAlias<X> = X | null;
enum MyEnum { A = 1, B = 2 }
";

    let source_v2 = r"
class MyClass<T> { value: T; bar(): string { return ''; } baz(): void {} }
interface MyInterface { x: number; y: string; z: boolean }
type MyAlias<X> = X | null;
enum MyEnum { A = 1, B = 2 }
";

    let binder1 = bind_source(source_v1);
    let binder2 = bind_source(source_v2);

    // Same top-level families should exist.
    assert_eq!(
        binder1.semantic_defs.len(),
        binder2.semantic_defs.len(),
        "Body changes should not affect semantic_defs count"
    );

    for entry1 in binder1.semantic_defs.values() {
        let entry2 = binder2
            .semantic_defs
            .values()
            .find(|e| e.name == entry1.name)
            .unwrap_or_else(|| panic!("Missing {} after body change", entry1.name));

        assert_eq!(
            entry1.kind, entry2.kind,
            "{}: kind should be stable across body changes",
            entry1.name
        );
        assert_eq!(
            entry1.type_param_count, entry2.type_param_count,
            "{}: arity should be stable across body changes",
            entry1.name
        );
    }
}

#[test]
fn arena_for_declaration_or_falls_back_when_unmapped() {
    // Fresh binder has no cross-file declaration arenas registered, so the
    // helper should return the fallback arena for any (sym, decl) pair.
    let source = r"const x = 1;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let sym_id = binder.file_locals.get("x").expect("symbol for x");
    let sym = binder.symbols.get(sym_id).expect("symbol data");
    let decl_idx = sym.primary_declaration().expect("primary declaration");

    // No cross-file mapping exists — helper must hand back the fallback arena.
    let got = binder.arena_for_declaration_or(sym_id, decl_idx, arena);
    assert!(std::ptr::eq(got, arena));

    // Helper matches the explicit Option-collapsing expression it replaces.
    assert!(binder.get_arena_for_declaration(sym_id, decl_idx).is_none());
}

#[test]
fn flow_nodes_arc_share_is_zero_copy() {
    // After binding, `BinderState.flow_nodes` is an `Arc<FlowNodeArena>`.
    // Cloning the Arc to model the per-file binder reconstruction path
    // (`flow_nodes: Arc::clone(&file.flow_nodes)` in
    //  `check_utils::create_binder_from_bound_file_with_augmentations`)
    // must be a pointer-equality share — no deep clone of the underlying
    // `Vec<FlowNode>`. This is the invariant that saves ~2N deep clones
    // on N-file projects.
    let source = r"
        function foo(x: number | string) {
            if (typeof x === 'string') {
                return x.length;
            }
            return x + 1;
        }
    ";
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Sanity: binding produced multiple flow nodes (START, assignment,
    // conditions, branch labels) for the control flow graph.
    assert!(
        binder.flow_nodes.len() > 3,
        "expected multiple flow nodes for the narrowing example, got {}",
        binder.flow_nodes.len()
    );

    // Cloning the Arc must be zero-copy: the inner pointer stays identical,
    // and the strong count increases to 2 (original + clone).
    assert_eq!(Arc::strong_count(&binder.flow_nodes), 1);
    let shared = Arc::clone(&binder.flow_nodes);
    assert_eq!(Arc::strong_count(&binder.flow_nodes), 2);
    assert!(Arc::ptr_eq(&binder.flow_nodes, &shared));

    // Read semantics still work through `Deref` — iterator count must
    // match `.len()` without any materialization of a new arena.
    assert_eq!(shared.iter().count(), binder.flow_nodes.len());
    assert_eq!(shared.len(), binder.flow_nodes.len());
}

// =============================================================================
// Phase 1 — `StableLocation` plumbing
//
// These tests verify the binder populates arena-free declaration locations
// in lockstep with the existing `NodeIndex` fields. See
// `docs/plan/ROADMAP.md` stable-identity workstream.
// =============================================================================

#[test]
fn stable_location_size_fits_twelve_bytes() {
    use crate::symbols::StableLocation;
    // Hard constraint from the architecture plan: `StableLocation` must stay
    // ≤ 16 bytes so it can live inline in symbol metadata without bloating
    // `Symbol`. Three `u32`s pack to exactly 12 bytes on every target.
    assert_eq!(
        std::mem::size_of::<StableLocation>(),
        12,
        "StableLocation must remain 12 bytes"
    );
    assert!(std::mem::size_of::<StableLocation>() <= 16);
}

#[test]
fn stable_location_default_is_none_sentinel() {
    use crate::symbols::StableLocation;
    let none = StableLocation::NONE;
    assert_eq!(none.file_idx, u32::MAX);
    assert_eq!(none.pos, 0);
    assert_eq!(none.end, 0);
    assert!(!none.is_known());
    assert!(!none.has_file_idx());
    assert_eq!(StableLocation::default(), none);
}

#[test]
fn stable_location_roundtrips_file_idx_and_span() {
    use crate::symbols::StableLocation;
    // file_idx=7 pos=100 end=142 — the synthetic project uses this below.
    let loc = StableLocation::new(7, 100, 142);
    assert_eq!(loc.file_idx, 7);
    assert_eq!(loc.pos, 100);
    assert_eq!(loc.end, 142);
    assert!(loc.is_known());
    assert!(loc.has_file_idx());
}

#[test]
fn stable_location_set_file_idx_if_unassigned_is_latching() {
    use crate::symbols::StableLocation;
    let mut loc = StableLocation::with_unassigned_file(10, 20);
    assert!(!loc.has_file_idx());
    loc.set_file_idx_if_unassigned(3);
    assert_eq!(loc.file_idx, 3);
    // Already-assigned locations must not be overwritten.
    loc.set_file_idx_if_unassigned(99);
    assert_eq!(loc.file_idx, 3);
}

#[test]
fn binder_populates_stable_declarations_in_lockstep() {
    // Every `NodeIndex` on `Symbol::declarations` must have a sibling entry
    // on `Symbol::stable_declarations`, and its `(pos, end)` must equal the
    // declaration node's source span. This is the core Phase 1 invariant.
    let source = r"
function foo() {}
interface Bar { x: number }
interface Bar { y: string }
type Baz = number;
class Qux {}
const v = 1;
";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(7);
    binder.bind_source_file(arena, root);

    for name in ["foo", "Bar", "Baz", "Qux", "v"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected symbol {name}"));
        let sym = binder.symbols.get(sym_id).expect("symbol data");
        assert_eq!(
            sym.declarations.len(),
            sym.stable_declarations.len(),
            "stable_declarations must parallel declarations for {name}"
        );
        for (decl_idx, stable) in sym.declarations.iter().zip(sym.stable_declarations.iter()) {
            let node = arena.get(*decl_idx).expect("declaration node");
            assert_eq!(
                stable.pos, node.pos,
                "stable pos should match node.pos for {name}"
            );
            assert_eq!(
                stable.end, node.end,
                "stable end should match node.end for {name}"
            );
            assert_eq!(
                stable.file_idx, 7,
                "file_idx must be stamped to driver value for {name}"
            );
        }
    }
}

#[test]
fn stable_value_declaration_matches_arena_span() {
    // `stable_value_declaration` must match the `value_declaration`'s
    // source span once populated.
    let source = r"
const hello = 42;
function world() { return 0; }
";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(13);
    binder.bind_source_file(arena, root);

    for name in ["hello", "world"] {
        let sym_id = binder.file_locals.get(name).expect("symbol");
        let sym = binder.symbols.get(sym_id).expect("symbol data");
        assert!(sym.value_declaration.is_some(), "{name} should have vd");
        let vd_node = arena.get(sym.value_declaration).expect("vd node");
        assert_eq!(sym.stable_value_declaration.file_idx, 13);
        assert_eq!(sym.stable_value_declaration.pos, vd_node.pos);
        assert_eq!(sym.stable_value_declaration.end, vd_node.end);
        assert!(sym.stable_value_declaration.is_known());
    }
}

#[test]
fn stable_locations_default_file_idx_when_driver_unassigned() {
    // When the driver never calls `set_file_idx`, stable locations retain
    // the `u32::MAX` sentinel (they still carry a usable span).
    let source = r"function foo() {}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    // No binder.set_file_idx(..) call on purpose.
    binder.bind_source_file(arena, root);

    let sym_id = binder.file_locals.get("foo").expect("symbol");
    let sym = binder.symbols.get(sym_id).expect("data");
    assert_eq!(sym.stable_declarations.len(), sym.declarations.len());
    for stable in &sym.stable_declarations {
        assert_eq!(
            stable.file_idx,
            u32::MAX,
            "unassigned drivers keep sentinel file_idx"
        );
        assert!(stable.is_known(), "span still captured");
    }
    assert_eq!(sym.stable_value_declaration.file_idx, u32::MAX);
    assert!(sym.stable_value_declaration.is_known());
}

#[test]
fn stable_locations_identify_declarations_after_arena_drop() {
    // Simulate the Phase 5 scenario: user binds a file, the driver drops
    // the arena, but the `StableLocation`s are enough to reconstruct
    // `(file_idx, span)` and match them against a freshly reparsed file.
    let source = r"
function foo() {}
type Bar = number;
class Qux {}
";
    type SnapshotEntry = ((u32, String), (u32, u32, u32));
    let snapshot: Vec<SnapshotEntry> = {
        let mut parser = ParserState::new("syn.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.set_file_idx(42);
        binder.bind_source_file(arena, root);

        let mut out = Vec::new();
        for name in ["foo", "Bar", "Qux"] {
            let sym_id = binder.file_locals.get(name).expect("symbol");
            let sym = binder.symbols.get(sym_id).expect("data");
            // Type aliases have no value declaration, so anchor the stable
            // identity on the first entry of `stable_declarations` — the
            // arena-free counterpart of `Symbol::declarations[0]`.
            let stable = *sym
                .stable_declarations
                .first()
                .expect("stable declaration for first decl");
            out.push((
                (sym_id.0, name.to_string()),
                (stable.file_idx, stable.pos, stable.end),
            ));
        }
        out
        // All binder/arena state is dropped at end of this scope.
    };

    // Re-parse the same file, then verify the previously-captured triples
    // still locate the expected declarations. (Span-based identity survives.)
    let mut parser = ParserState::new("syn.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.set_file_idx(42);
    binder.bind_source_file(arena, root);

    for ((_old_sym_id, name), (file_idx, pos, end)) in snapshot {
        assert_eq!(file_idx, 42, "file_idx survives reparse for {name}");
        // Find the node with the captured span: because the source text is
        // identical, the new arena will contain a node at the same (pos, end).
        let matched = arena
            .nodes
            .iter()
            .any(|node| node.pos == pos && node.end == end);
        assert!(
            matched,
            "stable (pos, end) for {name} must match a node in the re-parsed arena"
        );
    }
}

// =============================================================================
// BinderState round-trip (lib snapshot campaign)
// =============================================================================

/// Phase 1.5 (`PERFORMANCE_PLAN.md)`: `BinderState` must round-trip
/// through serde with diagnostic-identical behaviour, otherwise the
/// disk-backed lib cache would silently corrupt symbol resolution. The
/// resolution caches (`resolved_export_cache`, `resolved_identifier_cache`)
/// are `#[serde(skip)]` because they're regenerable; this test verifies
/// the *non-cache* state — symbols, scopes, flow, declared modules —
/// survives serialise → deserialise.
///
/// Uses `serde_json` for debuggability (the production lib snapshot
/// pipeline will use a binary format, but JSON exercises the same serde
/// derives).
#[test]
fn binder_state_round_trips_via_serde_json_preserves_symbols() {
    let source = "
interface Promise<T> {
    then(): Promise<T>;
}
const greeting = \"hello world\";
type Cache = { storedValue: number };
declare module \"virtual:env\" {
    export const VAL: string;
}
";
    let mut parser = ParserState::new("snapshot_round_trip.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Capture pre-serialize state.
    let original_file_locals_len = binder.file_locals.len();
    let original_symbols_len = binder.symbols.len();
    let original_declared_modules_len = binder.declared_modules.len();
    let original_promise = binder.file_locals.get("Promise");
    let original_greeting = binder.file_locals.get("greeting");
    let original_cache = binder.file_locals.get("Cache");
    assert!(
        original_file_locals_len > 0,
        "fixture must declare file locals"
    );
    assert!(original_promise.is_some(), "Promise must be bound");
    assert!(original_greeting.is_some(), "greeting must be bound");
    assert!(original_cache.is_some(), "Cache must be bound");

    // Round-trip via JSON (binary format will be a separate PR).
    let json = serde_json::to_string(&binder).expect("BinderState should serialize");
    let restored: BinderState =
        serde_json::from_str(&json).expect("BinderState should deserialize");

    // Top-level lengths preserved.
    assert_eq!(restored.file_locals.len(), original_file_locals_len);
    assert_eq!(restored.symbols.len(), original_symbols_len);
    assert_eq!(
        restored.declared_modules.len(),
        original_declared_modules_len
    );

    // Symbol IDs for the same names match (proves SymbolArena round-trips).
    assert_eq!(restored.file_locals.get("Promise"), original_promise);
    assert_eq!(restored.file_locals.get("greeting"), original_greeting);
    assert_eq!(restored.file_locals.get("Cache"), original_cache);

    // Declared modules survive (proves Arc<FxHashSet<String>> round-trips).
    assert!(restored.declared_modules.contains("virtual:env"));

    // Resolution caches start empty (they're #[serde(skip)] — confirms the
    // cache invariant: lazy-rebuild from the binder's sources is safe).
    assert!(
        restored
            .resolved_export_cache
            .read()
            .expect("RwLock should not be poisoned")
            .is_empty(),
        "resolved_export_cache must be empty after deserialize (regenerated lazily)"
    );
    assert!(
        restored
            .resolved_identifier_cache
            .read()
            .expect("RwLock should not be poisoned")
            .is_empty(),
        "resolved_identifier_cache must be empty after deserialize"
    );
}

#[test]
fn binder_resolution_cache_statistics_track_entries_and_clear() {
    let mut binder = BinderState::new();
    binder
        .resolved_export_cache
        .write()
        .expect("resolved_export_cache RwLock should not be poisoned")
        .insert(
            ("module".to_string(), "value".to_string()),
            Some(SymbolId(1)),
        );
    binder
        .resolved_identifier_cache
        .write()
        .expect("resolved_identifier_cache RwLock should not be poisoned")
        .insert((42, 7), None);

    let stats = binder.resolution_cache_statistics();
    assert_eq!(stats.export_cache_entries, 1);
    assert_eq!(stats.identifier_cache_entries, 1);
    assert_eq!(stats.total_entries(), 2);
    assert!(stats.estimated_size_bytes() > 0);

    binder.clear_resolution_caches();
    assert_eq!(
        binder.resolution_cache_statistics(),
        Default::default(),
        "resolution cache stats should return to zero after clear"
    );
}

#[test]
fn binder_state_round_trips_empty_program() {
    let source = ";";
    let mut parser = ParserState::new("empty.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let json = serde_json::to_string(&binder).expect("empty BinderState serializes");
    let restored: BinderState =
        serde_json::from_str(&json).expect("empty BinderState deserializes");

    assert_eq!(restored.symbols.len(), binder.symbols.len());
    assert_eq!(restored.file_locals.len(), binder.file_locals.len());
}

#[test]
fn next_persistent_scope_id_reserves_none_sentinel() {
    assert!(
        super::core::next_persistent_scope_id((u32::MAX as usize) - 1).is_some(),
        "largest representable persistent scope id should remain usable"
    );
    assert!(
        super::core::next_persistent_scope_id(u32::MAX as usize).is_none(),
        "ScopeId::NONE sentinel (u32::MAX) must remain reserved"
    );
}
