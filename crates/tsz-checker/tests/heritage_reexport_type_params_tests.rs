//! Regression tests: heritage clauses (`extends` / `implements`) over a
//! generic type that reaches the file through a barrel re-export must forward
//! the re-exported declaration's type parameters.
//!
//! Structural rule: the heritage-clause arity checks used the raw-symbol
//! counters (`count_required_type_params` / `get_type_params_for_symbol`),
//! which read parameters off the symbol they are handed. A
//! `export { X } from`/`export type { X } from` barrel creates an intermediate
//! alias that carries no type parameters of its own, so
//! `interface D extends Base<number>` (where `Base` is imported from the
//! barrel) resolved to that alias and falsely reported TS2315
//! ("Type 'Base' is not generic"). A direct single-hop import already resolved
//! correctly. The fix routes the heritage arity/constraint checks through an
//! alias-aware re-export path so the declaration symbol supplies the arity and
//! display parameters, while direct imports and non-generic bases keep their
//! existing behavior.
//!
//! Witnessed in the kysely / valibot project rows (#10663 / #13212), where
//! barrel-re-exported generic bases (`extends BaseTransformation<…>` etc.)
//! produced false TS2315/TS2314 families.

use tsz_checker::test_utils::check_multi_file;
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// Core witness: `interface D extends Base<number>` where the generic
/// interface `Base<T>` is re-exported through a `export type { Base }` barrel.
#[test]
fn interface_extends_type_only_reexported_generic_interface() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Base<T> { value: T }\n"),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "use.ts",
                r#"
