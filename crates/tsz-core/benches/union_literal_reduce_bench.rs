//! Standalone microbench isolating the residual `{Intrinsic, Literal:N}`
//! union-reduction signature from the ts-toolbelt #13250 row.
//!
//! Prior #13667 work retired the four 202-member all-literal/all-deferred
//! unions (range short-circuit + deferred early-return). The residual
//! `union_subtype_reduction_shallow_checks` (~13k on the real row) comes from
//! mixed `{ Intrinsic, Literal:~44 }` unions: a widened primitive plus many
//! distinct literals, where the primitive absorbs the literals but the
//! quadratic loop still pays O(N^2) ordered-pair `is_subtype_shallow` calls,
//! most of which are literal-vs-literal pairs that can never reduce.
//!
//! This bench reproduces that signature, reports min-of-K wall-clock and the
//! deterministic `shallow_checks` / `intern_calls` counters. Run with
//! `TSZ_PERF_COUNTERS=1` set in the environment, e.g.:
//!
//! ```sh
//! TSZ_PERF_COUNTERS=1 cargo bench -p tsz-core --bench union_literal_reduce_bench
//! ```

use std::sync::atomic::Ordering;
use std::time::Instant;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeInterner;

/// A union of one widened primitive of a DIFFERENT domain (`number`) plus `n`
/// distinct string literals. Absorption does not fire (number does not absorb
/// string literals), but `has_primitive` is set, so `reduce_union_subtypes`
/// falls through to the O(N^2) quadratic loop. Every literal-vs-literal pair is
/// wasted work: distinct literals are mutually non-subtypes; only the
/// literal-vs-`number` pairs can ever reduce, and none of those do here. This
/// is the residual `{ Intrinsic, Literal:~44 }` ts-toolbelt signature.
fn cross_domain_primitive_plus_literals(interner: &TypeInterner, n: usize) -> Vec<TypeId> {
    let mut members = Vec::with_capacity(n + 1);
    members.push(TypeId::NUMBER);
    for i in 0..n {
        members.push(interner.literal_string(&format!("member_{i}")));
    }
    members
}

/// A union of one widened primitive (`string`) plus `n` distinct string
/// literals. `string` absorbs every literal during the pre-pass, so the reduced
/// result is just `string` and the quadratic loop never runs (control case).
fn same_domain_primitive_plus_literals(interner: &TypeInterner, n: usize) -> Vec<TypeId> {
    let mut members = Vec::with_capacity(n + 1);
    members.push(TypeId::STRING);
    for i in 0..n {
        members.push(interner.literal_string(&format!("member_{i}")));
    }
    members
}

/// An all-literal union of `n` distinct string literals with no widened peer.
/// No pair can reduce (distinct literals are mutually non-subtypes), so this is
/// pure wasted pairwise work absent a partition (control case: short-circuited
/// today by the `all_non_reducible && !has_primitive` early return).
fn all_distinct_literals(interner: &TypeInterner, n: usize) -> Vec<TypeId> {
    (0..n)
        .map(|i| interner.literal_string(&format!("lit_{i}")))
        .collect()
}

fn shallow_checks() -> u64 {
    tsz_common::perf_counters::counters()
        .union_subtype_reduction_shallow_checks
        .load(Ordering::Relaxed)
}

fn intern_calls() -> u64 {
    tsz_common::perf_counters::counters()
        .interner_intern_calls
        .load(Ordering::Relaxed)
}

fn min_of_k<F: FnMut()>(k: u32, mut f: F) -> std::time::Duration {
    let mut best = std::time::Duration::MAX;
    for _ in 0..k {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

fn run_case(label: &str, build: impl Fn(&TypeInterner) -> Vec<TypeId>) {
    // Warm a throwaway interner so allocation noise is amortized.
    {
        let warm = TypeInterner::new();
        let _ = warm.union(build(&warm));
    }

    let before_checks = shallow_checks();
    let before_intern = intern_calls();

    // Wall-clock: fresh interner each iteration so reduction actually runs
    // (a cached union would short-circuit on the second call).
    let elapsed = min_of_k(50, || {
        let interner = TypeInterner::new();
        let members = build(&interner);
        std::hint::black_box(interner.union(members));
    });

    let after_checks = shallow_checks();
    let after_intern = intern_calls();

    println!(
        "{label:<32} min_wall={:>10.3}us  shallow_checks/run~{:>8.0}  intern/run~{:>8.0}",
        elapsed.as_secs_f64() * 1e6,
        (after_checks - before_checks) as f64 / 50.0,
        (after_intern - before_intern) as f64 / 50.0,
    );
}

fn main() {
    // Counters are gated by the `TSZ_PERF_COUNTERS` env var, read once into an
    // `OnceLock` on the first access below. Set it in the environment before
    // launching (see the module docs); otherwise the counter columns read 0 and
    // only the wall-clock numbers are meaningful.
    if !tsz_common::perf_counters::enabled_fast() {
        println!("note: TSZ_PERF_COUNTERS not set; shallow_checks/intern columns will read 0");
    }
    // Touch the counters to initialize the OnceLock.
    let _ = shallow_checks();

    println!("== cross-domain primitive (number) + N string literals [RESIDUAL] ==");
    for n in [20usize, 44, 100, 200] {
        run_case(&format!("num+strlit N={n}"), |i| {
            cross_domain_primitive_plus_literals(i, n)
        });
    }

    println!("\n== same-domain primitive (string) + N string literals [absorbed] ==");
    for n in [20usize, 44, 100, 200] {
        run_case(&format!("str+strlit N={n}"), |i| {
            same_domain_primitive_plus_literals(i, n)
        });
    }

    println!("\n== N distinct literals, no widened peer [short-circuit] ==");
    for n in [20usize, 44, 100, 200] {
        run_case(&format!("all-lit N={n}"), |i| all_distinct_literals(i, n));
    }
}
