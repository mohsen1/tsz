//! A cross-file global `var` whose initializer is a function/arrow/class
//! expression still participates in `TS2403`'s redeclaration-type-identity
//! check even once its name picks up a JS expando member assignment
//! (`x.prop = ...`). Unlike a bare `function x(){}` declaration (which
//! merges instead of conflicting), an ordinary `VariableDeclaration` does
//! not get an expando-driven exemption from `TS2403`.
//!
//! Structural rule (verified directly against the pinned `typescript@7.0.2`
//! oracle, bypassing `scripts/conformance/oracle.sh`'s single-file argument
//! handling): a `var`/`let`/`const` initialized with a function/arrow/class
//! expression conflicts by `TS2403` with an incompatible cross-file
//! declaration of the same name whether or not its name has picked up an
//! `x.prop = ...` expando assignment. Whichever file's declaration is bound
//! *later* in program order is the one flagged; this harness's
//! `check_multi_file` binds files in the order given in its `files` array
//! and only returns diagnostics for the entry file, so `b.ts` (the second,
//! later-bound array entry) must be the entry to observe `TS2403`.
//!
//! Mirrors `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration.ts`
//! (expects `TS2403` + `TS2339`; the real conformance harness's default
//! include globs bind `.ts` files before `.js` files, so `TS2403` there is
//! attached to `a.js`'s own diagnostics — an existing, separate file-
//! ordering question, orthogonal to this test's `var`/expando rule). A
//! prior fix here incorrectly exempted expando-container vars from
//! `TS2403` based on a misread of the oracle; this file replaces those
//! (backwards) assertions.
//!
//! The file also covers the unrelated `TS2339` half (#17443): a JS file's
//! own expando-container writes must stay clean against the container's own
//! type, not a conflicting cross-file sibling's.

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

