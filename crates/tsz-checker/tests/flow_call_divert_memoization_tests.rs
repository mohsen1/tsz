//! Regression coverage for the per-walk memoization of the CALL flow-node
//! narrow/divert classification (#13311).
//!
//! Structural rule: when the backward flow walk crosses a CALL flow node it must
//! decide whether that call can narrow or divert the queried reference (a
//! never-returning call or an `asserts` predicate). That decision is a pure
//! function of the call node within a single walk — it reads the immutable
//! per-check type cache and resolved type-predicate tables and never depends on
//! the reference being narrowed. The linear-passthrough chase re-scans
//! overlapping pass-through runs on every worklist pop, and the defer classifier
//! recurses over pass-through call chains, so without a memo a call-dense single
//! scope re-extracts each call's predicate signature thousands of times per
//! reference read (the `check_flow` worklist owns the loop; its cost is
//! `O(distinct reference paths * scope statements)`). Memoizing the
//! classification by flow-node id collapses the redundant extraction to one per
//! call node per walk with no change in narrowing value.
//!
//! These tests pin the value-preservation half of that rule: narrowing across a
//! call-and-guard-dense single function body is unchanged, identifier names do
//! not matter, and an unguarded read still reports the possibly-`undefined`
//! diagnostic (the memo must not over-splice a relevant call into silence). The
//! per-statement cost reduction itself is a constant-factor change validated by
//! the project benchmark, not by an absolute-time assertion here.

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Number of distinct properties cycled by [`call_dense_guarded_body`]. Kept
/// small so each backward walk lands on the previous read of the same property
/// quickly (well under the flow step budget); the call density that exercises
/// the per-walk memo comes from the `count` interleaved member calls, not from a
/// deep per-walk frontier.
const CYCLED_PROPS: usize = 8;

/// Build a single function body that interleaves guarded property reads — each
/// followed by a member call — over one shared object, cycling a small set of
/// properties. Every guard narrows its property to `number`, so the body
/// type-checks clean only if narrowing survives the call-dense backward walk.
/// `obj`/`sink` are parameters so the same shape can be re-bound under different
/// identifiers to prove the per-walk memo keys on flow-node identity, not names.
fn call_dense_guarded_body(count: usize, obj: &str, sink: &str) -> String {
    let props: Vec<String> = (0..CYCLED_PROPS)
        .map(|i| format!("p{i}: number | undefined;"))
        .collect();
    let mut out = String::new();
    out.push_str(&format!("interface Wide {{ {} }}\n", props.join(" ")));
    out.push_str(&format!("declare function {sink}(x: unknown): void;\n"));
    out.push_str("declare function makeWide(): Wide;\n");
    out.push_str("function big() {\n");
    out.push_str(&format!("  const {obj} = makeWide();\n"));
    for i in 0..count {
        let p = i % CYCLED_PROPS;
        out.push_str(&format!(
            "  if ({obj}.p{p} !== undefined) {{ {sink}({obj}.p{p} + {i}); }}\n"
        ));
    }
    out.push_str("}\n");
    out
}

#[test]
fn narrowing_preserved_across_call_dense_single_scope() {
    let source = call_dense_guarded_body(120, "obj", "sink");
    let diagnostics = strict_diagnostics(&source);
    assert!(
        diagnostics.is_empty(),
        "guarded property reads must narrow to `number` across a call-dense scope; got {diagnostics:?}",
    );
}

#[test]
fn call_divert_memo_is_identifier_name_independent() {
    // Same structural shape, different binder names: the per-walk memo keys on
    // flow-node identity, never on source identifiers, so the result must match.
    let a = strict_diagnostics(&call_dense_guarded_body(100, "obj", "sink"));
    let b = strict_diagnostics(&call_dense_guarded_body(100, "container", "consume"));
    assert!(
        a.is_empty(),
        "named variant `obj`/`sink` should be clean; got {a:?}"
    );
    assert!(
        b.is_empty(),
        "renamed variant `container`/`consume` should be clean; got {b:?}"
    );
    assert_eq!(
        a, b,
        "narrowing outcome must not depend on identifier names"
    );
}

#[test]
fn unguarded_read_in_call_dense_scope_still_reports_possibly_undefined() {
    // Negative control: the memo collapses only calls that cannot narrow/divert.
    // An unguarded read of a `number | undefined` property must still surface the
    // possibly-`undefined` diagnostic — the optimization must not silence it.
    let mut source = call_dense_guarded_body(50, "obj", "sink");
    // Drop the function close brace and append one unguarded read before closing.
    assert!(source.ends_with("}\n"));
    source.truncate(source.len() - 2);
    source.push_str("  sink(obj.p0 + 1);\n}\n");
    let diagnostics = strict_diagnostics(&source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == 18048 || *code == 2532),
        "unguarded `obj.p0` read should report possibly-undefined; got {diagnostics:?}",
    );
}

#[test]
fn flow_traversal_threads_call_divert_memo() {
    // Structural guard: the chase and defer classifier share a per-walk memo so
    // the CALL narrow/divert classification is extracted at most once per node.
    let core = include_str!("../src/flow/control_flow/core.rs");
    assert!(
        core.contains("fn call_node_may_narrow_or_divert_cached("),
        "core should expose a memoized CALL narrow/divert classifier",
    );
    assert!(
        core.contains("struct FlowDeferMemos"),
        "core should bundle the per-walk defer / call-divert classification memos",
    );
    let traversal = include_str!("../src/flow/control_flow/core/flow_traversal.rs");
    assert!(
        traversal.contains("FlowDeferMemos::default()"),
        "check_flow worklist should allocate the per-walk classification memos once",
    );
    assert!(
        traversal.contains(
            "call_node_may_narrow_or_divert_cached(current, flow, &mut memos.call_divert)"
        ),
        "the linear-passthrough chase should consult the memoized classifier",
    );
}
