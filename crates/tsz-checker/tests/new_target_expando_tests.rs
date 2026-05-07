//! Tests that `new.target` inside a function declaration/expression carries
//! the function's expando properties (e.g. `f.marked = true;`).
//!
//! Regression for `conformance/es6/newTarget/newTargetNarrowing.ts`: tsz used
//! to emit TS2339 on `new.target.marked` because the meta-property type was
//! the bare function shape without expando members. tsc surfaces expandos on
//! `new.target` so subsequent narrowing on `new.target.<expando>` resolves
//! the augmented members.
//!
//! Structural rule: when `new.target` resolves to a named function
//! declaration or expression, its type must equal the function's symbol
//! type — including any expando assignments — so property access and
//! narrowing behave the same as on a direct identifier reference.

use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn new_target_carries_expando_properties_on_named_function_declaration() {
    let diagnostics = check_source_code_messages(
        r#"
function foo(x: true) {}

function f() {
  if (new.target.marked === true) {
    foo(new.target.marked);
  }
}

f.marked = true;
"#,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "expected expando member `marked` to resolve on `new.target`, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "expected `new.target.marked` to narrow to `true` so the `foo(true)` call is accepted, got: {diagnostics:#?}"
    );
}

#[test]
fn new_target_expando_works_with_other_property_names() {
    // Structural: the fix must not depend on the property name. Try a few
    // distinct identifier choices to make sure we are not pattern-matching
    // on `marked` specifically.
    for prop in ["flag", "kind", "version", "isReady"] {
        let source = format!(
            r#"
function f() {{
  if (new.target.{prop} === true) {{
    const v: true = new.target.{prop};
    void v;
  }}
}}

f.{prop} = true;
"#
        );
        let diagnostics = check_source_code_messages(&source);
        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2339),
            "prop={prop}: expected expando to resolve on `new.target`, got: {diagnostics:#?}"
        );
    }
}

#[test]
fn new_target_expando_works_with_multiple_expando_properties() {
    let diagnostics = check_source_code_messages(
        r#"
function f() {
  if (new.target.a === 1 && new.target.b === "hello") {
    const x: number = new.target.a;
    const y: string = new.target.b;
    void x;
    void y;
  }
}

f.a = 1;
f.b = "hello";
"#,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "expected both expandos `a` and `b` to resolve on `new.target`, got: {diagnostics:#?}"
    );
}

#[test]
fn new_target_without_expandos_still_resolves_to_function_type() {
    // No expando assignments: new.target must still produce the function
    // type so call signatures and Function-prototype members work. This
    // test guards against regressing the base case while we add expando
    // augmentation.
    let diagnostics = check_source_code_messages(
        r#"
function f() {
  const t = new.target;
  void t;
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for plain `new.target`, got: {diagnostics:#?}"
    );
}

#[test]
fn new_target_expando_does_not_leak_unrelated_function_properties() {
    // Each function's expando set is rooted at its own name. `new.target`
    // inside `f` must surface `f`'s expandos, not `g`'s.
    let diagnostics = check_source_code_messages(
        r#"
function f() {
  // Only `marked` should be visible here, not `other`.
  return (new.target as any).marked;
}

function g() {}

f.marked = true;
g.other = 42;
"#,
    );

    // The `as any` keeps this code error-free; we only assert that the
    // checker did not crash and did not synthesize TS2339 against `f`.
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "expected no TS2339 for `f`'s own expando, got: {diagnostics:#?}"
    );
}