/// The exact salsa fixture shape: `TS2403` still fires even though `a.js`'s
/// function-valued `x` has an expando member.
#[test]
fn expando_container_var_still_reports_ts2403() {
    let diags = compile_files(
        &[
            ("a.js", "var x = function foo() {}\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2403),
        1,
        "expando-container var must still conflict by TS2403; got: {diags:?}"
    );
}

/// Control: without the expando assignment, the ordinary cross-file
/// `TS2403` identity check applies the same way.
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
fn ts2403_independent_of_identifier_choices() {
    for var_name in ["widget", "thing"] {
        for expando in ["extra", "hook"] {
            let a_src = format!(
                "var {var_name} = function foo() {{}}\n{var_name}.{expando} = function bar() {{}}"
            );
            let b_src = format!("var {var_name} = function () {{ return 1; }}();");
            let diags = compile_files(&[("a.js", a_src.as_str()), ("b.ts", b_src.as_str())], 1);
            assert_eq!(
                count_code(&diags, 2403),
                1,
                "TS2403 must still fire for var '{var_name}' + expando '{expando}'; got: {diags:?}"
            );
        }
    }
}

/// An arrow-function-valued var with an expando member gets the same
/// `TS2403` treatment as a `function` expression.
#[test]
fn arrow_expando_container_still_reports_ts2403() {
    let diags = compile_files(
        &[
            ("a.js", "var x = () => {};\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2403),
        1,
        "arrow-valued expando-container var must still conflict by TS2403; got: {diags:?}"
    );
}

/// `let`/`const` mixing with a cross-file `var` of the same name reports
/// `TS2451` (block-scoped redeclaration), not `TS2403` — an expando member
/// on the `let`/`const` side doesn't change that; `TS2403` requires both
/// sides to be non-block-scoped.
#[test]
fn let_and_const_expando_containers_report_ts2451_not_ts2403() {
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
            "TS2403 requires both sides non-block-scoped; `{keyword}` must not trigger it; got: {diags:?}"
        );
    }
}

/// The `.js` side's own `x.a = ...` expando assignment must not draw a
/// `TS2339` when a `.ts` sibling declares a conflicting non-callable `x`.
/// tsc (verified against 6.0.2) is clean when the expando container is the
/// program's first / value declaration of the merged global `x`: `x`'s type
/// is that container's augmented function type, so `.a` resolves. tsz used
/// to degrade the merged symbol to the `.ts` sibling's `number` regardless
/// of order and report `TS2339`. This is the residual noted in this file's
/// header, tracked by #17443.
#[test]
fn expando_container_own_property_access_not_ts2339_when_container_is_first() {
    let diags = compile_files(
        &[
            ("a.js", "var x = function foo() {}\nx.a = function bar() {}"),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "a.js's own x.a expando write must not be TS2339 when the container is the first declaration; got: {diags:?}"
    );
}

/// Anti-hardcoding (§25): the container-wins rule is structural, not keyed on
/// `x`/`a`/`foo`. Repeat with varied binder and expando-property names.
#[test]
fn expando_container_own_property_access_not_ts2339_independent_of_names() {
    for var_name in ["widget", "handler"] {
        for expando in ["extra", "hook"] {
            let a_src = format!(
                "var {var_name} = function foo() {{}}\n{var_name}.{expando} = function bar() {{}}"
            );
            let b_src = format!("var {var_name} = function () {{ return 1; }}();");
            let diags = compile_files(&[("a.js", a_src.as_str()), ("b.ts", b_src.as_str())], 0);
            assert_eq!(
                count_code(&diags, 2339),
                0,
                "TS2339 must not fire for container '{var_name}'.'{expando}'; got: {diags:?}"
            );
        }
    }
}

/// `let`/`const`/arrow/class-expression containers get the same
/// container-wins treatment for their own property access.
#[test]
fn expando_container_own_property_access_not_ts2339_across_container_shapes() {
    let containers = [
        "var x = function foo() {}",
        "let x = function foo() {}",
        "const x = function foo() {}",
        "var x = () => {}",
        "var x = class {}",
    ];
    for container in containers {
        let a_src = format!("{container}\nx.a = function bar() {{}}");
        let diags = compile_files(
            &[
                ("a.js", a_src.as_str()),
                ("b.ts", "var x = function () { return 1; }();"),
            ],
            0,
        );
        assert_eq!(
            count_code(&diags, 2339),
            0,
            "TS2339 must not fire for container shape `{container}`; got: {diags:?}"
        );
    }
}

/// Order-dependent half of #17544: with the `.ts` sibling listed FIRST (and
/// thus bound first in program order), it is the CANONICAL declaration of
/// the merged global `x` — `number`, with no `a` member — so `a.js`'s own
/// `x.a = ...` expando write resolves through that canonical type and
/// reports `TS2339`, matching real `tsc@7.0.2` (oracle-verified via the
/// harness's real synthetic include-pattern order, see #17544's issue body;
/// the local `/opt/node22` `tsc` 6.0.2 disagrees and is not the pinned
/// oracle). This is exactly `jsContainerMergeTsDeclaration.ts`'s shape
/// under the conformance harness's real `.ts`-before-`.js` include-glob
/// ordering.
#[test]
fn expando_container_own_property_access_reports_ts2339_when_sibling_is_canonical() {
    let diags = compile_files(
        &[
            ("b.ts", "var x = function () { return 1; }();"),
            ("a.js", "var x = function foo() {}\nx.a = function bar() {}"),
        ],
        1,
    );
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "a.js's own container write must resolve through the canonical (earlier-processed) sibling type; got: {diags:?}"
    );
}

/// Control: a genuinely-absent member on the container still reports TS2339 —
/// the fix must not blanket-suppress property errors on the container.
#[test]
fn missing_member_on_expando_container_still_reports_ts2339() {
    let diags = compile_files(
        &[
            (
                "a.js",
                "var x = function foo() {}\nx.a = function bar() {}\nx.b();",
            ),
            ("b.ts", "var x = function () { return 1; }();"),
        ],
        0,
    );
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "a genuinely-absent member (x.b) must still be TS2339; got: {diags:?}"
    );
}
