//! `class C implements Generic<Arg>` across a file boundary (#16434).
//!
//! Structural rule: when a class implements a generic interface, `tsc`
//! instantiates the interface with the written type arguments before
//! comparing members. tsz does this through the class-implements checker
//! (`crates/tsz-checker/src/classes/class_implements_checker/core.rs`) — but
//! only reliably when the interface is declared in the same file as the
//! class. When the heritage target is declared in another file, the
//! type-parameter-collecting AST walk indexes the CURRENT file's arena, so it
//! finds nothing for a foreign declaration; `interface_type_params` stays
//! empty, the substitution built from it degenerates to the identity, and
//! every interface member is compared against its own unsubstituted type
//! parameter (`() => T` vs `() => number`) instead of the instantiated type.
//!
//! The fix forces `delegate_cross_arena_interface_type` to resolve the
//! foreign interface's own type before the definition-store fallback reads
//! it, since that resolution registers the interface's type parameters into
//! the store as a side effect. Previously that resolution only happened
//! later (while building `raw_interface_type`), after the fallback had
//! already run and found nothing.
//!
//! The matrix varies: generic vs. non-generic, same-file vs. cross-file,
//! `import` vs. `import type`, renamed binders, multiple type parameters, and
//! a genuine mismatch that must still be reported (proving the fix
//! instantiates correctly rather than suppressing checking).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

