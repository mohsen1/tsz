//! Tests for JSX component attribute type checking.
//!
//! Verifies that TS2322 (type mismatch) and TS2741 (missing required property)
//! are correctly emitted for JSX component attributes.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerState;
use tsz_checker::test_utils::load_compiled_lib_files;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Compile JSX source with inline JSX namespace and return diagnostics.
fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    jsx_diagnostics_with_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, String)> {
    jsx_diagnostics_with_options(
        source,
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
    )
}

fn jsx_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn jsx_full_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<Diagnostic> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

// =============================================================================
// Diagnostic-assertion helpers
//
// Most assertions in this file boil down to a handful of shapes over the
// diagnostic lists produced by `jsx_diagnostics` / `jsx_diagnostics_with_pos`:
//
//   * "code C is present" / "code C is absent"
//   * "code C is present with a message fragment F"
//   * "list of messages for code C" / "count of diagnostics for code C"
//
// The helpers below express those shapes once, so individual tests don't have
// to repeat the `iter().any(...) / iter().filter(...).map(...).collect()`
// boilerplate. They are intentionally tiny adapters — they do not change any
// assertion's meaning, only its spelling.
// =============================================================================

/// Returns `true` if any diagnostic with `code` carries a message containing
/// `fragment`. The companion of [`has_code`] when callers also want to match a
/// substring of the rendered message.
fn has_code_with_message(diags: &[(u32, String)], code: u32, fragment: &str) -> bool {
    diags
        .iter()
        .any(|(c, message)| *c == code && message.contains(fragment))
}

/// Returns the messages for every diagnostic with the given `code`.
fn messages_for_code(diags: &[(u32, String)], code: u32) -> Vec<&str> {
    diags
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect()
}

/// Returns the number of diagnostics with the given `code`.
fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Position-aware variant of [`has_code_with_message`] for diagnostics
/// carrying `(code, start, message)`.
fn has_code_with_message_pos(diags: &[(u32, u32, String)], code: u32, fragment: &str) -> bool {
    diags
        .iter()
        .any(|(c, _, message)| *c == code && message.contains(fragment))
}

/// Return diagnostics with position info (code, start, message).
fn jsx_diagnostics_with_pos(source: &str) -> Vec<(u32, u32, String)> {
    jsx_diagnostics_with_pos_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_pos_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// Inline JSX namespace preamble for tests (with `ElementAttributesProperty` { props: {} }).
/// This mimics react16.d.ts's structure where props are accessed via instance.props.
const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

// =============================================================================
// SFC attribute type checking
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of jsx_component_attribute_tests tests.
/// Helper: Standard JSX namespace preamble with `ElementAttributesProperty` + `ElementChildrenAttribute`.
/// Element has a `__brand` property so it's not just `{}` — this prevents `any[]` from being
/// assignable to `JSX.Element` (which would break TS2746 single-child detection).
const JSX_CHILDREN_PREAMBLE: &str = r#"
interface Array<T> { length: number; [n: number]: T; }
declare namespace JSX {
    interface Element { __brand: string }
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

/// Helper to compile a multi-file JSX project and return diagnostics for the main file.
fn cross_file_jsx_diagnostics(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        false,
    )
}

fn cross_file_jsx_diagnostics_with_mode(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(lib_source, main_source, jsx_mode, false)
}

fn cross_file_jsx_diagnostics_with_mode_and_default_libs(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
    include_default_libs: bool,
) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_options_and_default_libs(
        lib_source,
        main_source,
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
        include_default_libs,
    )
}