import { Base } from './barrel';
export interface Derived extends Base<number> { extra: string }
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "barrel-re-exported generic interface heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// `export { X } from` (value re-export) must behave identically — the trigger
/// is the barrel hop, not the type-only modifier. Renamed binders.
#[test]
fn interface_extends_value_reexported_generic_interface() {
    let diags = check_multi_file(
        &[
            ("shapes.ts", "export interface Shape<E> { item: E }\n"),
            ("hub.ts", "export { Shape } from './shapes';\n"),
            (
                "consumer.ts",
                r#"
import { Shape } from './hub';
export interface Box extends Shape<string> { label: string }
"#,
            ),
        ],
        "consumer.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "value-re-exported generic interface heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// `import type { Base }` at the use site is also fine.
#[test]
fn interface_extends_reexported_generic_via_import_type() {
    let diags = check_multi_file(
        &[
            ("origin.ts", "export interface Node1<P> { payload: P }\n"),
            ("index.ts", "export type { Node1 } from './origin';\n"),
            (
                "leaf.ts",
                r#"
import type { Node1 } from './index';
export interface Leaf1 extends Node1<boolean> { tag: number }
"#,
            ),
        ],
        "leaf.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "import-type re-exported generic interface heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// A re-exported generic type *alias* used in heritage (the alias body is an
/// object type, so it is a valid `extends` target).
#[test]
fn interface_extends_reexported_generic_type_alias() {
    let diags = check_multi_file(
        &[
            ("aliases.ts", "export type Wrap<T> = { value: T };\n"),
            ("barrel.ts", "export type { Wrap } from './aliases';\n"),
            (
                "main.ts",
                r#"
import { Wrap } from './barrel';
export interface Holder extends Wrap<number> { extra: string }
"#,
            ),
        ],
        "main.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "re-exported generic alias heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// `class C extends Base<number>` where the generic base *class* reaches the
/// file through a barrel.
#[test]
fn class_extends_reexported_generic_class() {
    let diags = check_multi_file(
        &[
            ("core.ts", "export class Container<T> { contents!: T }\n"),
            ("pkg.ts", "export { Container } from './core';\n"),
            (
                "app.ts",
                r#"
import { Container } from './pkg';
export class Crate extends Container<number> { size = 2 }
"#,
            ),
        ],
        "app.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "barrel-re-exported generic base class heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// Two-hop barrel chain: `origin -> mid -> top`, imported at the use site.
#[test]
fn interface_extends_multi_hop_reexported_generic() {
    let diags = check_multi_file(
        &[
            ("origin.ts", "export interface Base2<T> { value: T }\n"),
            ("mid.ts", "export type { Base2 } from './origin';\n"),
            ("top.ts", "export type { Base2 } from './mid';\n"),
            (
                "use.ts",
                r#"
import { Base2 } from './top';
export interface Derived2 extends Base2<number> { extra: string }
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags).iter().any(|c| *c == 2315 || *c == 2314),
        "multi-hop re-exported generic interface heritage must not report TS2315/TS2314; got {diags:#?}"
    );
}

/// Constraint satisfaction must validate against the *declared* constraint of
/// the re-exported generic: a satisfying argument is clean.
#[test]
fn reexported_generic_heritage_constraint_satisfied_is_clean() {
    let diags = check_multi_file(
        &[
            (
                "base.ts",
                "export interface Base<T extends string> { value: T }\n",
            ),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "ok.ts",
                r#"
import { Base } from './barrel';
export interface D extends Base<'a'> { extra: number }
"#,
            ),
        ],
        "ok.ts",
        strict_opts(),
    );
    assert!(
        !codes(&diags)
            .iter()
            .any(|c| *c == 2315 || *c == 2314 || *c == 2344),
        "satisfying constraint through barrel must be clean; got {diags:#?}"
    );
}

// --- Negative cases: the fix must NOT loosen real diagnostics -------------

/// A re-exported *non-generic* type applied with type arguments must still
/// report TS2315.
#[test]
fn reexported_non_generic_heritage_still_ts2315() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Plain { x: number }\n"),
            ("barrel.ts", "export type { Plain } from './base';\n"),
            (
                "bad.ts",
                r#"
import { Plain } from './barrel';
export interface D extends Plain<number> { z: string }
"#,
            ),
        ],
        "bad.ts",
        strict_opts(),
    );
    assert!(
        codes(&diags).iter().any(|c| *c == 2315),
        "non-generic re-exported type applied with type args must still report TS2315; got {diags:#?}"
    );
}

/// A re-exported generic type given too few type arguments must still report
/// TS2314 — and the display name must resolve through the chain (`Two<A, B>`).
#[test]
fn reexported_generic_heritage_too_few_args_still_ts2314() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Two<A, B> { a: A; b: B }\n"),
            ("barrel.ts", "export type { Two } from './base';\n"),
            (
                "bad.ts",
                r#"
import { Two } from './barrel';
export interface D extends Two<number> { z: string }
"#,
            ),
        ],
        "bad.ts",
        strict_opts(),
    );
    let ts2314: Vec<_> = diags.iter().filter(|d| d.code == 2314).collect();
    assert!(
        !ts2314.is_empty(),
        "too few type args on re-exported generic must still report TS2314; got {diags:#?}"
    );
    assert!(
        ts2314.iter().any(|d| d.message_text.contains("Two<A, B>")),
        "TS2314 display name must resolve through the re-export chain; got {ts2314:#?}"
    );
}

/// A constraint *violation* through the barrel must report TS2344 (the check
/// is now reached because arity resolves).
#[test]
fn reexported_generic_heritage_constraint_violated_reports_ts2344() {
    let diags = check_multi_file(
        &[
            (
                "base.ts",
                "export interface Base<T extends string> { value: T }\n",
            ),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "bad.ts",
                r#"
import { Base } from './barrel';
export interface D extends Base<number> { extra: number }
"#,
            ),
        ],
        "bad.ts",
        strict_opts(),
    );
    assert!(
        codes(&diags).iter().any(|c| *c == 2344),
        "constraint violation through barrel must report TS2344; got {diags:#?}"
    );
}

/// Local (non-imported) heritage is unchanged: local generic heritage stays
/// clean, local non-generic heritage with args still reports TS2315.
#[test]
fn local_heritage_unchanged() {
    let clean = check_multi_file(
        &[(
            "a.ts",
            r#"
interface LocalBase<T> { v: T }
export interface D extends LocalBase<number> { z: string }
"#,
        )],
        "a.ts",
        strict_opts(),
    );
    assert!(
        !codes(&clean).iter().any(|c| *c == 2315 || *c == 2314),
        "local generic heritage must stay clean; got {clean:#?}"
    );

    let errs = check_multi_file(
        &[(
            "b.ts",
            r#"
interface LocalPlain { v: number }
export interface D extends LocalPlain<number> { z: string }
"#,
        )],
        "b.ts",
        strict_opts(),
    );
    assert!(
        codes(&errs).iter().any(|c| *c == 2315),
        "local non-generic heritage with args must still report TS2315; got {errs:#?}"
    );
}
