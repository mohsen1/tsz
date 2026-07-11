//! Relating applications of two structurally DIVERGENT recursive conditional
//! aliases must fail like `tsc`, not diverge or get deferred into silence.
//!
//! Structural rule: when a source is related to a deferred conditional target,
//! the target's true branch is decisive on failure — descending into a
//! recursive false branch after the true branch already failed cannot change
//! the conjunction, but it re-enters the rule with an ever-growing target and
//! burns the relation depth budget (surfacing as a spurious TS2859 or a
//! depth-exceeded assumed-related false negative). `tsc` relates the source to
//! the true branch first and only consults the false branch on success, so
//! `Grow2<[], T>` vs `Grow1<[], T>` bottoms out at the first element mismatch.
//!
//! Companion checker rule: a generic-call argument mismatch whose actual and
//! expected sides reference exactly the same set of enclosing-scope type
//! parameters (no in-flight `infer` holes) is permanent — the contextual
//! deferral policy must not swallow it.
//!
//! Witness: `recursiveConditionalTypes.ts` line 121 `f21(y, x)` (TS2345).

use tsz_checker::test_utils::{
    DiagnosticShape, assert_diagnostic_shapes_exactly, check_source_diagnostics,
};

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// The fixture shape: divergent recursive growth (`[number, ...]` vs
/// `[string, ...]`) applied to the same rigid type parameter, rejected at a
/// recursive call's argument position.
#[test]
fn divergent_recursive_tuple_growth_call_argument_reports_ts2345() {
    let source = r#"
type Grow1<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow1<[number, ...T], N>;
type Grow2<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow2<[string, ...T], N>;

function f21<T extends number>(x: Grow1<[], T>, y: Grow2<[], T>) {
    f21(y, x);
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2345).at(6, 9).with_message_fragment(
            "Argument of type 'Grow2<[], T>' is not assignable to parameter of type 'Grow1<[], T>'.",
        )],
    );
}

/// Renamed binders and a non-recursive (declared callee) form: the rejection
/// is a property of the alias structure, not of the self-call or the names.
#[test]
fn divergent_recursive_growth_renamed_binders_declared_callee_reports_ts2345() {
    let source = r#"
type BuildNum<Acc extends unknown[], Len extends number> = Acc['length'] extends Len ? Acc : BuildNum<[number, ...Acc], Len>;
type BuildStr<Acc extends unknown[], Len extends number> = Acc['length'] extends Len ? Acc : BuildStr<[string, ...Acc], Len>;

declare function take<L extends number>(nums: BuildNum<[], L>, strs: BuildStr<[], L>): void;

function caller<L extends number>(nums: BuildNum<[], L>, strs: BuildStr<[], L>) {
    take(strs, nums);
    take(nums, strs);
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2345).at(8, 10).with_message_fragment(
            "Argument of type 'BuildStr<[], L>' is not assignable to parameter of type 'BuildNum<[], L>'.",
        )],
    );
}

/// Direct assignment form: `tsc` reports a plain TS2322 (with elaboration),
/// not TS2859 "Excessive complexity" — the relation must terminate at the
/// first branch mismatch instead of exhausting its depth budget.
#[test]
fn divergent_recursive_growth_assignment_reports_ts2322_not_ts2859() {
    let source = r#"
type Grow1<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow1<[number, ...T], N>;
type Grow2<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow2<[string, ...T], N>;

function f<T extends number>(x: Grow1<[], T>, y: Grow2<[], T>) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2322).at(6, 5).with_message_fragment(
            "Type 'Grow2<[], T>' is not assignable to type 'Grow1<[], T>'.",
        )],
    );
}

/// Negative control: structurally IDENTICAL recursive conditional aliases
/// stay mutually assignable in both assignment and call positions
/// (`recursiveConditionalTypes.ts` function `f20`).
#[test]
fn structurally_identical_recursive_conditionals_stay_related() {
    let source = r#"
type Unpack1<T> = T extends (infer U)[] ? Unpack1<U> : T;
type Unpack2<T> = T extends (infer U)[] ? Unpack2<U> : T;

function f20<T, U extends T>(x: Unpack1<T>, y: Unpack2<T>) {
    x = y;
    y = x;
    f20(y, x);
}
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "identical recursive conditional aliases must stay related. Codes: {:?}",
        codes(&diags)
    );
}

/// Concrete-length control: with a concrete tuple length the aliases evaluate
/// fully and the mismatch is reported at the evaluated tuple, exactly one
/// error for the mismatched direction.
#[test]
fn divergent_recursive_growth_concrete_length_reports_evaluated_tuple() {
    let source = r#"
type Grow1<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow1<[number, ...T], N>;
type Grow2<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow2<[string, ...T], N>;

declare function g(x: Grow1<[], 3>): void;
declare let y: Grow2<[], 3>;
declare let ok: Grow1<[], 3>;
g(y);
g(ok);
"#;
    let diags = check_source_diagnostics(source);
    assert_diagnostic_shapes_exactly(
        source,
        &diags,
        &[DiagnosticShape::code(2345).at(8, 3).with_message_fragment(
            "Argument of type '[string, string, string]' is not assignable to parameter of type '[number, number, number]'.",
        )],
    );
}

/// Alias-wrapper adjacency: routing one side through a transparent wrapper
/// alias keeps the divergence visible and the rejection intact.
#[test]
fn divergent_recursive_growth_through_wrapper_alias_reports_ts2345() {
    let source = r#"
type Grow1<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow1<[number, ...T], N>;
type Grow2<T extends unknown[], N extends number> = T['length'] extends N ? T : Grow2<[string, ...T], N>;
type WrapTwo<N extends number> = Grow2<[], N>;

declare function need1<L extends number>(x: Grow1<[], L>): void;

function caller<L extends number>(w: WrapTwo<L>) {
    need1(w);
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    assert!(
        codes.contains(&2345),
        "wrapper-alias divergence must still report TS2345. Codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2859),
        "relation must terminate without an excessive-complexity overflow. Codes: {codes:?}"
    );
}

/// Negative control for the alias-shape gate on the checker's rigid-parameter
/// rule: a generic call whose argument is a UNION over the enclosing type
/// parameter and whose parameter is a bare parameter must keep DEFERRING, not
/// report. `tsc` narrows the destructured discriminant so the union never
/// reaches the call; matching it requires the rule to fire only for two
/// deferred conditional/application aliases, never for `T[] | T` vs `T`. This
/// is the `dependentDestructuredVariables.ts` regression that ejected the PR.
#[test]
fn union_argument_vs_bare_parameter_defers_no_spurious_ts2345() {
    let source = r#"
interface A<T> { variant: 'a', value: T }
interface B<T> { variant: 'b', value: Array<T> }
type AB<T> = A<T> | B<T>;
declare function printValue<T>(t: T): void;
declare function printValueList<T>(t: Array<T>): void;
function unrefined1<T>(ab: AB<T>): void {
    const { variant, value } = ab;
    if (variant === 'a') {
        printValue<T>(value);
    } else {
        printValueList<T>(value);
    }
}
"#;
    let diags = check_source_diagnostics(source);
    let codes = codes(&diags);
    assert!(
        !codes.contains(&2345),
        "union-over-parameter argument vs bare parameter must defer, not report. Codes: {codes:?}"
    );
}
