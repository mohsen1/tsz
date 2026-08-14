//! A `var`/`let`/`const` variable does not declaration-merge with a class —
//! only a `function` declaration merges with a class's static side. So when
//! a `.d.ts` `declare class A {}` and a `.js` `const A = {}` share a name
//! (with `allowJs` + `checkJs`), both files report `TS2451` ("Cannot
//! redeclare block-scoped variable") exactly as an ordinary cross-file
//! block-scoped-variable-vs-class conflict would — `checkJs` does not
//! suppress it, and the conflicting initializer must not be checked for
//! assignability against the class's shape (`TS2739`/`TS2741`) — there is no
//! merge to check it against. Verified against the pinned `typescript@7.0.2`
//! oracle: `declare class A {}` + `const A = {}` reports `TS2451` on *both*
//! sides and nothing else.
//!
//! Mirrors `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration3.ts`
//! (`expected:[TS2339,TS2451]`). tsc additionally resolves `A`'s type for
//! *later* references (e.g. an `A.d` property access after the conflicting
//! declaration) to the class's static side, reporting `TS2339` there — that
//! half is covered by
//! `crates/tsz-cli/tests/js_container_merge_class_variable_conflict_expando_cli_tests.rs`
//! (the real project-mode driver, not the entry-only harness used here,
//! is required: the fix depends on the production `global_symbol_file_index`
//! cross-arena merge that unifies `A`'s `.d.ts`/`.js` declarations under one
//! `SymbolId`, which none of the lightweight multi-file test harnesses in
//! this crate reproduce).

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318) // ignore lib-not-loaded noise
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// `.d.ts`-side check: `TS2451` fires on `declare class A {}` when a JS file
/// declares `const A = {}` — `checkJs` does not suppress the redeclaration.
#[test]
fn ts2451_in_dts_when_js_const_conflicts_with_class() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "const A = { };\nA.d = { };"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2451),
        1,
        ".d.ts must emit TS2451 for a JS `const` vs TS `class` name conflict; got: {diags:?}"
    );
}

/// `.js`-side check: `TS2451` fires on `const A = {}` when the conflicting
/// `.d.ts` file declares `class A`, and the expando assignment `A.d = {}`
/// is otherwise treated as an ordinary JS expando on the `const`'s own
/// object-literal type (no `TS2739`/`TS2741` from checking it against the
/// unrelated class shape).
#[test]
fn ts2451_in_js_when_dts_class_conflicts_with_const() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "const A = { };\nA.d = { };"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2451),
        1,
        ".js must emit TS2451 for a JS `const` vs TS `class` name conflict; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2739),
        0,
        "must not check the initializer against the unrelated class shape; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the rule is structural ("cross-file block-scoped
/// variable vs class"), not specific to the name `A`. Re-run with a
/// different identifier choice; both files must still report `TS2451`.
#[test]
fn ts2451_with_different_class_name_two_choices() {
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
                1,
                "TS2451 must fire for class '{class_name}' (entry={entry}); got: {diags:?}"
            );
        }
    }
}

/// Anti-hardcoding (§25): the rule is structural over names. Repeat the
/// salsa container-merge case with two different class names AND two
/// different expando property names. `TS2451` must hold in every
/// combination, and neither `TS2739` nor `TS2741` (the merge-shaped
/// diagnostics) may fire — there is no merge.
#[test]
fn ts2451_independent_of_identifier_choices() {
    for class_name in ["Widget", "Foo"] {
        for expando in ["d", "extra"] {
            let dts_src = format!("declare class {class_name} {{}}");
            let js_src = format!("const {class_name} = {{ }};\n{class_name}.{expando} = {{ }};");
            let diags = compile_files(
                &[("a.d.ts", dts_src.as_str()), ("b.js", js_src.as_str())],
                1,
            );
            assert_eq!(
                count_code(&diags, 2451),
                1,
                "TS2451 must fire for class '{class_name}' + expando '{expando}'; got: {diags:?}"
            );
            assert_eq!(
                count_code(&diags, 2739),
                0,
                "TS2739 must not fire — a variable never merges with a class; got: {diags:?}"
            );
            assert_eq!(
                count_code(&diags, 2741),
                0,
                "TS2741 must not fire — a variable never merges with a class; got: {diags:?}"
            );
        }
    }
}

/// A `function` declaration (unlike `const`/`let`/`var`) genuinely merges
/// with a class's static side — `FUNCTION_EXCLUDES` omits `CLASS` — so no
/// `TS2451`/`TS2300` fires and the expando is checked against the merged
/// static shape. Positive control proving the variable-specific fix above
/// didn't overreach into the legitimate function/class merge path.
#[test]
fn function_expando_merges_with_dts_class_without_redeclaration_error() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A { static x: number; }"),
            ("b.js", "function A() {}\nA.x = 1;"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2451),
        0,
        "function/class merge must not report TS2451; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2300),
        0,
        "function/class merge must not report TS2300; got: {diags:?}"
    );
}

/// `var` (function-scoped, not block-scoped) vs a `.d.ts` class reports
/// `TS2300` ("Duplicate identifier"), not `TS2451` — same non-merge rule,
/// different redeclaration diagnostic because `var` isn't block-scoped.
#[test]
fn ts2300_not_ts2451_when_var_conflicts_with_dts_class() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "var A = { };\nA.d = { };"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2451),
        0,
        "var vs class must not report TS2451 (var is function-scoped); got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2300),
        1,
        "var vs class must report TS2300 (duplicate identifier); got: {diags:?}"
    );
}

/// Negative control: without the conflicting `.d.ts` class, the same
/// empty-object-literal expando write on a plain JS `const` stays clean —
/// proving the companion CLI-level fix (see the module doc comment) is
/// scoped to the CLASS+VARIABLE conflict shape and does not disable
/// ordinary JS expando writes.
#[test]
fn ts2339_does_not_fire_for_plain_js_expando_without_class_conflict() {
    let diags = compile_files(&[("b.js", "const A = { };\nA.d = { };")], 0);
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "plain JS expando write must stay clean without a conflicting class; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2451),
        0,
        "no conflict expected; got: {diags:?}"
    );
}
