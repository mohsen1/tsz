//! Tests for lib symbol merging of external module lib files.
//!
//! Verifies that `declare global` blocks in external module lib files
//! (those with `export {}`) correctly merge flags and value declarations
//! into the global symbol table, while module-scoped symbols are excluded.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext;
use tsz_binder::symbol_flags;
use tsz_parser::parser::ParserState;

fn bind_source(source: &str) -> (Arc<tsz_parser::parser::node::NodeArena>, BinderState) {
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = Arc::new(parser.get_arena().clone());
    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);
    (arena, binder)
}

fn make_lib_context(
    arena: &Arc<tsz_parser::parser::node::NodeArena>,
    binder: &BinderState,
) -> LibContext {
    LibContext {
        arena: Arc::clone(arena),
        binder: Arc::new(binder.clone()),
    }
}

#[test]
fn declare_global_var_merges_value_flag_into_existing_interface() {
    // Simulate es2015.iterable.d.ts: defines Iterator as an interface
    let (base_arena, base_binder) = bind_source(
        "interface Iterator<T, TReturn = any, TNext = any> {
            next(...args: [] | [TNext]): { done: boolean; value: T };
        }",
    );

    // Simulate esnext.iterator.d.ts: external module with declare global
    let (ext_arena, ext_binder) = bind_source(
        "export {};
        declare abstract class Iterator<T, TResult = undefined, TNext = unknown> {
            abstract next(value?: TNext): { done: boolean; value: T };
        }
        interface Iterator<T, TResult, TNext> {}
        type IteratorObjectConstructor = typeof Iterator;
        declare global {
            interface IteratorConstructor extends IteratorObjectConstructor {}
            var Iterator: IteratorConstructor;
        }",
    );

    // Verify the external module binder has the expected state
    assert!(ext_binder.is_external_module);
    assert!(
        ext_binder.global_augmentations.contains_key("Iterator"),
        "Iterator should be in global_augmentations"
    );

    // Create the main binder and merge lib contexts
    let mut main_binder = BinderState::new();
    let base_ctx = make_lib_context(&base_arena, &base_binder);
    let ext_ctx = make_lib_context(&ext_arena, &ext_binder);
    main_binder.merge_lib_contexts_into_binder(&[base_ctx, ext_ctx]);

    // The merged Iterator symbol should have both INTERFACE and VALUE flags
    let iter_sym_id = main_binder
        .file_locals
        .get("Iterator")
        .expect("Iterator should be in file_locals");
    let iter_sym = main_binder
        .symbols
        .get(iter_sym_id)
        .expect("Iterator symbol should exist");

    assert!(
        iter_sym.has_any_flags(symbol_flags::INTERFACE),
        "Iterator should have INTERFACE flag (from base lib)"
    );
    assert!(
        iter_sym.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE),
        "Iterator should have FUNCTION_SCOPED_VARIABLE flag (from declare global var)"
    );
    // The CLASS flag from the module-scoped abstract class should NOT be merged
    assert!(
        !iter_sym.has_any_flags(symbol_flags::CLASS),
        "Iterator should NOT have CLASS flag (module-scoped, not from declare global)"
    );
}

