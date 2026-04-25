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
