//! A cross-file global `var` whose initializer is a function/arrow/class
//! expression, and whose name later picks up a JS expando member assignment
//! (`x.prop = ...`), is exempt from `TS2403`'s redeclaration-type-identity
//! check — the same exemption a bare `function x(){}` declaration already
//! gets — even though it is syntactically an ordinary `VariableDeclaration`.
//!
//! Structural rule (verified against the pinned `typescript@7.0.2` oracle):
//! when a `var`/`let`/`const` is initialized with a function/arrow/class
//! expression AND its name has at least one `x.prop = ...` expando
//! assignment anywhere in the project, tsc treats it as a function-like
//! container for declaration-merge purposes and does not compare its type
//! against another file's declaration of the same name. Without the
//! expando assignment, the ordinary `TS2403` identity check still applies.
//!
//! Mirrors `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`
//! (expects zero diagnostics). This fix covers the `TS2403` half; a
//! separate `TS2339` false positive remains on `a.js`'s own `x.a = ...`
//! expando assignment (the merged symbol's declared type used for that
//! local property access is not yet fixed by this change) — not asserted
//! here, tracked separately.

use tsz_checker::context::CheckerOptions;

fn compile_files(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let entry_file = files[entry_idx].0;
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
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

/// The exact salsa fixture shape, checked from the `.ts` side: no `TS2403`
/// once `a.js`'s function-valued `x` has an expando member.
#[test]
fn expando_container_var_suppresses_ts2403_from_ts_side() {
    let diags = compile_files(
        &[
            ("a.js", "var x = function foo() {}\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2403),
        0,
        "expando-container var must not conflict by TS2403; got: {diags:?}"
    );
}

/// Same pair, checked from the `.js` side (symmetric — the exemption must
/// not depend on which file is the entry/self side of the comparison).
#[test]
fn expando_container_var_suppresses_ts2403_from_js_side() {
    let diags = compile_files(
        &[
            ("a.js", "var x = function foo() {}\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2403),
        0,
        "expando-container var must not conflict by TS2403 (JS-side entry); got: {diags:?}"
    );
}

/// Control: without the expando assignment, the ordinary cross-file
/// `TS2403` identity check still applies (oracle-verified — removing
/// `x.a = ...` makes tsc itself report `TS2403` here).
#[test]
fn non_expando_function_valued_var_still_reports_ts2403() {
    let diags = compile_files(
        &[
            ("a.js", "var x = function foo() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2403),
        1,
        "a bare function-valued var (no expando) must still conflict by TS2403; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the rule is structural over names — repeat with
/// different identifiers and expando property names.
#[test]
fn expando_container_exemption_independent_of_identifier_choices() {
    for var_name in ["widget", "thing"] {
        for expando in ["extra", "hook"] {
            let a_src = format!(
                "var {var_name} = function foo() {{}}\n{var_name}.{expando} = function bar() {{}}"
            );
            let b_src = format!("var {var_name} = function () {{ return 1; }}();");
            let diags = compile_files(&[("a.js", a_src.as_str()), ("b.ts", b_src.as_str())], 1);
            assert_eq!(
                count_code(&diags, 2403),
                0,
                "TS2403 must not fire for var '{var_name}' + expando '{expando}'; got: {diags:?}"
            );
        }
    }
}

/// `let`/`const` initialized with a function expression get the same
/// expando-container exemption as `var` — the rule keys off the
/// initializer shape and the expando assignment, not the declaration kind.
#[test]
fn let_and_const_expando_containers_also_suppress_ts2403() {
    for keyword in ["let", "const"] {
        let a_src = format!("{keyword} x = function foo() {{}}\nx.a = function bar() {{}}");
        let diags = compile_files(
            &[
                ("a.js", a_src.as_str()),
                ("b.ts", "var x = function () { return 1; }();"),
            ],
            1,
        );
        assert_eq!(
            count_code(&diags, 2403),
            0,
            "expando-container `{keyword}` must not conflict by TS2403; got: {diags:?}"
        );
    }
}

/// An arrow-function-valued var with an expando member gets the same
/// exemption as a `function` expression.
#[test]
fn arrow_expando_container_suppresses_ts2403() {
    let diags = compile_files(
        &[
            ("a.js", "var x = () => {};\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2403),
        0,
        "arrow-valued expando-container var must not conflict by TS2403; got: {diags:?}"
    );
}
