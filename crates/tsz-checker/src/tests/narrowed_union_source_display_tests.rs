//! Regression coverage for the *source-type display* of a flow-narrowed
//! identifier in assignability diagnostics.
//!
//! When a source identifier whose declared type is a named union/alias is
//! flow-narrowed to a strict subset (a discriminated-union parameter narrowed
//! to one member, or a union narrowed to a sub-union), `tsc` drops the
//! `aliasSymbol` on `filterType`/`getNarrowedType` and renders the narrowed
//! *structural* type. tsz previously repainted the narrowed source with its
//! declared alias name (`Shape`, `U`) — and, on the structural fallback, even
//! widened the narrowed literal discriminant (`"square"` -> `string`). These
//! tests pin the structural narrowed display and the not-narrowed control
//! (where the alias name is correctly preserved).
//!
//! The rule is structural, never name-keyed: the renamed-binder variants below
//! vary the alias name, the discriminant property name, and the member spelling
//! to prove the fix reads the narrowed type's shape, not any identifier string.

use crate::test_utils::check_source_diagnostics;

fn source_not_assignable_message(diags: &[crate::diagnostics::Diagnostic]) -> String {
    diags
        .iter()
        .find(|d| d.code == 2322 || d.code == 2741)
        .map_or_else(
            || {
                panic!(
                    "expected a TS2322/TS2741 assignability diagnostic; got: {:?}",
                    diags
                        .iter()
                        .map(|d| (d.code, &d.message_text))
                        .collect::<Vec<_>>()
                )
            },
            |d| d.message_text.clone(),
        )
}

#[test]
fn discriminated_union_narrowed_to_member_renders_structural_member_not_alias() {
    // `sh` narrows to the `"square"` member in the `default` arm; tsc renders
    // `{ kind: "square"; s: number; }`, not the alias `Shape`.
    let diags = check_source_diagnostics(
        r#"
type Shape = { kind: "circle"; r: number } | { kind: "square"; s: number };
function f(sh: Shape) {
  if (sh.kind === "circle") return;
  const bad: number = sh;
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains(r#"{ kind: "square"; s: number; }"#),
        "narrowed source must render the structural member with its literal \
         discriminant preserved; got: {msg}"
    );
    assert!(
        !msg.contains("'Shape'"),
        "narrowed source must not repaint to the declared alias name; got: {msg}"
    );
}

#[test]
fn discriminated_union_narrowed_to_sub_union_renders_structural_union_not_alias() {
    // `u` narrows to the two-member residual `{ k: "b" } | { k: "c" }`; tsc
    // renders that sub-union structurally, not the three-member alias `U`.
    let diags = check_source_diagnostics(
        r#"
type U = { k: "a"; x: number } | { k: "b"; y: number } | { k: "c"; z: number };
function f(u: U) {
  if (u.k === "a") return;
  const bad: number = u;
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains(r#"{ k: "b"; y: number; } | { k: "c"; z: number; }"#),
        "narrowed source must render the residual sub-union structurally; got: {msg}"
    );
    assert!(
        !msg.contains("'U'"),
        "narrowed sub-union source must not repaint to the declared alias name; got: {msg}"
    );
}

#[test]
fn object_union_narrowed_via_in_operator_renders_structural_member() {
    // `in`-operator narrowing of a named object union to a single member.
    let diags = check_source_diagnostics(
        r#"
type Payload = { a: number } | { b: string };
function f(p: Payload) {
  if ("a" in p) return;
  const bad: { a: number } = p;
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains("{ b: string; }"),
        "narrowed-via-`in` source must render the surviving member structurally; got: {msg}"
    );
    assert!(
        !msg.contains("'Payload'"),
        "narrowed source must not repaint to the declared alias name; got: {msg}"
    );
}

#[test]
fn exhaustiveness_never_assignment_renders_narrowed_member_not_alias() {
    // The classic exhaustiveness pattern: the unhandled member is assigned to
    // `never`. tsc shows the unhandled member, not the full alias.
    let diags = check_source_diagnostics(
        r#"
type Shape = { kind: "circle"; r: number } | { kind: "square"; s: number };
function area(sh: Shape): number {
  switch (sh.kind) {
    case "circle": return sh.r;
    default: {
      const _exhaustive: never = sh;
      return _exhaustive;
    }
  }
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains(r#"{ kind: "square"; s: number; }"#),
        "unhandled member assigned to never must render structurally; got: {msg}"
    );
    assert!(
        !msg.contains("'Shape'"),
        "unhandled member must not repaint to the declared alias name; got: {msg}"
    );
}

#[test]
fn not_narrowed_union_alias_keeps_its_name() {
    // Control: with no narrowing, the full union is still the alias `U`, and
    // tsc keeps the alias name. The fix must not strip the alias here.
    let diags = check_source_diagnostics(
        r#"
type U = { a: number } | { b: string };
function f(u: U) {
  const bad: { a: number } = u;
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains("'U'"),
        "an un-narrowed union alias source must keep its declared name; got: {msg}"
    );
}

#[test]
fn narrowing_display_rule_is_structural_not_name_keyed() {
    // Renamed-binder variant: a different alias name (`Figure`), a different
    // discriminant property (`tag`), and different member spellings. The fix
    // reads the narrowed shape, so the structural member is rendered exactly as
    // for `Shape` above — proving the rule is not keyed on any identifier text.
    let diags = check_source_diagnostics(
        r#"
type Figure = { tag: "dot"; px: number } | { tag: "line"; len: number };
function g(fig: Figure) {
  if (fig.tag === "dot") return;
  const bad: number = fig;
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains(r#"{ tag: "line"; len: number; }"#),
        "renamed-binder narrowed source must render structurally; got: {msg}"
    );
    assert!(
        !msg.contains("'Figure'"),
        "renamed-binder narrowed source must not repaint to the alias name; got: {msg}"
    );
}

#[test]
fn typeof_narrowed_primitive_union_source_unchanged() {
    // Control: typeof narrowing of a primitive union already rendered the
    // narrowed member structurally; the new guard must not perturb it.
    let diags = check_source_diagnostics(
        r#"
function f(x: string | number) {
  if (typeof x === "string") {
    const bad: number = x;
  }
}
"#,
    );
    let msg = source_not_assignable_message(&diags);
    assert!(
        msg.contains("Type 'string' is not assignable to type 'number'"),
        "typeof-narrowed primitive source display must be unchanged; got: {msg}"
    );
}
