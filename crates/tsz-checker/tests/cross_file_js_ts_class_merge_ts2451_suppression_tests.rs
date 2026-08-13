//! Cross-file JS/TS declaration-merge behavior for a `.js` value declaration
//! sharing a name with a `.d.ts` `declare class` (with `allowJs` +
//! `checkJs`).
//!
//! Structural rule, verified against the pinned tsc oracle for
//! `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration3.ts`
//! (`var A = {}` shape) and its `const` variant:
//! - `var X = <init>` is function/global scoped, so it never collides with
//!   the class's block-scoped binding — tsc treats it as a container/expando
//!   merge: `X.prop = ...` assignments augment the merged static type, and
//!   the initializer is checked against that merged type (TS2739 when it is
//!   missing required members, never TS2741).
//! - `const`/`let X = <init>` share the class's block-scoped binding, so tsc
//!   reports a genuine redeclaration (TS2451 on *both* the `.d.ts` and `.js`
//!   declarations) instead. No merged type is synthesized, so the
//!   initializer is not checked against the class shape (no TS2739).
//!
//! Originally this file asserted the opposite for the `const` case (no
//! TS2451, only TS2739) based on a misreading of
//! `jsContainerMergeTsDeclaration3.ts` — the pinned `typescript@7.0.2`
//! oracle (`scripts/conformance/tsc-cache-full.json`) actually reports
//! `TS2339`/`TS2451` for that exact fixture, not `TS2739`.

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

/// `.d.ts`-side check: TS2451 fires on `declare class A {}` when a JS file
/// declares the block-scoped `const A = {}` (genuine redeclaration).
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
        ".d.ts must emit TS2451 for a block-scoped JS/TS class conflict; got: {diags:?}"
    );
}

/// `.js`-side check: TS2451 fires on `const A = {}` when the conflicting
/// `.d.ts` file declares `class A`.
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
        ".js must emit TS2451 for a block-scoped JS/TS class conflict; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the rule is structural ("block-scoped variable +
/// class across files"), not specific to the name `A`. Re-run with a
/// different identifier choice; both files must still report TS2451.
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

/// The block-scoped `const`/class conflict does not synthesize a merged
/// declared type: the initializer is never checked against `typeof A`, so
/// TS2739/TS2741 must not fire alongside the TS2451 redeclaration.
#[test]
fn const_class_conflict_does_not_emit_ts2739_or_ts2741() {
    let diags = compile_files(
        &[
            ("a.d.ts", "declare class A {}"),
            ("b.js", "const A = { };\nA.d = { };"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2739),
        0,
        "block-scoped conflict must not check the initializer against the class shape; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2741),
        0,
        "block-scoped conflict must not check the initializer against the class shape; got: {diags:?}"
    );
}

/// `var` (function/global scoped) does NOT conflict with the class's
/// block-scoped binding: no TS2451, and the `var` initializer is checked
/// against the merged static type instead (TS2739, not TS2741, when both
/// `prototype` and a JS-augmented member are missing).
///
/// Mirrors `conformance/salsa/jsContainerMergeTsDeclaration3.ts`'s `var`
/// shape (verified against the pinned tsc oracle).
#[test]
fn var_class_container_merge_emits_ts2739_not_ts2451_or_ts2741() {
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
        "`var` must not conflict with the class's block-scoped binding; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2741),
        0,
        "must not emit TS2741 (single missing property) when both `prototype` and a JS expando are missing; got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2739),
        1,
        "must emit TS2739 (multiple missing properties) for the merged `typeof A` static side; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the `var` container-merge rule is structural over
/// names. Repeat with two different class names AND two different expando
/// property names.
#[test]
fn var_class_container_merge_ts2739_independent_of_identifier_choices() {
    for class_name in ["Widget", "Foo"] {
        for expando in ["d", "extra"] {
            let dts_src = format!("declare class {class_name} {{}}");
            let js_src = format!("var {class_name} = {{ }};\n{class_name}.{expando} = {{ }};");
            let diags = compile_files(
                &[("a.d.ts", dts_src.as_str()), ("b.js", js_src.as_str())],
                1,
            );
            assert_eq!(
                count_code(&diags, 2451),
                0,
                "TS2451 must not fire for `var` + class '{class_name}' (expando='{expando}'); got: {diags:?}"
            );
            assert_eq!(
                count_code(&diags, 2741),
                0,
                "TS2741 must not fire for class '{class_name}' + expando '{expando}'; got: {diags:?}"
            );
            assert_eq!(
                count_code(&diags, 2739),
                1,
                "TS2739 must fire for class '{class_name}' + expando '{expando}'; got: {diags:?}"
            );
        }
    }
}