#[test]
fn declare_global_interface_merges_into_existing_interface() {
    // Simulate a base lib with a minimal IteratorObject interface
    let (base_arena, base_binder) = bind_source(
        "interface IteratorObject<T, TReturn = unknown, TNext = unknown> {
            [Symbol.iterator](): IteratorObject<T, TReturn, TNext>;
        }",
    );

    // External module with declare global adding methods to IteratorObject
    let (ext_arena, ext_binder) = bind_source(
        "export {};
        declare global {
            interface IteratorObject<T, TReturn, TNext> {
                map<U>(fn: (value: T) => U): IteratorObject<U, undefined, unknown>;
                filter(fn: (value: T) => boolean): IteratorObject<T, undefined, unknown>;
            }
        }",
    );

    assert!(ext_binder.is_external_module);
    assert!(
        ext_binder
            .global_augmentations
            .contains_key("IteratorObject")
    );

    let mut main_binder = BinderState::new();
    let base_ctx = make_lib_context(&base_arena, &base_binder);
    let ext_ctx = make_lib_context(&ext_arena, &ext_binder);
    main_binder.merge_lib_contexts_into_binder(&[base_ctx, ext_ctx]);

    let sym_id = main_binder
        .file_locals
        .get("IteratorObject")
        .expect("IteratorObject should be in file_locals");
    let sym = main_binder
        .symbols
        .get(sym_id)
        .expect("IteratorObject symbol should exist");

    assert!(
        sym.has_any_flags(symbol_flags::INTERFACE),
        "IteratorObject should have INTERFACE flag"
    );
    // Should have declarations from both lib files
    assert!(
        sym.declarations.len() >= 2,
        "IteratorObject should have declarations from both libs, got {}",
        sym.declarations.len()
    );
}

#[test]
fn script_top_level_interface_augments_non_builtin_lib_global() {
    // Regression test for the conformance failure on `coAndContraVariantInferences2.ts`
    // (and any similar test that augments a lib.dom/webworker/etc. global like
    // `Node`, `Element`, `EventTarget`, etc.).
    //
    // Before the fix, only a hardcoded allow-list of "built-in" type names
    // (`Object`, `Array`, `Promise`, …) would be tracked as augmentations when
    // a script-level `interface X { ... }` shared the name of a lib symbol.
    // Names like `Node` (defined in lib.dom.d.ts) were silently ignored, so
    // user-side declaration merging never propagated to the merged program
    // and downstream lib re-checks couldn't see the new members.
    //
    // The fix replaces the static allow-list with a dynamic check against
    // the binder's `lib_symbol_ids`, so any same-named lib symbol counts.

    // Simulate a "lib.dom" file declaring a `Node` interface.
    let lib = Arc::new(LibFile::from_source(
        "lib.dom.d.ts".to_string(),
        "interface Node {
            nodeType: number;
        }"
        .to_string(),
    ));

    // User script (not a module) augments `Node` with a new property.
    // Use `bind_source_file_with_libs` so lib symbols are visible in the
    // current scope before we bind the user file — this matches how the
    // CLI/parallel binder pipeline drives binding.
    let mut user_parser = ParserState::new(
        "user.ts".to_string(),
        "interface Node { kind: string; }".to_string(),
    );
    let user_root = user_parser.parse_source_file();
    let mut user_binder = BinderState::new();
    user_binder.bind_source_file_with_libs(user_parser.get_arena(), user_root, &[Arc::clone(&lib)]);

    assert!(
        !user_binder.is_external_module,
        "test source should be a script, not a module"
    );

    assert!(
        user_binder.global_augmentations.contains_key("Node"),
        "user `interface Node` in a script must be tracked as a global \
         augmentation even though `Node` is not in the static built-in list"
    );

    // Sanity: the corresponding allow-list entry that already worked
    // (Object) keeps working — augmentation tracking should not regress.
    let mut object_parser = ParserState::new(
        "object.ts".to_string(),
        "interface Object { extra: number; }".to_string(),
    );
    let object_root = object_parser.parse_source_file();
    let mut object_binder = BinderState::new();
    let object_lib = Arc::new(LibFile::from_source(
        "lib.es5.d.ts".to_string(),
        "interface Object { toString(): string; }".to_string(),
    ));
    object_binder.bind_source_file_with_libs(
        object_parser.get_arena(),
        object_root,
        &[Arc::clone(&object_lib)],
    );
    assert!(
        object_binder.global_augmentations.contains_key("Object"),
        "Object (built-in lib type) must keep being tracked as a global \
         augmentation — the new dynamic check must not regress the existing \
         allow-list path"
    );
}

