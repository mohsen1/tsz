//! Regression coverage for narrowing that must survive a conditional /
//! short-circuit *expression initializer* placed between a type guard and a
//! later read of the guarded reference.
//!
//! Structural rule (matches `tsc`): inside `if (x.kind === "min") { ... }`, a
//! statement such as `const t = cond ? a : b;` (or `const t = a && b;`) does not
//! widen `x`. The conditional expression's flow merge carries the narrowing
//! established by the guard, so a following `if (...) { x.value }` still reads
//! the narrowed member type.
//!
//! Owner: `tsz_checker` control-flow worklist
//! (`flow/control_flow/core/flow_traversal.rs`). The CONDITION node for the inner
//! `if` reaches the guard through a *non-targeting* `const t` ASSIGNMENT whose
//! antecedent is the conditional-expression merge `BRANCH_LABEL`. Previously the
//! worklist did not defer through that assignment to the merge, so the inner
//! CONDITION read the *declared* (un-narrowed) union and produced a false
//! `TS2339`. The fix defers when the non-targeting assignment's antecedent is a
//! conditional-EXPRESSION merge specifically; statement merges (`if`/`switch`/
//! `try`) are excluded so targeting-assignment chains (e.g. the
//! `controlFlowForCatchAndFinally` try/catch pattern) are not re-ordered.
//!
//! Binder names are varied per case so coverage is structural, not keyed to a
//! particular identifier.

use tsz_checker::test_utils::check_source_strict_codes;

const TS2339: u32 = 2339; // Property does not exist on type

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

// ---------------------------------------------------------------------------
// Positive cases: narrowing survives a conditional-expression initializer.
// `tsc` is clean on all of these.
// ---------------------------------------------------------------------------

#[test]
fn ternary_initializer_then_if_preserves_discriminant_narrowing() {
    // The zod `ZodNumber._parse` witness, minimized: a `const` whose initializer
    // is a ternary, followed by a nested `if` that does not re-narrow.
    let diags = codes(
        r#"
type Check =
  | { kind: "min"; value: number; inclusive: boolean }
  | { kind: "int" };
declare const checks: Check[];
for (const check of checks) {
  if (check.kind === "min") {
    const tooSmall = check.inclusive ? check.value > 0 : check.value >= 0;
    if (tooSmall) {
      const v: number = check.value;
      const b: boolean = check.inclusive;
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "ternary initializer must not widen the narrowed discriminant, got {diags:?}"
    );
}

#[test]
fn logical_and_initializer_then_if_preserves_narrowing_renamed() {
    // Short-circuit `&&` initializer, different binder names.
    let diags = codes(
        r#"
type Rule =
  | { tag: "lo"; bound: number; strict: boolean }
  | { tag: "hi" };
declare const rules: Rule[];
for (const entry of rules) {
  if (entry.tag === "lo") {
    const triggered = entry.strict && entry.bound > 0;
    if (triggered) {
      const n: number = entry.bound;
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "`&&` initializer must not widen the narrowed member, got {diags:?}"
    );
}

#[test]
fn logical_or_and_nullish_initializers_preserve_narrowing() {
    // `||` and `??` short-circuit initializers, both followed by a nested `if`.
    let diags = codes(
        r#"
type Spec =
  | { sort: "asc"; weight: number }
  | { sort: "none" };
declare const specs: Spec[];
for (const s of specs) {
  if (s.sort === "asc") {
    const fallbackOr = s.weight > 0 || false;
    const fallbackNullish = (s.weight > 0 ? 1 : null) ?? 0;
    if (fallbackOr) {
      const a: number = s.weight;
    }
    if (fallbackNullish > 0) {
      const b: number = s.weight;
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "`||`/`??` initializers must not widen the narrowed member, got {diags:?}"
    );
}

#[test]
fn ternary_initializer_then_if_in_generic_method_class() {
    // Generic class form: `this._def.checks` element is a discriminated union;
    // the conditional initializer sits between the guard and the read, exactly as
    // in zod's class methods.
    let diags = codes(
        r#"
interface Def<C> { checks: C[]; }
type NumCheck =
  | { kind: "min"; value: number; inclusive: boolean }
  | { kind: "int" };
abstract class Base<D> { readonly _def!: D; abstract run(d: number): void; }
class NumType extends Base<Def<NumCheck>> {
  run(data: number): void {
    for (const check of this._def.checks) {
      if (check.kind === "min") {
        const small = check.inclusive ? data < check.value : data <= check.value;
        if (small) {
          const v: number = check.value;
          const incl: boolean = check.inclusive;
        }
      }
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "generic-class method must preserve narrowing past a ternary initializer, got {diags:?}"
    );
}

#[test]
fn nested_conditional_initializers_chain_preserves_narrowing() {
    // Multiple conditional initializers in sequence before the read.
    let diags = codes(
        r#"
type TreeCell =
  | { type: "leaf"; payload: number }
  | { type: "branch" };
declare const cells: TreeCell[];
for (const cell of cells) {
  if (cell.type === "leaf") {
    const a = cell.payload > 0 ? 1 : 2;
    const b = a > 0 && cell.payload < 10;
    const c = b || cell.payload === 0;
    if (c) {
      const p: number = cell.payload;
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "chained conditional initializers must preserve narrowing, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback: a statement-level merge (try/catch) between a targeting
// assignment guard must NOT be over-narrowed by the same defer logic. `tsc`
// accepts the inner `.abort()` call (narrowed to the class type, not `never`).
// ---------------------------------------------------------------------------

#[test]
fn try_catch_targeting_assignment_does_not_over_narrow() {
    // controlFlowForCatchAndFinally witness: the conditional-expression-merge
    // defer must not bleed into statement (`try`/`catch`) merges, which would
    // re-order the targeting-assignment chain and narrow to `never`.
    let diags = codes(
        r#"
declare class Aborter { abort(): void }
declare function make(): Aborter;
class Holder {
  controller: Aborter | undefined = undefined;
  run(): void {
    if (this.controller !== undefined) {
      this.controller.abort();
      this.controller = undefined;
    }
    try {
      this.controller = make();
    } catch (error) {
      if (this.controller !== undefined) {
        this.controller.abort();
      }
    }
  }
}
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "try/catch targeting-assignment narrowing must stay accurate (no `never`), got {diags:?}"
    );
}
