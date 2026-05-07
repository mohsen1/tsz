//! Regression for interface call signatures referencing their own parameters
//! via `typeof` in the return type annotation.
//!
//! `interface I { (a: number): typeof a }` is a self-referential typeof. The
//! identifier `a` is not a file-level value binding — it only exists as the
//! call signature's parameter — so resolution must go through the
//! checker's `typeof_param_scope`. The interface's structural type
//! (`get_type_of_interface`) already pushes that scope before resolving the
//! return type, but `check_interface_declaration` was eagerly calling
//! `get_type_from_type_node` on each member's type annotation in a second
//! pass without populating the scope, so a TS2304 was fabricated even though
//! the interface's structural type resolved correctly.
//!
//! The fix populates `typeof_param_scope` from the call signature's
//! parameters before re-checking the return type annotation in
//! `check_interface_declaration`. This locks in:
//!   1. No spurious TS2304 for `typeof <param>` inside an interface call
//!      signature return type.
//!   2. The fix is structural — the parameter name is irrelevant — so two
//!      different binding names are tested below.

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn interface_call_signature_typeof_param_in_return_type_resolves() {
    let source = r#"
interface Example {
    (a: number): typeof a
}
"#;
    let diags = check(source);
    let ts2304: Vec<_> = diags.iter().filter(|(c, _)| *c == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "`typeof a` in an interface call signature return type must resolve via the parameter scope, but got TS2304: {diags:?}"
    );
}

#[test]
fn interface_call_signature_typeof_param_resolves_with_alternate_name() {
    let source = r#"
interface Example {
    (k: string): typeof k
}
"#;
    let diags = check(source);
    let ts2304: Vec<_> = diags.iter().filter(|(c, _)| *c == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "Same rule must hold for any parameter name (renaming `a` to `k` should not change behavior): {diags:?}"
    );
}

/// Sibling case: `newLineInTypeofInstantiation` — a second call signature
/// with type parameters follows after a newline; the line break must keep
/// the `<T>` from being parsed as type arguments to the earlier `typeof a`.
#[test]
fn interface_typeof_param_followed_by_generic_signature_does_not_steal_type_args() {
    let source = r#"
interface Example {
    (a: number): typeof a

    <T>(): void
}
"#;
    let diags = check(source);
    let ts2304: Vec<_> = diags.iter().filter(|(c, _)| *c == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "`typeof a` must still resolve when followed by a separate generic call signature: {diags:?}"
    );
}
