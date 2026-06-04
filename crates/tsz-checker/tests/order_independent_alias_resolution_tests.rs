//! Order-independence regression tests for cross-file alias / re-export
//! resolution.
//!
//! Root cause (refs <https://github.com/tsz-org/tsz/issues/7574>, PR #12148):
//! `resolve_alias_symbol_inner` used to pin a resolved alias to its target's
//! owning file by reading the *dynamic*, monotonically-growing `symbol -> file`
//! overlay. That made the pinned file — and therefore which symbol an alias
//! ultimately resolves to — depend on *when* the alias was first resolved
//! relative to other files. A colliding same-name declaration in another file
//! could win or lose the pin purely on processing order, producing a spurious
//! `TS2315 Type 'X' is not generic` (or its absence) for the *same* program.
//!
//! Structural rule under test:
//!
//! > When a cross-file alias resolves to an exported declaration, tsc pins the
//! > alias to that declaration's *declaring* file (a static fact); this change
//! > makes tsz pin against the stable `global_symbol_file_index` too, so the
//! > resolution endpoint is identical regardless of file processing order.
//!
//! Each test checks the same logical program under every file permutation and
//! asserts byte-identical diagnostics. The fix is name-agnostic: the generic
//! type parameter names are varied (`N`/`E`, `K`/`V`) to prove the rule is not
//! keyed on any spelling.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::ESNext,
        no_lib: true,
        ..Default::default()
    }
}

/// Render diagnostics into a stable, order-independent signature: a sorted set
/// of `code@line:col` strings. File processing order must not change this set.
fn diagnostic_signature(diags: &[Diagnostic]) -> Vec<String> {
    let mut sig: Vec<String> = diags
        .iter()
        .map(|d| format!("TS{}@{}+{}", d.code, d.start, d.length))
        .collect();
    sig.sort();
    sig
}

/// Generate every permutation of `items` (small n only).
fn permutations<'a>(items: &[(&'a str, &'a str)]) -> Vec<Vec<(&'a str, &'a str)>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut perm in permutations(&rest) {
            perm.insert(0, head);
            out.push(perm);
        }
    }
    out
}

/// Assert that checking `entry` is order-independent across every permutation
/// of the project files, and return the canonical signature for follow-up
/// assertions.
fn assert_order_independent(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    let perms = permutations(files);
    let baseline_files = &perms[0];
    let baseline = diagnostic_signature(&check_multi_file_with_global_index(
        baseline_files,
        entry,
        opts(),
    ));
    for (perm_idx, perm) in perms.iter().enumerate() {
        let sig = diagnostic_signature(&check_multi_file_with_global_index(perm, entry, opts()));
        assert_eq!(
            sig,
            baseline,
            "diagnostics diverged for file order #{perm_idx} = {:?}\n  baseline = {baseline:?}\n  got      = {sig:?}",
            perm.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        );
    }
    baseline
}

/// The #12148 witness: `@shared/core`-style generic `Graph<N, E>` is re-exported
/// through a barrel and consumed via the barrel as a generic type argument.
/// A *colliding non-generic* `Graph` lives in another module. The generic
/// instantiation must resolve to the generic `Graph` (no TS2315) regardless of
/// which file is processed first.
#[test]
fn generic_reexport_resolves_consistently_across_file_orders() {
    let files = [
        (
            "core.ts",
            "export interface Graph<N, E> { nodes: N[]; edges: E[]; }\n",
        ),
        // A colliding *non-generic* Graph in an unrelated module.
        ("other.ts", "export interface Graph { count: number; }\n"),
        // Barrel re-export of the generic Graph.
        ("barrel.ts", "export * from './core';\n"),
        // Consumer instantiates the generic Graph<...> via the barrel.
        (
            "adapter.ts",
            "import { Graph } from './barrel';\nexport type Dep = Graph<string, number>;\n",
        ),
    ];

    // The core invariant: diagnostics are identical across every file order.
    // (Under `no_lib` the generic interface's `N[]` cannot resolve to Array, so
    // absolute correctness of `Graph<...>` is asserted at the CLI level with
    // libs present; here we pin order-independence, which is the bug this fix
    // targets.)
    assert_order_independent(&files, "adapter.ts");
}

/// Same shape, renamed type parameters (`K`/`V` instead of `N`/`E`) and a named
/// re-export instead of a wildcard barrel, proving the fix is name-agnostic and
/// not keyed to the wildcard-reexport spelling.
#[test]
fn named_reexport_renamed_params_resolves_consistently_across_file_orders() {
    let files = [
        (
            "model.ts",
            "export interface Graph<K, V> { keys: K[]; values: V[]; }\n",
        ),
        ("legacy.ts", "export interface Graph { total: number; }\n"),
        ("index.ts", "export { Graph } from './model';\n"),
        (
            "consumer.ts",
            "import { Graph } from './index';\nexport type Pair = Graph<number, string>;\n",
        ),
    ];

    // Order-independence is the invariant under test (see note above).
    assert_order_independent(&files, "consumer.ts");
}

/// `export =` / `import =` chain through an intermediate module, with a
/// colliding alias name in a sibling module. The endpoint must be stable
/// across orders. This covers the `export =` arm of the resolution walk.
#[test]
fn export_equals_chain_resolves_consistently_across_file_orders() {
    let files = [
        ("lib.ts", "interface Box<T> { value: T; }\nexport = Box;\n"),
        ("rival.ts", "export interface Box { tag: string; }\n"),
        (
            "use.ts",
            "import Box = require('./lib');\nexport type Wrapped = Box<boolean>;\n",
        ),
    ];

    // export = / import = require resolution under CommonJS.
    let cjs = CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::CommonJS,
        no_lib: true,
        ..Default::default()
    };
    let perms = permutations(&files);
    let baseline = diagnostic_signature(&check_multi_file_with_global_index(
        &perms[0],
        "use.ts",
        cjs.clone(),
    ));
    for perm in &perms {
        let sig = diagnostic_signature(&check_multi_file_with_global_index(
            perm,
            "use.ts",
            cjs.clone(),
        ));
        assert_eq!(
            sig,
            baseline,
            "export= chain diverged for order {:?}",
            perm.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        );
    }
}

/// Negative / fallback case: a genuinely non-generic re-exported type used with
/// a type argument must report TS2315 in *every* order (the fix must not
/// silence real errors — it only removes the order-dependence).
#[test]
fn non_generic_reexport_reports_ts2315_in_every_order() {
    let files = [
        ("decl.ts", "export interface Widget { id: number; }\n"),
        ("hub.ts", "export * from './decl';\n"),
        (
            "site.ts",
            "import { Widget } from './hub';\nexport type Bad = Widget<string>;\n",
        ),
    ];

    let sig = assert_order_independent(&files, "site.ts");
    assert!(
        sig.iter().any(|s| s.starts_with("TS2315@")),
        "expected TS2315 (Widget is not generic) in every order; got {sig:?}",
    );
}