fn cross_file_jsx_diagnostics_with_options_and_default_libs(
    lib_source: &str,
    main_source: &str,
    options: CheckerOptions,
    include_default_libs: bool,
) -> Vec<(u32, String)> {
    let default_lib_files = if include_default_libs {
        load_cross_file_jsx_lib_files()
    } else {
        Vec::new()
    };

    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    // Parse and bind main file
    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    let mut raw_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    raw_lib_contexts.push(tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    binder_main.merge_lib_contexts_into_binder(&raw_lib_contexts);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);

    let mut all_arenas_vec = vec![Arc::clone(&arena_main), Arc::clone(&arena_lib)];
    let mut all_binders_vec = vec![Arc::clone(&binder_main), Arc::clone(&binder_lib)];
    for lib in &default_lib_files {
        all_arenas_vec.push(Arc::clone(&lib.arena));
        all_binders_vec.push(Arc::clone(&lib.binder));
    }
    let all_arenas = Arc::new(all_arenas_vec);
    let all_binders = Arc::new(all_binders_vec);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    let mut checker_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker_lib_contexts.push(tsz_checker::context::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(default_lib_files.len());

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn cross_file_jsx_diagnostics_with_pos(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, u32, String)> {
    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    binder_main.merge_lib_contexts_into_binder(&[tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    }]);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&arena_main),
        Arc::clone(&arena_lib),
    ]));
    checker.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&binder_main),
        Arc::clone(&binder_lib),
    ]));
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_lib_contexts(vec![tsz_checker::context::LibContext {
            arena: Arc::clone(&arena_lib),
            binder: Arc::clone(&binder_lib),
        }]);
    checker.ctx.set_actual_lib_file_count(1);

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// Regression test for issue #3227: `JSX.LibraryManagedAttributes` was being
/// discarded whenever the formatted evaluated props type happened to contain
/// the substring `Factory<`. That was a display-text heuristic, not a
/// semantic condition, so any user type named `Factory` (or anything else
/// whose printed form started with `Factory<`) silently broke LMA.
///
/// Structural rule: when a component has `defaultProps`, the props returned
/// from `JSX.LibraryManagedAttributes<C, Props>` must reflect the mapped
/// optional-property result regardless of the names of types appearing in
/// the props.
fn jsx_lma_user_type_named_factory_does_not_disable_default_props_helper(
    user_type_name: &str,
) -> Vec<u32> {
    let source = format!(
        r#"
declare namespace JSX {{
    interface Element {{}}
    interface ElementClass {{}}
    interface IntrinsicElements {{}}
    type LibraryManagedAttributes<C, P> =
        C extends {{ defaultProps: infer D }}
          ? {{ [K in keyof P]?: P[K] }}
          : P;
}}

interface {user_type_name}<T> {{
    make(): T;
}}

interface Props {{
    value: {user_type_name}<number>;
    other: number;
}}

declare function Comp(props: Props): JSX.Element;
declare namespace Comp {{
    const defaultProps: {{
        value: {user_type_name}<number>;
    }};
}}

const _ok = <Comp />;
"#
    );
    jsx_codes(&source)
}

use tsz_checker::test_utils::load_typescript_fixture;

/// Helper that wraps `jsx_diagnostics` but returns only unique error codes.
fn jsx_codes(source: &str) -> Vec<u32> {
    let diags = jsx_diagnostics(source);
    let mut codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

fn load_cross_file_jsx_lib_files() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn load_es2015_dom_lib_files() -> Vec<Arc<LibFile>> {
    // `load_compiled_lib_files` parses only the named files; it does not follow
    // `/// <reference lib>` directives. Spell out the transitive
    // `lib.es6.d.ts` closure used by the React 16 fixtures that target ES2015.
    load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.decorators.d.ts",
        "lib.decorators.legacy.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.dom.d.ts",
        "lib.dom.iterable.d.ts",
        "lib.webworker.importscripts.d.ts",
        "lib.scripthost.d.ts",
    ])
}

fn stamped_cross_file_jsx_diagnostics_with_es2015_dom(
    lib_source: &str,
    main_source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut lib_files = load_es2015_dom_lib_files();
    let actual_lib_file_count = lib_files.len();
    lib_files.push(Arc::new(LibFile::from_source(
        "react.d.ts".to_string(),
        lib_source.to_string(),
    )));

    let mut parser = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.set_file_idx(0);
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "file.tsx".to_string(),
        options,
    );
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder)]));
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(
        lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect(),
    );
    checker.ctx.set_actual_lib_file_count(actual_lib_file_count);

    let mut symbol_file_index = rustc_hash::FxHashMap::default();
    for symbol in binder.symbols.iter() {
        if symbol.decl_file_idx != u32::MAX {
            symbol_file_index.entry(symbol.id).or_insert(0);
        }
    }
    checker
        .ctx
        .set_global_symbol_file_index(Arc::new(symbol_file_index));

    checker.prime_module_augmentation_bodies();
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
        .collect()
}

#[path = "jsx_component_attribute_tests/part_00.rs"]
mod part_00;
#[path = "jsx_component_attribute_tests/part_01.rs"]
mod part_01;
#[path = "jsx_component_attribute_tests/part_02.rs"]
mod part_02;
#[path = "jsx_component_attribute_tests/part_03.rs"]
mod part_03;
#[path = "jsx_component_attribute_tests/part_04.rs"]
mod part_04;
