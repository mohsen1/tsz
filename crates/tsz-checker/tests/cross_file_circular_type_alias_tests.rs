//! Cross-file circular type-alias detection (TS2456).
//!
//! When two type aliases in different files form a mutual cycle through
//! type-only imports, `tsc` reports `TS2456 "Type alias 'X' circularly
//! references itself."` at *each* alias's local declaration. tsz's cross-file
//! alias resolution runs through child-checker delegation, which collapses the
//! `Lazy(DefId)` re-entry to `ERROR` — leaving neither a `Lazy` chain nor a
//! `circular_def` mark — so the post-pass detection reconstructs the cycle
//! structurally from the alias declarations across files.
//!
//! Mirrors the conformance cases `externalModules/typeOnly/circular2.ts`
//! (simple `type A = B` / `type B = A`) and `circular4.ts` (namespace-nested
//! `ns1.nested.T = ns2.nested.T`). The structural rule is keyed on the cycle
//! shape, not on the chosen alias/namespace names, so the renamed variants
//! below must behave identically.

use tsz_checker::context::CheckerOptions;

fn codes_for(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(files, entry, CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// circular2 shape: `type A = B` / `type B = A` via `import type`. Each file's
/// local alias must be reported circular when that file is the entry.
#[test]
fn simple_two_file_cycle_emits_ts2456_for_each_file() {
    let a = "import type { B } from './b';\nexport type A = B;\n";
    let b = "import type { A } from './a';\nexport type B = A;\n";
    let files = [("a.ts", a), ("b.ts", b)];
    for entry in ["a.ts", "b.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            codes.contains(&2456),
            "entry {entry} should emit TS2456 for the cross-file alias cycle, got: {codes:?}"
        );
    }
}

/// Renamed variant of the simple cycle. The fix must not key on the names
/// `A`/`B` — any two mutually-referential aliases cycle the same way.
#[test]
fn renamed_two_file_cycle_emits_ts2456() {
    let a = "import type { Bar } from './b';\nexport type Foo = Bar;\n";
    let b = "import type { Foo } from './a';\nexport type Bar = Foo;\n";
    let files = [("a.ts", a), ("b.ts", b)];
    for entry in ["a.ts", "b.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            codes.contains(&2456),
            "entry {entry} renamed cycle should emit TS2456, got: {codes:?}"
        );
    }
}

/// circular4 shape: the cycle runs through namespace-qualified members
/// (`ns1.nested.T` / `ns2.nested.T`) across files, so resolution must follow
/// namespace exports, not just bare top-level names.
#[test]
fn namespace_nested_cross_file_cycle_emits_ts2456() {
    let a = "import type { ns2 } from './b';\n\
             export namespace ns1 {\n  export namespace nested {\n    export type T = ns2.nested.T;\n  }\n}\n";
    let b = "import type { ns1 } from './a';\n\
             export namespace ns2 {\n  export namespace nested {\n    export type T = ns1.nested.T;\n  }\n}\n";
    let files = [("a.ts", a), ("b.ts", b)];
    for entry in ["a.ts", "b.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            codes.contains(&2456),
            "entry {entry} namespace-nested cycle should emit TS2456, got: {codes:?}"
        );
    }
}

/// Three-file cycle `a -> b -> c -> a` — the walk must traverse an arbitrary
/// number of cross-file hops before closing the loop.
#[test]
fn three_file_cycle_emits_ts2456() {
    let a = "import type { B } from './b';\nexport type A = B;\n";
    let b = "import type { C } from './c';\nexport type B = C;\n";
    let c = "import type { A } from './a';\nexport type C = A;\n";
    let files = [("a.ts", a), ("b.ts", b), ("c.ts", c)];
    for entry in ["a.ts", "b.ts", "c.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            codes.contains(&2456),
            "entry {entry} three-file cycle should emit TS2456, got: {codes:?}"
        );
    }
}

/// Negative: a cross-file alias chain that terminates in a concrete type is
/// not circular and must not report TS2456.
#[test]
fn non_circular_cross_file_chain_has_no_ts2456() {
    let a = "import type { B } from './b';\nexport type A = B;\n";
    let b = "export type B = number;\n";
    let files = [("a.ts", a), ("b.ts", b)];
    let codes = codes_for(&files, "a.ts");
    assert!(
        !codes.contains(&2456),
        "non-circular chain must not emit TS2456, got: {codes:?}"
    );
}

/// Negative: a structural wrapper (array) defers the back-reference, so
/// `type A = B` / `type B = A[]` is a legal recursive type, not a circular
/// alias — `tsc` reports no TS2456 here.
#[test]
fn deferred_cross_file_cycle_through_array_has_no_ts2456() {
    let a = "import type { B } from './b';\nexport type A = B;\n";
    let b = "import type { A } from './a';\nexport type B = A[];\n";
    let files = [("a.ts", a), ("b.ts", b)];
    for entry in ["a.ts", "b.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            !codes.contains(&2456),
            "entry {entry}: array-deferred recursive alias must not emit TS2456, got: {codes:?}"
        );
    }
}

/// Negative: a structural wrapper (object literal) likewise defers the
/// back-reference; `type B = { next: A }` is a legal recursive type.
#[test]
fn deferred_cross_file_cycle_through_object_has_no_ts2456() {
    let a = "import type { B } from './b';\nexport type A = B;\n";
    let b = "import type { A } from './a';\nexport type B = { next: A };\n";
    let files = [("a.ts", a), ("b.ts", b)];
    for entry in ["a.ts", "b.ts"] {
        let codes = codes_for(&files, entry);
        assert!(
            !codes.contains(&2456),
            "entry {entry}: object-deferred recursive alias must not emit TS2456, got: {codes:?}"
        );
    }
}

/// Negative: a generic alias whose type-parameter name shadows a top-level
/// alias name must not be mistaken for a cycle. In `type T = U; type U<T> = T`
/// the `T` inside `U`'s body is `U`'s parameter, not the alias `T`. `tsc`
/// reports only TS2314 (missing type argument), never TS2456; the structural
/// walk must be scope-aware and skip in-scope type-parameter names.
#[test]
fn type_parameter_shadowing_alias_name_has_no_ts2456() {
    let src = "type T = U;\ntype U<T> = T;\nexport {};\n";
    let files = [("a.ts", src)];
    let codes = codes_for(&files, "a.ts");
    assert!(
        !codes.contains(&2456),
        "type-parameter shadowing must not emit TS2456 (only TS2314), got: {codes:?}"
    );
}