#[test]
fn module_scoped_symbols_excluded_from_file_locals() {
    // External module lib with module-scoped type alias
    let (ext_arena, ext_binder) = bind_source(
        "export {};
        type ModuleScopedType = string;
        declare global {
            interface GlobalInterface {}
        }",
    );

    assert!(ext_binder.is_external_module);

    let mut main_binder = BinderState::new();
    let ext_ctx = make_lib_context(&ext_arena, &ext_binder);
    main_binder.merge_lib_contexts_into_binder(&[ext_ctx]);

    // GlobalInterface should be visible
    assert!(
        main_binder.file_locals.has("GlobalInterface"),
        "GlobalInterface from declare global should be in file_locals"
    );
    // ModuleScopedType should NOT be visible
    assert!(
        !main_binder.file_locals.has("ModuleScopedType"),
        "Module-scoped type should NOT be in file_locals"
    );
}

#[test]
fn resolve_name_in_lib_module_locals_finds_hoisted_global() {
    // Two-lib setup mirroring the runtime: a base lib defines `Iterator`,
    // and an external-module extension contributes a `declare global { var
    // Iterator }` augmentation. The probe must return the main-binder
    // SymbolId, expose the lib symbol flags to the accept callback, and
    // short-circuit on the first accepted candidate.
    let (base_arena, base_binder) = bind_source("interface Iterator<T> { next(): T; }");
    let (ext_arena, ext_binder) = bind_source(
        "export {};
        declare global {
            var Iterator: { new<T>(): Iterator<T> };
        }",
    );

    let mut main_binder = BinderState::new();
    let base_ctx = make_lib_context(&base_arena, &base_binder);
    let ext_ctx = make_lib_context(&ext_arena, &ext_binder);
    main_binder.merge_lib_contexts_into_binder(&[base_ctx, ext_ctx]);

    let main_iter_id = main_binder
        .file_locals
        .get("Iterator")
        .expect("Iterator should be in main file_locals after merge");

    let lib_binders: Vec<Arc<BinderState>> = vec![Arc::new(base_binder), Arc::new(ext_binder)];

    let mut visit_count = 0;
    let mut seen_flags = Vec::new();
    let resolved =
        main_binder.resolve_name_in_lib_module_locals("Iterator", &lib_binders, |id, flags| {
            visit_count += 1;
            seen_flags.push(flags);
            Some(id)
        });
    assert_eq!(resolved, Some(main_iter_id));
    assert_eq!(visit_count, 1, "should stop at first accepted candidate");
    assert!(
        seen_flags[0] & symbol_flags::INTERFACE != 0,
        "callback must receive lib symbol flags ({:#x})",
        seen_flags[0]
    );

    let mut total_visits = 0;
    let result =
        main_binder.resolve_name_in_lib_module_locals("Iterator", &lib_binders, |_id, _flags| {
            total_visits += 1;
            None
        });
    assert_eq!(result, None);
    assert_eq!(
        total_visits, 2,
        "reject-all must visit every lib binder that has the name"
    );
}

#[test]
fn resolve_name_in_lib_module_locals_returns_none_when_name_absent() {
    // Module-scoped lib symbols excluded by Phase 3 of the merge are absent
    // from the main binder's file_locals. The probe must return None without
    // invoking the accept callback (no point asking policy about a symbol
    // that has no current-binder ID).
    let (lib_arena, lib_binder) = bind_source("interface SomeGlobal {}");
    let mut main_binder = BinderState::new();
    let lib_ctx = make_lib_context(&lib_arena, &lib_binder);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);
    assert!(!main_binder.file_locals.has("Nonexistent"));

    let lib_binders: Vec<Arc<BinderState>> = vec![Arc::new(lib_binder)];
    let mut accept_calls = 0;
    let result =
        main_binder.resolve_name_in_lib_module_locals("Nonexistent", &lib_binders, |id, _| {
            accept_calls += 1;
            Some(id)
        });
    assert_eq!(result, None);
    assert_eq!(accept_calls, 0);
}

