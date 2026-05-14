//! Regression tests for #6533: recursive conditional types whose true branch
//! is a mapped type lose infer bindings, collapsing the outer level of the
//! result.
//!
//! Structural rule: after a conditional `T extends Pattern ? Body : ...`
//! succeeds with infer bindings `{ X -> ... }`, every `infer X` reachable from
//! `Body` must be replaced — including the `constraint`, `name_type`, and
//! `template` of any mapped type inside `Body`, plus the binder's
//! `constraint`/`default`. `InferSubstitutor` was missing the `Mapped` arm,
//! so DeepPick-style recursion produced a deferred mapped type with free
//! infer variables instead of the substituted nested object.

use tsz_checker::diagnostics::Diagnostic;

fn check_source(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

fn assert_no_ts2322(source: &str, label: &str) {
    let diagnostics = check_source(source);
    let errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        errors
    );
}

fn assert_has_assignment_error(source: &str, label: &str) {
    let diagnostics = check_source(source);
    // The error may surface as TS2322 (assignability) or TS2353 (excess
    // property on a literal whose contextual type lacks the property).
    // Either is acceptable proof that the inner-shape literal is rejected
    // by the substituted outer type.
    let errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2353)
        .collect();
    assert!(
        !errors.is_empty(),
        "[{label}] expected a TS2322 or TS2353, got none. All diagnostics: {:#?}",
        diagnostics
    );
}

/// The exact reproduction from #6533: DeepPick with a dot-separated path
/// drives a recursive conditional whose true branch is `{ [K in Key]: ... }`.
/// `Key` is captured by the outer infer; the mapped type's constraint must
/// see the substituted literal type, not the original infer node.
#[test]
fn issue_6533_deeppick_two_level_path_evaluates_nested_object() {
    let source = r#"
interface User5 {
  id: number;
  profile: {
    avatar: string;
    bio: string;
  };
}

type DeepPick<T, Path extends string> = Path extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key]: DeepPick<T[Key], Rest> }
    : never
  : Path extends keyof T
    ? { [K in Path]: T[K] }
    : never;

const dp: DeepPick<User5, "profile.avatar"> = { profile: { avatar: "url" } };
"#;
    assert_no_ts2322(source, "issue 6533 repro");
}

/// Renamed infer/mapped binder names — the fix must be structural, not
/// name-matched. If the substitutor only kicked in for specific identifier
/// spellings, swapping `Key`/`Rest`/`K` for `A`/`B`/`X` would still fail.
#[test]
fn deeppick_renamed_binders_evaluates_nested_object() {
    let source = r#"
interface User5 {
  profile: { bio: string };
}

type DP<T, P extends string> = P extends `${infer A}.${infer B}`
  ? A extends keyof T
    ? { [X in A]: DP<T[A], B> }
    : never
  : P extends keyof T
    ? { [X in P]: T[P] }
    : never;

const dp: DP<User5, "profile.bio"> = { profile: { bio: "hello" } };
"#;
    assert_no_ts2322(source, "renamed binders");
}

/// Three-level path — drives the recursive conditional one extra level.
/// Each recursion step substitutes through a mapped type whose constraint
/// is a captured `Infer` from the immediately surrounding conditional.
#[test]
fn deeppick_three_level_path_evaluates_nested_object() {
    let source = r#"
interface ThreeDeep {
  a: { b: { c: number } };
}

type DeepPick<T, Path extends string> = Path extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key]: DeepPick<T[Key], Rest> }
    : never
  : Path extends keyof T
    ? { [K in Path]: T[K] }
    : never;

const dp: DeepPick<ThreeDeep, "a.b.c"> = { a: { b: { c: 7 } } };
"#;
    assert_no_ts2322(source, "three-level path");
}

/// Single-level path (no recursion needed) still works through the same
/// substitution machinery. Regression guard so the Mapped arm doesn't
/// break the simple case.
#[test]
fn deeppick_single_level_path_evaluates_object() {
    let source = r#"
interface User5 {
  id: number;
}

type DeepPick<T, Path extends string> = Path extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key]: DeepPick<T[Key], Rest> }
    : never
  : Path extends keyof T
    ? { [K in Path]: T[K] }
    : never;

const dp: DeepPick<User5, "id"> = { id: 1 };
"#;
    assert_no_ts2322(source, "single-level path");
}

/// Negative case: assigning the inner shape (missing the outer wrapper)
/// must still fail. This proves the substitution actually produces the
/// outer level — if the Mapped arm dropped properties or returned an
/// over-wide type, this assignment would incorrectly succeed.
#[test]
fn deeppick_rejects_inner_shape_without_outer_wrapper() {
    let source = r#"
interface User5 {
  profile: { avatar: string };
}

type DeepPick<T, Path extends string> = Path extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key]: DeepPick<T[Key], Rest> }
    : never
  : Path extends keyof T
    ? { [K in Path]: T[K] }
    : never;

const wrong: DeepPick<User5, "profile.avatar"> = { avatar: "x" };
"#;
    assert_has_assignment_error(source, "missing outer wrapper rejected");
}

/// Non-template recursive conditional that funnels through a mapped type.
/// Verifies the fix is not specific to template-literal infer — any
/// conditional whose bindings flow into a mapped type benefits.
#[test]
fn nested_conditional_with_mapped_in_true_branch_substitutes_keys() {
    let source = r#"
interface Box<T> { value: T }

type UnwrapToObject<T> = T extends Box<infer V>
  ? { [K in "value"]: V }
  : never;

const x: UnwrapToObject<Box<number>> = { value: 42 };
"#;
    assert_no_ts2322(source, "infer-from-Box flows into mapped");
}

/// Mapped key remapping (`as` clause) referencing a captured infer.
/// The `name_type` field of `MappedType` must also be substituted —
/// covered by the same Mapped arm.
#[test]
fn deeppick_with_key_remapping_propagates_infer_into_name_type() {
    let source = r#"
interface User { profile: { avatar: string } };

type Renamed<T, P extends string> = P extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key as `${K & string}_renamed`]: Rest }
    : never
  : never;

const r: Renamed<User, "profile.avatar"> = { profile_renamed: "avatar" };
"#;
    assert_no_ts2322(source, "key remapping uses substituted infer");
}
