//! Robustness guard: the `arguments`-reference body walk must terminate on a
//! cyclic AST instead of overflowing the stack (SIGABRT / rc134).
//!
//! `CheckerState::body_has_arguments_reference` is an uncached recursive descent
//! over a function body used while building a signature (JS `arguments`
//! detection). It only stops at nested function/class boundaries, so it assumes
//! the node graph it walks is a finite tree. During checking of an
//! `async` generic method whose body has a `for..of` destructuring loop with a
//! spread call over a generic indexed-access rest type, a synthesized body node
//! ends up reachable from itself, and the walk recurses on the same node
//! forever — a genuine cycle, not merely deep nesting (the parser bounds real
//! nesting via `MAX_PARSER_RECURSION_DEPTH`). On the config-broken canary apps
//! (immich-server, cal-com, infisical) this overflowed the worker stack and
//! aborted the whole compile; `tsc` handles the same input in well under a
//! second.
//!
//! The fix threads an `FxHashSet<NodeIndex>` visited set through the walk (the
//! same cycle-guard idiom used for node-index walks elsewhere in this crate), so
//! a node is only descended into once. A well-formed tree never revisits a node,
//! so the boolean answer is unchanged; a cyclic graph terminates. This test is
//! the reduced witness: without the guard, `check_source` aborts with a stack
//! overflow; with it, the check returns normally.
//!
//! Binder names here deliberately differ from the originating fixture
//! (`event.repository.ts`) to keep the guard structural — it must not depend on
//! any identifier, alias, or file-name string.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// Reduced repro of the immich-server / cal-com / infisical SIGABRT: an `async`
/// generic method whose body spreads a generic indexed-access rest type through
/// a `for..of` destructuring call. Pre-guard this overflowed the stack inside
/// `body_has_arguments_reference`; the assertion is simply that `check_source`
/// *returns* (the process does not abort).
#[test]
fn async_generic_spread_body_walk_does_not_overflow() {
    let source = r#"
type Registry = { alpha: []; beta: [number] };
type RegistryKey = keyof Registry;
type ParamsFor<P extends RegistryKey> = Registry[P];
class Dispatcher {
  async dispatch<P extends RegistryKey>(
    packet: { name: P; params: ParamsFor<P>; broadcast: boolean },
  ): Promise<void> {
    const listeners: { callback: (...rest: any[]) => void }[] = [];
    for (const { callback } of listeners) {
      await callback(...packet.params);
    }
  }
}
"#;

    let diagnostics = check_source(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    // Reaching this line at all is the guarantee: the recursive body walk
    // terminated on the cyclic AST rather than overflowing the stack. The exact
    // diagnostic set is incidental (this stub source has no lib types), so we
    // only require that checking completed and did not explode into a runaway
    // count.
    assert!(
        diagnostics.len() < 1000,
        "expected checking to complete with a bounded diagnostic set, got {}",
        diagnostics.len()
    );
}