#[test]
fn resolve_name_in_lib_module_locals_surfaces_phase3_excluded_module_scoped_flags() {
    // The probe's reason for existing is to reach lib symbols that Phase 3
    // of the merge intentionally excluded from the global hoist. Mirror the
    // `es2025.iterator.d.ts` shape: a base lib defines `interface Iterator`,
    // and an external-module lib has a module-scoped `class Iterator` that
    // does NOT participate in the global hoist (no `declare global` for it).
    //
    // The probe must surface the module-scoped CLASS flags to the accept
    // callback so the checker can choose to accept that candidate even
    // though the post-merge `file_locals` carries only the global INTERFACE
    // symbol. Reject the base candidate explicitly to prove the iteration
    // continues past it.
    let (base_arena, base_binder) = bind_source("interface Iterator<T> { next(): T; }");
    let (ext_arena, ext_binder) = bind_source(
        "export {};
        declare abstract class Iterator<T> { abstract next(): T; }",
    );

    let mut main_binder = BinderState::new();
    let base_ctx = make_lib_context(&base_arena, &base_binder);
    let ext_ctx = make_lib_context(&ext_arena, &ext_binder);
    main_binder.merge_lib_contexts_into_binder(&[base_ctx, ext_ctx]);

    // Sanity: the module-scoped class is excluded from the merged file_locals,
    // but the base interface is hoisted with INTERFACE-only flags.
    let main_iter_id = main_binder
        .file_locals
        .get("Iterator")
        .expect("base lib Iterator should be hoisted into file_locals");
    let main_iter_sym = main_binder
        .symbols
        .get(main_iter_id)
        .expect("Iterator symbol exists");
    assert!(
        main_iter_sym.has_any_flags(symbol_flags::INTERFACE),
        "hoisted symbol must have INTERFACE flag"
    );
    assert!(
        !main_iter_sym.has_any_flags(symbol_flags::CLASS),
        "module-scoped CLASS must NOT leak into the global symbol's flags"
    );

    let lib_binders: Vec<Arc<BinderState>> = vec![Arc::new(base_binder), Arc::new(ext_binder)];

    let mut seen_flags = Vec::new();
    let result =
        main_binder.resolve_name_in_lib_module_locals("Iterator", &lib_binders, |id, flags| {
            seen_flags.push(flags);
            (flags & symbol_flags::CLASS != 0).then_some(id)
        });

    assert_eq!(
        result,
        Some(main_iter_id),
        "probe must accept the module-scoped CLASS candidate and return the current-binder id"
    );
    assert_eq!(
        seen_flags.len(),
        2,
        "probe must visit both lib binders (rejected interface + accepted class)"
    );
    assert!(
        seen_flags[0] & symbol_flags::INTERFACE != 0,
        "first visit should expose the base lib INTERFACE flag, got {:#x}",
        seen_flags[0]
    );
    assert!(
        seen_flags[1] & symbol_flags::CLASS != 0,
        "second visit should expose the module-scoped CLASS flag — this is the \
         Phase-3-excluded fact the probe must surface, got {:#x}",
        seen_flags[1]
    );
}

#[test]
fn resolve_name_in_lib_module_locals_callback_can_substitute_sym_id() {
    // The accept callback may return a different SymbolId than `file_sym_id`
    // — for example, the alias target. The probe returns the callback's
    // chosen id verbatim. Use a second real binder symbol as the substitute
    // so the returned id refers to a valid declaration.
    let (lib_arena, lib_binder) = bind_source(
        "interface Anchor {}
         interface Substitute {}",
    );
    let mut main_binder = BinderState::new();
    let lib_ctx = make_lib_context(&lib_arena, &lib_binder);
    main_binder.merge_lib_contexts_into_binder(&[lib_ctx]);

    let anchor_id = main_binder.file_locals.get("Anchor").expect("Anchor");
    let substitute_id = main_binder
        .file_locals
        .get("Substitute")
        .expect("Substitute");
    assert_ne!(anchor_id, substitute_id);

    let lib_binders: Vec<Arc<BinderState>> = vec![Arc::new(lib_binder)];
    let result =
        main_binder.resolve_name_in_lib_module_locals("Anchor", &lib_binders, |_id, _flags| {
            Some(substitute_id)
        });
    assert_eq!(result, Some(substitute_id));
}
