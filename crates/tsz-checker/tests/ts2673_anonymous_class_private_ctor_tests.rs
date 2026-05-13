//! Tests for TS2673 / TS2674 on anonymous class expressions whose
//! constructors are private / protected.
//!
//! Closes #6012. The structural rule:
//!
//! > When `new <receiver>(...)` targets an anonymous class expression
//! > (literal `class { ... }`, possibly wrapped in parentheses) and
//! > that class's constructor is private / protected, the `new`
//! > expression must lie lexically inside that class expression's
//! > body. Otherwise tsc emits:
//! >  - TS2673 — private: "Constructor of class '(Anonymous class)'
//! >    is private and only accessible within the class declaration."
//! >  - TS2674 — protected: same shape.
//!
//! The fix lives in `check_constructor_accessibility_for_new`
//! (`classes/constructor_checker.rs`). Previously the symbol-lookup
//! early return skipped the entire visibility check for anonymous
//! receivers; now we route through `class_expression_from_new_expr` +
//! `new_expr_within_class_expression_body` and emit with the literal
//! `"(Anonymous class)"` display name.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diags_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn has_ts2673(diags: &[(u32, String)]) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == 2673 && m.contains("(Anonymous class)") && m.contains("private"))
}

fn has_ts2674(diags: &[(u32, String)]) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == 2674 && m.contains("(Anonymous class)") && m.contains("protected"))
}

#[test]
fn private_ctor_anon_class_immediately_invoked_errors() {
    // Direct repro from #6012.
    let source = r#"
const singleton = new (class {
  private constructor() {}
  static instance = new this();
})();
"#;
    let diags = diags_strict(source);
    assert!(
        has_ts2673(&diags),
        "expected TS2673 (Anonymous class) at outer `new`, got: {diags:?}",
    );
}

#[test]
fn protected_ctor_anon_class_immediately_invoked_errors() {
    // Protected variant — anti-hardcoding: not specific to the `private` keyword.
    let source = r#"
const inst = new (class {
  protected constructor() {}
})();
"#;
    let diags = diags_strict(source);
    assert!(
        has_ts2674(&diags),
        "expected TS2674 (Anonymous class) at outer `new`, got: {diags:?}",
    );
}

#[test]
fn private_ctor_anon_class_without_parens_errors() {
    // Some grammars accept `new class {...}()` without surrounding parens.
    // The fix uses `skip_parenthesized`, so it should work either way.
    let source = r#"
const x = new class {
  private constructor() {}
}();
"#;
    let diags = diags_strict(source);
    assert!(
        has_ts2673(&diags),
        "expected TS2673 (Anonymous class) without parens, got: {diags:?}",
    );
}

#[test]
fn public_ctor_anon_class_no_diagnostic() {
    // Regression guard: a public constructor must NOT trigger the new
    // anonymous-class path.
    let source = r#"
const x = new (class {
  constructor() {}
})();
"#;
    let diags = diags_strict(source);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2673 || *c == 2674),
        "public anonymous-class constructor must not emit TS2673/TS2674, got: {diags:?}",
    );
}

#[test]
fn private_ctor_anon_class_inner_new_this_allowed() {
    // The inner `new this()` is inside the class expression body; it must
    // not get TS2673. (The outer call does — already covered above.)
    let source = r#"
new (class {
  private constructor() {}
  static instance = new this();
})();
"#;
    let diags = diags_strict(source);
    // Count occurrences of TS2673: we expect exactly ONE (the outer `new`),
    // not two. If both fired we'd see two.
    let ts2673_count = diags.iter().filter(|(c, _)| *c == 2673).count();
    assert_eq!(
        ts2673_count, 1,
        "exactly one TS2673 expected (outer new only), got: {diags:?}",
    );
}

#[test]
fn private_ctor_named_class_still_errors() {
    // Regression guard: the existing named-class path must keep emitting
    // TS2673 — the new anonymous fallback runs only when no class symbol
    // resolves.
    let source = r#"
class C {
  private constructor() {}
}
new C();
"#;
    let diags = diags_strict(source);
    assert!(
        diags.iter().any(|(c, m)| *c == 2673 && m.contains("'C'")),
        "expected TS2673 with class name 'C', got: {diags:?}",
    );
}
