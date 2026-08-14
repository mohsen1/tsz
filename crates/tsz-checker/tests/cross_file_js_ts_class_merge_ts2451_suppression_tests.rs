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
//! declaration) to the class's static side, reporting `TS2339` there — a
//! `mergeSymbol` artifact (`checker.ts`): on a flag conflict the pre-existing
//! target symbol is left untouched and the conflicting source's declarations
//! feed only the diagnostic, so *which* declaration wins is decided by file
//! processing order, not by declaration order within a file.
//!
//! `cross_file_variable_class_merge.rs` implements this for ordinary
//! (non-`checkJs`) property-type resolution — see the `ts2339_*` tests
//! below, which cover the rule in pure TS. The `.js` + `checkJs` variant of
//! the same fixture (`ts2451_in_js_when_dts_class_conflicts_with_const`
//! below) still does not report `TS2339`: a `checkJs` direct-write target
//! runs assignment-target expando-write machinery
//! (`property_access_helpers/expando.rs`,
//! `types/property_access_type/resolve.rs`) ahead of the generic
//! property-type path, and that machinery grants the write before
//! `cross_file_variable_class_merge.rs`'s resolution is ever consulted —
//! traced to at least `is_expando_function_assignment` short-circuiting
//! before the generic property-access-type path is reached, but the exact
//! grant site was not pinned down. Not fixed here; left for a follow-up.

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
/// unrelated class shape). Per the module doc comment, the `TS2339` half of
/// `jsContainerMergeTsDeclaration3.ts`'s expectation is not yet covered for
/// this `.js` + `checkJs` shape (see `ts2339_fires_for_pure_ts_cross_file_conflict_no_js_involved`
/// below for the covered pure-TS shape of the same rule).
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

/// Anti-hardcoding (§25) + generality: the earlier-processed-class rule is
/// not JS-specific — it is a plain cross-file symbol-merge artifact that
/// applies identically between two ordinary `.ts` files with no `allowJs`/
/// `checkJs` involved at all. Oracle-verified (`typescript@7.0.2`). This is
/// the shape `cross_file_variable_class_merge.rs` fixes.
#[test]
fn ts2339_fires_for_pure_ts_cross_file_conflict_no_js_involved() {
    let diags = compile_files(
        &[
            ("a.ts", "declare class Widget {}"),
            ("b.ts", "const Widget = { };\nWidget.extra = { };"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2451),
        1,
        "b.ts must emit TS2451 for the cross-file class/const conflict; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "Widget.extra must resolve `Widget` to the earlier-processed class; got: {diags:?}"
    );
}

/// Reversing file processing order flips which declaration wins the shared
/// script-global symbol: when the variable's own file is processed *before*
/// the conflicting class's file, `Widget`'s value type stays the variable's
/// own plain `{}` — `Widget.extra = {}` still reports `TS2339` (a strict-TS
/// object literal never grants expando writes, merge or no merge), but
/// against `{}`, not `typeof Widget` — oracle-verified (`typescript@7.0.2`)
/// as the mirror image of the test above. `TS2451` still fires on both
/// sides regardless of order; only the resolved value type — visible in the
/// diagnostic's type name — is order-dependent.
#[test]
fn variable_ts_file_processed_before_conflicting_class_file_keeps_own_type() {
    let diags = compile_files(
        &[
            ("a.ts", "const Widget = { };\nWidget.extra = { };"),
            ("b.ts", "declare class Widget {}"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2451),
        1,
        "a.ts must still emit TS2451 even though its own variable wins the merge; got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| *code == 2339 && msg.contains("'{}'")),
        "an earlier-processed variable must resolve TS2339 against its own `{{}}` type, not `typeof Widget`; got: {diags:?}"
    );
}
