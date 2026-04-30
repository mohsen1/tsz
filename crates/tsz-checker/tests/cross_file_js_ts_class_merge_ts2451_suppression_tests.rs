//! When a `.d.ts` `declare class A {}` and a `.js` `const A = {}` share a
//! name across files (with `allowJs` + `checkJs`), neither file should emit
//! TS2451 ("Cannot redeclare block-scoped variable"). The structural rule:
//! "if any side of the variable/class pair lives in a JS file with `check_js`,
//! it's a CommonJS-style namespace augmentation, not a redeclaration."
//!
//! Mirrors the .d.ts-side false positive observed in
//! `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration3.ts`.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };

    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318) // ignore lib-not-loaded noise
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// `.d.ts`-side check: no TS2451 should fire on `declare class A {}` when
/// a JS file declares `const A = {}`.
#[test]
fn no_ts2451_in_dts_when_js_const_merges_with_class() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "const A = { };\nA.d = { };"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2451),
        0,
        ".d.ts must not emit TS2451 for JS+TS class merge; got: {diags:?}"
    );
}

/// `.js`-side check: no TS2451 should fire on `const A = {}` when the
/// merging .d.ts file declares `class A`.
#[test]
fn no_ts2451_in_js_when_dts_class_merges_with_const() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "const A = { };\nA.d = { };"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2451),
        0,
        ".js must not emit TS2451 for JS+TS class merge; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the rule is structural ("variable + class across
/// files where any side is JS with `check_js`"), not specific to the name `A`.
/// Re-run with a different identifier choice; both files must still suppress
/// TS2451.
#[test]
fn no_ts2451_with_different_class_name_two_choices() {
    for class_name in ["Widget", "MyType"] {
        let dts_src = format!("declare class {class_name} {{}}");
        let js_src = format!("const {class_name} = {{ }};\n{class_name}.d = {{ }};");
        for entry in [0, 1] {
            let diags = compile_files(
                &[("a.d.ts", dts_src.as_str()), ("b.js", js_src.as_str())],
                entry,
            );
            assert_eq!(
                count_code(&diags, 2451),
                0,
                "TS2451 must not fire for class '{class_name}' (entry={entry}); got: {diags:?}"
            );
        }
    }
}