fn opts() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(files, entry, opts())
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_clean(diags: &[Diagnostic], label: &str) {
    let cs = codes(diags);
    assert!(
        !cs.contains(&2416) && !cs.contains(&2345) && !cs.contains(&2420),
        "[{label}] expected no TS2416/TS2345/TS2420 for a correctly-implemented cross-file \
         generic interface, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

// ── Positive: the reported bug, and its adjacent shapes ─────────────────────

#[test]
fn cross_file_generic_interface_single_type_param() {
    let types_ts = r#"
        export interface Plain<T> { plain(): T }
    "#;
    let actor_ts = r#"
        import { Plain } from "./types";
        export class Actor implements Plain<number> {
            public plain(): number { return 1 }
        }
        declare function want(r: Plain<number>): void;
        declare const a: Actor;
        want(a);
    "#;
    let diags = check(
        &[("actor.ts", actor_ts), ("types.ts", types_ts)],
        "actor.ts",
    );
    assert_clean(&diags, "single-type-param");
}

#[test]
fn cross_file_generic_interface_import_type() {
    let types_ts = r#"
        export interface Plain<T> { plain(): T }
    "#;
    let actor_ts = r#"
        import type { Plain } from "./types";
        export class Actor implements Plain<number> {
            public plain(): number { return 1 }
        }
    "#;
    let diags = check(
        &[("actor.ts", actor_ts), ("types.ts", types_ts)],
        "actor.ts",
    );
    assert_clean(&diags, "import-type");
}

#[test]
fn cross_file_generic_interface_renamed_binders() {
    // Vary every identifier and file name so the fix is not tied to the
    // literal names in the reported repro (anti-hardcoding: nothing in the
    // resolution path reads a user-chosen name).
    let contract_ts = r#"
        export interface Codec<Payload> { encode(): Payload }
    "#;
    let worker_ts = r#"
        import { Codec } from "./contract";
        export class JsonWorker implements Codec<string> {
            public encode(): string { return "{}" }
        }
        declare function accepts(c: Codec<string>): void;
        declare const w: JsonWorker;
        accepts(w);
    "#;
    let diags = check(
        &[("worker.ts", worker_ts), ("contract.ts", contract_ts)],
        "worker.ts",
    );
    assert_clean(&diags, "renamed-binders");
}

#[test]
fn cross_file_generic_interface_multiple_type_params_one_used() {
    let types_ts = r#"
        export interface Pair<K, V> {
            key(): K;
            value(): V;
        }
    "#;
    let main_ts = r#"
        import { Pair } from "./types";
        export class Entry implements Pair<string, number> {
            public key(): string { return "k" }
            public value(): number { return 1 }
        }
    "#;
    let diags = check(&[("main.ts", main_ts), ("types.ts", types_ts)], "main.ts");
    assert_clean(&diags, "multi-param");
}

#[test]
fn cross_file_generic_interface_two_hop_reexport() {
    // The interface is declared in `dep.ts`, re-exported through `hub.ts`,
    // and implemented from `main.ts` — the heritage target's own declaration
    // is two module hops away from the implementing class.
    let dep_ts = r#"
        export interface Box<T> { get(): T }
    "#;
    let hub_ts = r#"
        export { Box } from "./dep";
    "#;
    let main_ts = r#"
        import { Box } from "./hub";
        export class NumberBox implements Box<number> {
            public get(): number { return 1 }
        }
    "#;
    let diags = check(
        &[("main.ts", main_ts), ("hub.ts", hub_ts), ("dep.ts", dep_ts)],
        "main.ts",
    );
    assert_clean(&diags, "two-hop-reexport");
}

#[test]
fn cross_file_generic_class_heritage_target() {
    // The heritage target itself is a CLASS, not an interface — a class may
    // structurally `implements` another class in TS. The type-parameter
    // AST walk this fix touches collects params from both CLASS_DECLARATION
    // and INTERFACE_DECLARATION heritage nodes, so this must be fixed too.
    let base_ts = r#"
        export class Box<T> {
            value!: T;
            get(): T { return this.value }
        }
    "#;
    let main_ts = r#"
        import { Box } from "./base";
        export class NumberBox implements Box<number> {
            value = 0;
            get(): number { return this.value }
        }
    "#;
    let diags = check(&[("main.ts", main_ts), ("base.ts", base_ts)], "main.ts");
    assert_clean(&diags, "class-heritage-target");
}

#[test]
fn cross_file_generic_interface_alongside_unrelated_type_annotation() {
    // Regression witness for the specific failure mode that surfaced while
    // fixing #16434: a `Plain<number>` type annotation appearing ANYWHERE
    // ELSE in the file (here, an unrelated ambient function signature) can
    // resolve and cache the interface's own type first. A fix that only
    // works on a cold cache passed every other case in this file and still
    // produced a false TS2416 here, because the earlier resolution reused
    // the same `DefId` without ever deriving its type parameters.
    let types_ts = r#"
        export interface Plain<T> { plain(): T }
    "#;
    let actor_ts = r#"
        import { Plain } from "./types";
        export class Actor implements Plain<number> {
            public plain(): number { return 1 }
        }
        declare function unrelated(r: Plain<number>): void;
    "#;
    let diags = check(
        &[("actor.ts", actor_ts), ("types.ts", types_ts)],
        "actor.ts",
    );
    assert_clean(&diags, "alongside-unrelated-annotation");
}

// ── Negative controls: must not move ─────────────────────────────────────────

#[test]
fn same_file_generic_interface_stays_clean() {
    let source = r#"
        interface Plain<T> { plain(): T }
        class Actor implements Plain<number> {
            public plain(): number { return 1 }
        }
    "#;
    let diags = check(&[("actor.ts", source)], "actor.ts");
    assert_clean(&diags, "same-file");
}

#[test]
fn cross_file_non_generic_interface_stays_clean() {
    let types_ts = r#"
        export interface Named { name(): string }
    "#;
    let main_ts = r#"
        import { Named } from "./types";
        export class Thing implements Named {
            public name(): string { return "x" }
        }
    "#;
    let diags = check(&[("main.ts", main_ts), ("types.ts", types_ts)], "main.ts");
    assert_clean(&diags, "non-generic");
}

#[test]
fn cross_file_generic_interface_real_mismatch_still_reported() {
    // The fix must instantiate the interface correctly, not stop checking it.
    // `plain(): string` does not satisfy `Plain<number>`'s `plain(): number`,
    // so tsc still reports TS2420 (class does not correctly implement).
    let types_ts = r#"
        export interface Plain<T> { plain(): T }
    "#;
    let actor_ts = r#"
        import { Plain } from "./types";
        export class Actor implements Plain<number> {
            public plain(): string { return "x" }
        }
    "#;
    let diags = check(
        &[("actor.ts", actor_ts), ("types.ts", types_ts)],
        "actor.ts",
    );
    let cs = codes(&diags);
    assert!(
        cs.contains(&2420) || cs.contains(&2416),
        "expected a real TS2420/TS2416 mismatch (plain(): string vs Plain<number>'s \
         plain(): number) to still be reported after instantiation, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cross_file_generic_interface_wrong_type_argument_still_reported() {
    // `Plain<number>` implemented with a `string`-returning method is wrong
    // regardless of cross-file resolution; must not be silently accepted.
    let types_ts = r#"
        export interface Container<T> { value: T }
    "#;
    let main_ts = r#"
        import { Container } from "./types";
        export class StringBox implements Container<number> {
            public value: string = "x";
        }
    "#;
    let diags = check(&[("main.ts", main_ts), ("types.ts", types_ts)], "main.ts");
    let cs = codes(&diags);
    assert!(
        cs.contains(&2416) || cs.contains(&2322) || cs.contains(&2420),
        "expected the value:string vs Container<number>'s value:number mismatch to still \
         be reported, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
