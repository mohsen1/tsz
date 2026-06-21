//! Indexing a callable / hybrid (call-signature + properties) interface member
//! must preserve the member's full apparent type, including its call
//! signatures, on the constraint-satisfaction path.
//!
//! Structural rule:
//!   When a type argument is `(<reducible object>)['method']` and the indexed
//!   access reduces (through the solver) to a concrete callable/function type,
//!   it satisfies a callable constraint such as `T extends (...args: any) =>
//!   any`. tsz previously kept an eager TS2344 because the checker's
//!   `indexed_access_resolves_to_callable` gate only recognised callability
//!   through a bare-type-parameter mapped/index-signature constraint chain, so
//!   `(Cond extends infer T ? T : never)['method']` over a hybrid interface was
//!   wrongly judged non-callable and reported a false TS2344.
//!
//! Issue #14164 (zustand witness). `Parameters` is inlined so the test does not
//! depend on the standard library being loaded by the test harness.
//!
//! Binder names are varied across cases to prevent fixture-name fast paths, and
//! a negative control keeps a genuinely non-callable indexed member reporting
//! TS2344 so the fix is not over-broad.

use tsz_checker::test_utils::check_source_diagnostics;

const PARAMS_DECL: &str = "type MyParameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;\n";

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Repro A (zustand): a hybrid interface reached through an inline conditional
/// with `infer`, then indexed by a callable member, satisfies the callable
/// constraint of `MyParameters`. tsc: clean. tsz (pre-fix): false TS2344.
#[test]
fn hybrid_interface_indexed_through_inline_conditional_satisfies_callable_constraint() {
    let source = format!(
        "{PARAMS_DECL}
interface ReduxDevtoolsExtension {{
  (config?: {{ type?: string }}): unknown
  connect: (preConfig: {{ type?: string }}) => {{ send: (a: unknown) => void }}
}}
interface Win {{ __REDUX_DEVTOOLS_EXTENSION__?: ReduxDevtoolsExtension }}
type Config = MyParameters<
  (Win extends {{ __REDUX_DEVTOOLS_EXTENSION__?: infer T }} ? T : {{ connect: (param: any) => unknown }})['connect']
>[0]
const c: Config = {{ type: 'x' }}
export {{ c }}
"
    );
    let diags = codes(&source);
    assert!(
        !diags.contains(&2344),
        "hybrid-interface indexed member should satisfy the callable constraint (no TS2344); got {diags:?}"
    );
}

/// Same shape, varied binder names (renamed interfaces, member, and the
/// `infer` variable) so the fix cannot key off identifier spellings.
#[test]
fn hybrid_interface_indexed_through_inline_conditional_renamed_binders() {
    let source = format!(
        "{PARAMS_DECL}
interface Bridge {{
  (cfg?: {{ kind?: string }}): unknown
  attach: (pre: {{ kind?: string }}) => {{ emit: (x: unknown) => void }}
}}
interface Host {{ bridge?: Bridge }}
type Cfg = MyParameters<
  (Host extends {{ bridge?: infer U }} ? U : {{ attach: (p: any) => unknown }})['attach']
>[0]
const v: Cfg = {{ kind: 'y' }}
export {{ v }}
"
    );
    let diags = codes(&source);
    assert!(
        !diags.contains(&2344),
        "renamed hybrid-interface indexed member should satisfy the callable constraint; got {diags:?}"
    );
}

/// Adjacent case: the hybrid interface reached directly (no conditional) was
/// already clean; pin it so the constraint path stays green.
#[test]
fn hybrid_interface_indexed_directly_satisfies_callable_constraint() {
    let source = format!(
        "{PARAMS_DECL}
interface Ext {{
  (config?: {{ type?: string }}): unknown
  connect: (preConfig: {{ type?: string }}) => {{ send: (a: unknown) => void }}
}}
type Config = MyParameters<Ext['connect']>[0]
const c: Config = {{ type: 'x' }}
export {{ c }}
"
    );
    let diags = codes(&source);
    assert!(
        !diags.contains(&2344),
        "directly-indexed hybrid member should satisfy the callable constraint; got {diags:?}"
    );
}

/// Negative control 1: indexing a genuinely NON-callable member and using it
/// where a callable constraint is required must still report TS2344, so the fix
/// is not over-broad. `value` is `string`, not a function.
#[test]
fn non_callable_indexed_member_still_reports_ts2344() {
    let source = format!(
        "{PARAMS_DECL}
interface Holder {{
  (config?: {{ type?: string }}): unknown
  value: string
}}
type Bad = MyParameters<
  (Holder extends infer T ? T : never)['value']
>;
export type {{ Bad }};
"
    );
    let diags = codes(&source);
    assert!(
        diags.contains(&2344),
        "indexing a non-callable member used as a callable constraint must still report TS2344; got {diags:?}"
    );
}

/// Negative control 2: a genuinely non-callable type used as callable still
/// errors (TS2349). Guards against the fix loosening real callability checks.
#[test]
fn plain_non_callable_value_used_as_callable_still_reports_ts2349() {
    let source = "
interface Box { value: string }
declare const b: Box;
b.value();
";
    let diags = codes(source);
    assert!(
        diags.contains(&2349),
        "calling a non-callable property must still report TS2349; got {diags:?}"
    );
}
