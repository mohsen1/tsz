//! Regression tests: a sibling argument's type-predicate target (`x is T`)
//! must count as inference evidence for a shared type parameter `T`, just
//! like an ordinary declared parameter or return-type occurrence does.
//!
//! Structural rule: when a generic call passes a type-predicate-typed
//! argument (`isApplicable: (v: any) => v is I`) alongside a sibling
//! context-sensitive callback whose parameter type mentions `I`, `tsc` infers
//! `I` from the predicate target and fixes it before contextually typing the
//! sibling callback body.
//!
//! `argument_provides_type_param_evidence`
//! (`crates/tsz-checker/src/types/computation/call_inference/unknown_callback.rs`)
//! decides whether a sibling argument counts as evidence by walking its
//! declared parameter type and, for callback-like siblings, its resolved
//! contextual signature's return type, via
//! `type_contains_type_parameter_binder`. That walk is backed by
//! `ChildPolicy::CONTENT_PREDICATE`
//! (`crates/tsz-solver/src/visitors/child_policy.rs`), which deliberately
//! does not descend into a signature's type-predicate target
//! (`signature_type_predicate: false` — the policy's own doc calls this a
//! preserved historical exclusion, "not known to be semantic"). A type
//! parameter whose only appearance in the sibling's signature is inside a
//! type predicate was therefore invisible to the evidence walk, so the
//! recheck concluded "no evidence", defaulted the parameter to `unknown`
//! (no constraint), and re-checked the callback body against it — a
//! spurious `TS18046`/`TS2698` inside a callback body that legitimately
//! depends on the predicate-inferred type.
//!
//! Every expectation below is pinned against real `tsc` 7.0.2
//! (`--noEmit --strict`), reduced from the superjson canary row's
//! `classRule`/`compositeTransformation` shape (issue #15731).

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn assert_no_ts18046(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 18046),
        "Did not expect TS18046 in {context}. Got: {diagnostics:?}"
    );
}

fn assert_no_ts2698(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2698),
        "Did not expect TS2698 in {context}. Got: {diagnostics:?}"
    );
}

#[test]
fn named_predicate_function_seeds_type_param_for_sibling_callback() {
    // Minimal repro: `isApplicable` is a named function reference (not an
    // inline arrow) whose predicate pins `I = any`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function isInstanceOfRegisteredClass(
  potentialClass: any
): potentialClass is any {
  return !!potentialClass?.constructor;
}

function compositeTransformation<I, O>(
  isApplicable: (v: any) => v is I,
  transform: (v: I) => O
) {
  return { isApplicable, transform };
}

const rule = compositeTransformation(isInstanceOfRegisteredClass, (clazz) => {
  const ctor = clazz.constructor;
  return { ...clazz };
});
export { rule };
"#,
    );
    assert_no_ts18046(&diagnostics, "named predicate function sibling evidence");
    assert_no_ts2698(&diagnostics, "named predicate function sibling evidence");
}

#[test]
fn inline_predicate_arrow_seeds_type_param_for_sibling_callback() {
    // Same shape with an inline arrow instead of a named function reference,
    // proving the evidence path (not just the non-callback-argument
    // shortcut) sees the predicate target.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function compositeTransformation<I, O>(
  isApplicable: (v: any) => v is I,
  transform: (v: I) => O
) {
  return { isApplicable, transform };
}

const rule = compositeTransformation(
  (potentialClass): potentialClass is any => !!potentialClass?.constructor,
  (clazz) => {
    const ctor = clazz.constructor;
    return { ...clazz };
  }
);
export { rule };
"#,
    );
    assert_no_ts18046(&diagnostics, "inline predicate arrow sibling evidence");
    assert_no_ts2698(&diagnostics, "inline predicate arrow sibling evidence");
}

#[test]
fn concrete_predicate_target_also_seeds_type_param() {
    // The predicate target need not be `any` — any concrete type counts,
    // just like an ordinary declared-type occurrence would.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function compositeTransformation<I, O>(
  isApplicable: (v: any) => v is I,
  transform: (v: I) => O
) {
  return { isApplicable, transform };
}

const rule = compositeTransformation(
  (potentialClass): potentialClass is string =>
    typeof potentialClass === 'string',
  (clazz) => {
    const len = clazz.length;
    return len;
  }
);
export { rule };
"#,
    );
    assert_no_ts18046(&diagnostics, "concrete predicate target sibling evidence");
}

#[test]
fn renamed_binder_predicate_evidence_is_structural() {
    // Same shape with `I`/`isApplicable`/`transform` renamed — proves the
    // rule is structural, not keyed on a specific identifier.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function makeRule<Target, Out>(
  guard: (raw: any) => raw is Target,
  encode: (value: Target) => Out
) {
  return { guard, encode };
}

const rule = makeRule(
  (raw): raw is any => !!raw,
  (value) => {
    const probe = value.field;
    return probe;
  }
);
export { rule };
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed type parameter `Target`");
}

#[test]
fn non_predicate_sibling_without_evidence_still_reports_unknown() {
    // Negative control: without a type predicate (or any other evidence
    // source) for `I`, `tsc` genuinely leaves the callback's parameter
    // `unknown`. The predicate fix must not suppress this real diagnostic.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function compositeTransformation<I, O>(
  isApplicable: (v: any) => boolean,
  transform: (v: I) => O
) {
  return { isApplicable, transform };
}

const rule = compositeTransformation(
  (potentialClass) => !!potentialClass,
  (clazz) => {
    const ctor = clazz.constructor;
    return { ...clazz };
  }
);
export { rule };
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 18046),
        "Expected TS18046 with no predicate-based evidence for `I`. Got: {diagnostics:?}"
    );
}
