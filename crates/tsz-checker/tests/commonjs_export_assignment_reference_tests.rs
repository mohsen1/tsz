//! Type of a `module.exports` reference under the TS2309 export-assignment mix.
//!
//! Oracle (`typescript@7.0.2`, `--allowJs --checkJs --module commonjs`):
//! in a JS file that mixes a bare `module.exports = X` with sibling
//! `exports.p` / `module.exports.p` property exports (the TS2309 surface),
//! a *read* of `module.exports` types as exactly the export= target `X` —
//! so `module.exports(...)` on a function target is callable (no TS2349),
//! and only the sibling property writes surface TS2339. That holds for reads
//! inside a sibling export's own function-expression RHS (the shape
//! `salsa/moduleExportAssignment2` pins), where tsz used to type the read
//! against an empty re-entrancy placeholder namespace. A genuinely
//! non-callable target still reports TS2349, and without the mix the
//! namespace-typed reference keeps its members.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single JS file under `allowJs`/`checkJs` and return
/// `(code, start_offset)` per diagnostic.
fn check_js_positioned(file_name: &str, source: &str) -> Vec<(u32, u32)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: false,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        // The no-lib harness reports TS2318 (cannot find global type) for the
        // ambient globals every file touches; that is harness setup, not the
        // CommonJS reference typing under test.
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.start))
        .collect()
}

fn codes(diags: &[(u32, u32)]) -> Vec<u32> {
    let mut codes: Vec<u32> = diags.iter().map(|(code, _)| *code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

#[test]
fn export_assignment_mix_call_of_module_exports_is_callable() {
    // tsc: TS2309 (the mix) + TS2339 (the sibling write), no TS2349 —
    // `module.exports` reads as the export= target `() => void`.
    let source = "module.exports = function () {};\nmodule.exports.prop = 1;\nmodule.exports();\n";
    let diags = check_js_positioned("mixcall.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339], "got {diags:?}");
}

#[test]
fn export_assignment_mix_aliased_local_call_is_callable() {
    let source =
        "module.exports = function () {};\nexports.zeb = 1;\nvar gorp = module.exports;\ngorp();\n";
    let diags = check_js_positioned("mixalias.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339], "got {diags:?}");
}

#[test]
fn export_assignment_mix_property_read_off_target_reports_ts2339() {
    // Reading a sibling name back off `module.exports` is a member miss on
    // the export= target type, exactly like the write.
    let source =
        "module.exports = function () {};\nexports.blah = 1;\nvar v = module.exports.blah;\n";
    let diags = check_js_positioned("mixread.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339], "got {diags:?}");
    let ts2339_count = diags.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(ts2339_count, 2, "write and read both miss, got {diags:?}");
}

#[test]
fn mix_call_inside_sibling_export_rhs_function_body_is_callable() {
    // Minimal re-entrancy repro: the call sits inside the function expression
    // whose type the export-surface computation itself is inferring. The read
    // must resolve to the in-progress direct export type, not the empty
    // placeholder namespace.
    let source = "module.exports = function () {};\nmodule.exports.later = function () {\n    module.exports()\n}\n";
    let diags = check_js_positioned("mixbody.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339], "got {diags:?}");
}

#[test]
fn var_nested_export_assignment_calls_are_callable_from_function_body() {
    // Mirrors salsa/moduleExportAssignment2 (renamed binders): the export
    // assignment nests in a var initializer, and both the var alias and
    // `module.exports` itself are called inside a sibling export's function
    // RHS. tsc (noImplicitAny on): TS2309 + TS2339 + one TS7006 per untyped
    // parameter, and no TS2349.
    let source = "var zog = module.exports = function (tree) {\n}\nmodule.exports.later = function (tree) {\n    zog(tree)\n    module.exports(tree)\n}\n";
    let diags = check_js_positioned("varmix.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339, 7006], "got {diags:?}");
    let ts7006_count = diags.iter().filter(|(code, _)| *code == 7006).count();
    assert_eq!(ts7006_count, 2, "one TS7006 per parameter, got {diags:?}");
}

#[test]
fn export_assignment_mix_noncallable_target_keeps_ts2349() {
    // Negative control: the export= target itself has no call signatures, so
    // the call still fails — as TS2349 on the object type, matching tsc.
    let source = "module.exports = { zig: 1 };\nexports.zag = 2;\nmodule.exports();\n";
    let diags = check_js_positioned("mixnoncallable.js", source);
    assert_eq!(codes(&diags), vec![2309, 2339, 2349], "got {diags:?}");
}

#[test]
fn unmixed_function_export_call_stays_callable() {
    // Control: no property-export siblings, no TS2309 — the call is clean.
    let source = "module.exports = function () {};\nmodule.exports();\n";
    let diags = check_js_positioned("plaincall.js", source);
    assert_eq!(diags, vec![], "got {diags:?}");
}

#[test]
fn unmixed_call_inside_function_body_stays_callable() {
    let source =
        "module.exports = function () {};\nfunction f() {\n    module.exports()\n}\nf();\n";
    let diags = check_js_positioned("plainbody.js", source);
    assert_eq!(diags, vec![], "got {diags:?}");
}

#[test]
fn unmixed_namespace_reference_keeps_members() {
    // Control: property exports without an export assignment — reading a
    // member off `module.exports` resolves through the namespace surface.
    // (The read sits in a function body: a top-level read straight after the
    // assignment is tsc's TS2565 use-before-assigned territory instead.)
    let source = "exports.first = 1;\nfunction later() { return module.exports.first; }\nvar use = later();\n";
    let diags = check_js_positioned("plainns.js", source);
    assert_eq!(diags, vec![], "got {diags:?}");
}
