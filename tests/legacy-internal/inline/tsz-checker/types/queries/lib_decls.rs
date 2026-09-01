//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/queries/lib_decls.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f6a6499ff87c4fa7c228d1c9efb717e5008e611d3725b19f11eeb59cb1cf0b0b 506 foreign_arena_collision_pair_is_rejected
    /// A `(decl_idx, fallback_arena)` pair where the index addresses an
    /// unrelated node in a foreign arena must be dropped, not lowered.
    /// This was the issue #13255 poison: cross-file program symbols fell
    /// back to an arena that never declared them, and the colliding node
    /// produced a wrong type in the shared definition store.
    #[test]
    fn foreign_arena_collision_pair_is_rejected() {
        let (_, owner_binder) = parse_and_bind(
            "telemetry.ts",
            "export interface TelemetryFrame { gimbalAxis: string; }\n",
        );
        let (sym_id, decl_idx, decl_count) = interface_decl(&owner_binder, "TelemetryFrame");
        assert_eq!(decl_count, 1);

        // A foreign arena large enough that `decl_idx` addresses *some*
        // node — just never a declaration of `TelemetryFrame`.
        let (foreign_arena, _) = parse_and_bind(
            "unrelated.ts",
            "const pad0 = 0;\nconst pad1 = 1;\nconst pad2 = 2;\nconst pad3 = 3;\n\
             const pad4 = 4;\nconst pad5 = 5;\nconst pad6 = 6;\nconst pad7 = 7;\n",
        );
        assert!(
            foreign_arena.get(decl_idx).is_some(),
            "collision setup requires the index to resolve in the foreign arena"
        );

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            foreign_arena.as_ref(),
            None,
        );
        assert!(
            pairs.is_empty(),
            "foreign-arena collision pair must be rejected, got {} pair(s)",
            pairs.len()
        );
    }
// TSZ_INLINE_TEST_END f6a6499ff87c4fa7c228d1c9efb717e5008e611d3725b19f11eeb59cb1cf0b0b

// TSZ_INLINE_TEST_BEGIN 98b19d280961044543ed277496d8072424c09c1453fee0475ea10913ec4d931e 543 owning_arena_fallback_pair_is_kept
    /// The fallback stays usable when the arena really declares the symbol:
    /// the node at `decl_idx` is a named declaration with the symbol's name.
    #[test]
    fn owning_arena_fallback_pair_is_kept() {
        let (owner_arena, owner_binder) = parse_and_bind(
            "telemetry.ts",
            "export interface ApogeeWindow { ascentRate: number; }\n",
        );
        let (sym_id, decl_idx, _) = interface_decl(&owner_binder, "ApogeeWindow");

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            owner_arena.as_ref(),
            None,
        );
        assert_eq!(
            pairs.len(),
            1,
            "fallback pair in the declaring arena must be kept"
        );
        assert_eq!(pairs[0].0, decl_idx);
        assert!(std::ptr::eq(pairs[0].1, owner_arena.as_ref()));
    }
// TSZ_INLINE_TEST_END 98b19d280961044543ed277496d8072424c09c1453fee0475ea10913ec4d931e

// TSZ_INLINE_TEST_BEGIN 04c2b6cf7a78392920e464d39369bd6f87d449349aa543c25ef70541ba0ff136 570 registered_home_arena_pair_is_trusted_without_name_match
    /// Declarations without a plain identifier name (destructuring binding
    /// patterns here) cannot pass the name check, but the binder-registered
    /// home arena proves ownership, so the pair must survive.
    #[test]
    fn registered_home_arena_pair_is_trusted_without_name_match() {
        let (owner_arena, mut owner_binder) = parse_and_bind(
            "boom.ts",
            "export const { boomArmSpan } = { boomArmSpan: 4 };\n",
        );
        let sym_id = owner_binder
            .file_locals
            .get("boomArmSpan")
            .expect("binder should expose boomArmSpan");
        let symbol = owner_binder
            .get_symbol(sym_id)
            .expect("symbol should resolve");
        let decl_idx = *symbol
            .declarations
            .first()
            .expect("symbol should have a declaration");
        assert!(
            !fallback_arena_node_declares_symbol(
                &owner_binder,
                sym_id,
                decl_idx,
                owner_arena.as_ref()
            ),
            "test setup requires a declaration the name check cannot prove"
        );

        let symbol_arenas = Arc::make_mut(&mut owner_binder.symbol_arenas);
        symbol_arenas.insert(sym_id, Arc::clone(&owner_arena));

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            owner_arena.as_ref(),
            None,
        );
        assert_eq!(
            pairs.len(),
            1,
            "registered home-arena pair must be trusted even without a provable name"
        );
    }
// TSZ_INLINE_TEST_END 04c2b6cf7a78392920e464d39369bd6f87d449349aa543c25ef70541ba0ff136
