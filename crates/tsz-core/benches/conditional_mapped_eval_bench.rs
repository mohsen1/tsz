//! Constant-factor microbench for the per-node work in conditional/mapped type
//! evaluation (#13250). Isolates the `contains_free_type_parameters` re-walk
//! that `resolve_operands` runs (twice) on every conditional node, plus the
//! end-to-end `evaluate_type` path over a deep generic conditional/mapped nest.
//!
//! The freeness predicate answer is a pure function of `TypeId` within one
//! interner, so a project-wide deep cache (like the sibling
//! `contains_param_or_infer` cache) should collapse the repeated re-walk to
//! O(1) per shared subtree. These benches A/B that hypothesis: build BOTH the
//! PR head and the pre-PR parent and confirm the right-sign win.

use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use tsz_solver::computation::evaluate_type;
use tsz_solver::construction::TypeInterner;
use tsz_solver::query::contains_free_type_parameters;
use tsz_solver::{ConditionalType, MappedType, TypeData, TypeId, TypeParamInfo};

/// Build a deep, free-type-parameter-bearing object/union/tuple/conditional
/// nest of the given depth. Each level wraps the previous one in another shape
/// that still references the free parameter `T`, so a freeness walk must
/// descend the whole chain (no early leaf short-circuit).
fn build_deep_generic(interner: &TypeInterner, t: TypeId, depth: usize) -> TypeId {
    let mut node = t;
    for i in 0..depth {
        // Alternate the wrapper shape so the walk traverses several
        // `TypeData` variants (array, union, tuple, conditional, mapped, keyof,
        // index-access) rather than a single repeated kind.
        node = match i % 6 {
            0 => interner.array(node),
            1 => interner.union(vec![node, TypeId::STRING]),
            2 => {
                // `node extends string ? node : T` — a generic conditional whose
                // extends side stays concrete but check/branches stay generic.
                interner.conditional(ConditionalType {
                    check_type: node,
                    extends_type: TypeId::STRING,
                    true_type: node,
                    false_type: t,
                    is_distributive: false,
                })
            }
            3 => interner.keyof(node),
            4 => interner.index_access(node, TypeId::STRING),
            _ => {
                // A mapped type `{ [K in keyof node]: node }` keeps the free
                // parameter on both the constraint and template sides.
                let k = interner.intern_string("__k_bench");
                let key_param = TypeParamInfo::simple(k);
                let constraint = interner.keyof(node);
                interner.mapped(MappedType {
                    type_param: key_param,
                    constraint,
                    name_type: None,
                    template: node,
                    readonly_modifier: None,
                    optional_modifier: None,
                })
            }
        };
    }
    node
}

/// A top-level *generic* conditional whose extends side is the deep generic
/// nest. `evaluate_conditional` -> `resolve_operands` runs
/// `contains_free_type_parameters` twice over this extends side per node, and
/// the conditional stays deferred (check type is a naked param), so the work is
/// the predicate walk itself rather than branch expansion.
fn build_top_conditional(interner: &TypeInterner, depth: usize) -> (TypeId, TypeId) {
    let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let deep = build_deep_generic(interner, t, depth);
    let cond = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: deep,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    (cond, deep)
}

/// Isolated predicate cost under the REALISTIC re-query pattern.
///
/// `resolve_operands` runs `contains_free_type_parameters` over the same
/// extends-side shapes many times across distinct conditional re-evaluations
/// and instantiations within ONE project run (one interner). The hot cost is
/// therefore the *repeated* query, not the first walk. We build the deep type
/// once (untimed) on a shared interner and then issue many queries per
/// iteration. With the project-wide deep cache the repeats collapse to O(1);
/// without it every call rebuilds a fresh `DeepContainsChecker` and re-walks
/// the whole subtree (the constant-factor this fix removes).
///
/// Queries hit a spread of subtree roots (every other prefix node) so the memo
/// is exercised across overlapping shapes, matching how nested conditionals ask
/// about progressively smaller extends sides.
const REQUERIES: usize = 256;

fn bench_contains_free_predicate(c: &mut Criterion) {
    let mut group = c.benchmark_group("contains_free_requery");
    for depth in [16usize, 64, 256] {
        // Build once on a shared interner; collect a spread of nested node ids
        // (every other prefix of the deep generic chain) so repeated queries hit
        // overlapping subtrees of varying size.
        let interner = TypeInterner::new();
        let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
        let mut roots = Vec::new();
        let mut node = t;
        for i in 0..depth {
            node = match i % 4 {
                0 => interner.array(node),
                1 => interner.union(vec![node, TypeId::STRING]),
                2 => interner.keyof(node),
                _ => interner.index_access(node, TypeId::STRING),
            };
            if i % 2 == 0 {
                roots.push(node);
            }
        }
        group.bench_function(format!("deep_d{depth}"), |b| {
            b.iter(|| {
                let mut acc = false;
                for _ in 0..REQUERIES {
                    for &r in &roots {
                        acc ^= contains_free_type_parameters(&interner, r);
                    }
                }
                black_box(acc)
            })
        });
    }
    group.finish();
}

/// End-to-end deferred-conditional evaluation over the deep generic nest. This
/// drives the real `evaluate_conditional` -> `resolve_operands` per-node path.
fn bench_evaluate_deep_conditional(c: &mut Criterion) {
    let mut group = c.benchmark_group("evaluate_deep_conditional");
    for depth in [16usize, 64, 256] {
        group.bench_function(format!("deep_d{depth}"), |b| {
            b.iter_batched(
                || {
                    let interner = TypeInterner::new();
                    let (cond, _deep) = build_top_conditional(&interner, depth);
                    (interner, cond)
                },
                |(interner, cond)| black_box(evaluate_type(&interner, cond)),
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

/// Sanity: the top conditional stays deferred (a `Conditional` again), proving
/// the bench exercises the deferral path rather than collapsing to a leaf.
fn assert_deferred() {
    let interner = TypeInterner::new();
    let (cond, _deep) = build_top_conditional(&interner, 16);
    let result = evaluate_type(&interner, cond);
    assert!(
        matches!(interner.lookup(result), Some(TypeData::Conditional(_))),
        "expected deferred conditional, got {:?}",
        interner.lookup(result)
    );
}

fn bench_all(c: &mut Criterion) {
    assert_deferred();
    bench_contains_free_predicate(c);
    bench_evaluate_deep_conditional(c);
}

criterion_group!(conditional_mapped_eval_benches, bench_all);
criterion_main!(conditional_mapped_eval_benches);
