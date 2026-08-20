//! Regression coverage for the msw `unique symbol` member-lookup
//! false-positive family (sse.ts / RequestHandler.ts canary sites).
//!
//! Three structural rules, one per sub-bug:
//!
//! 1. **Split accessors (relations)**: when relating object types, tsc
//!    compares only property *read* types; setter/write types never
//!    participate in assignability, subtype, or conditional-`extends`
//!    relations (TS 4.3 divergent accessors). Owner:
//!    `tsz_solver::relations::subtype` property compatibility.
//!
//! 2. **In-flight class constructor types (caching)**: a class constructor
//!    type computed while the class's own instance type is still being built
//!    (static self-reference forcing `typeof C` mid-build) embeds the
//!    provisional prescan instance shape — missing computed/symbol-keyed
//!    members and heritage — and must not be cached. Owner:
//!    `CheckerState::get_type_of_symbol` result caching and
//!    `class_constructor_type_cache` insertion.
//!
//! 3. **Well-known symbol `in` guards (narrowing)**: members keyed by a
//!    built-in `Symbol.*` unique symbol are stored under the
//!    "[Symbol.<name>]" member name, so `Symbol.iterator in x` must narrow
//!    with that key, not the generic "__unique_<id>" name. Owner:
//!    `FlowAnalyzer::literal_atom_and_kind_from_node_or_type`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn check_with_libs(src: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(src, "test.ts", CheckerOptions::default(), &libs)
}

fn assert_clean_setup(diags: &[Diagnostic], label: &str) {
    // Guard against vacuous passes: the witness must not have unresolved
    // global/lib names (TS2304/TS2583/TS2584), otherwise the asserted codes
    // could be absent merely because the types collapsed to errors.
    assert!(
        !diags.iter().any(|d| matches!(d.code, 2304 | 2583 | 2584)),
        "{label}: witness has unresolved names; fix the test source: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

fn assert_no_codes(label: &str, src: &str, codes: &[u32]) {
    let diags = check_with_libs(src);
    assert_clean_setup(&diags, label);
    let bad: Vec<_> = diags.iter().filter(|d| codes.contains(&d.code)).collect();
    assert!(
        bad.is_empty(),
        "{label}: expected none of {codes:?}; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

fn assert_has_code(label: &str, src: &str, code: u32) {
    let diags = check_with_libs(src);
    assert_clean_setup(&diags, label);
    assert!(
        diags.iter().any(|d| d.code == code),
        "{label}: expected TS{code}; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-bug 1: split-accessor write types must not participate in relations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn divergent_accessor_class_assignable_to_plain_property_interface() {
    // tsc-verified clean: read types match; the narrower setter is ignored.
    assert_no_codes(
        "divergent accessor → interface",
        r#"
type Cb = (ev: string) => void

interface Sink {
    handler: Cb | null
}

class Holder {
    #cb: Cb | null = null
    get handler(): Cb | null {
        return this.#cb
    }
    set handler(value: Cb) {
        this.#cb = value
    }
}

declare const h: Holder
const sink: Sink = h
declare function take(s: Sink): void
take(h)
"#,
        &[2322, 2345],
    );
}

#[test]
fn divergent_accessor_object_types_relate_through_read_types_both_directions() {
    // tsc-verified clean in all four combinations (wt.ts witness).
    assert_no_codes(
        "split accessor anonymous object relations",
        r#"
declare const a: { get x(): string; set x(v: string) }
const b: { get x(): string; set x(v: string | number) } = a
declare const c: { get x(): string; set x(v: string | number) }
const d: { get x(): string; set x(v: string) } = c
declare const e: { x: string }
const f: { get x(): string; set x(v: string | number) } = e
const g: { x: string } = c
declare const h: { get x(): string; set x(v: string) }
const i: { readonly x: string } = h
export {}
"#,
        &[2322],
    );
}

#[test]
fn divergent_accessor_conditional_extends_ignores_write_types() {
    // `T extends { get x; set x(wider) }` matches on read types in tsc.
    assert_no_codes(
        "conditional extends with split accessor",
        r#"
type W<T> = T extends { get x(): string; set x(v: string | number) } ? 1 : 0
type T1 = W<{ x: string }>
declare const probe: T1
const one: 1 = probe
export {}
"#,
        &[2322],
    );
}

#[test]
fn divergent_accessor_read_type_mismatch_still_errors() {
    // Negative control: an incompatible *read* type must keep failing.
    assert_has_code(
        "read type mismatch",
        r#"
declare const a: { get x(): number; set x(v: number | string) }
const b: { x: string } = a
export {}
"#,
        2322,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-bug 2: provisional class constructor types must not leak via caches
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn early_new_of_class_with_static_self_ref_sees_symbol_member() {
    // The msw ObservableEventSource shape: a method declared BEFORE the class
    // forces the class type; the class has a static self-reference and a
    // generic method. The `new` result must include the symbol-keyed member
    // and satisfy the declared return type.
    assert_no_codes(
        "use-before-decl class with static self-ref",
        r#"
class Maker {
    build(): Widget {
        const w = new Widget()
        w[kHandler] = (event) => {
            console.log(event)
        }
        return w
    }
}

const kHandler = Symbol('kHandler')

class Widget {
    static readonly MODE = 2
    public readonly MODE = Widget.MODE
    private [kHandler]: ((event: string) => void) | null = null
    public listen<K extends string>(name: K): void {
        console.log(name)
    }
}
export {}
"#,
        &[7053, 7006, 2739, 2741, 2322],
    );
}

#[test]
fn early_new_of_extending_class_with_static_self_ref_keeps_heritage() {
    // Same shape but with a lib base class: the `new` result must also keep
    // inherited members (dispatchEvent) — the provisional prescan type has
    // neither heritage nor computed members.
    assert_no_codes(
        "use-before-decl extending class keeps heritage",
        r#"
class Driver {
    open(): Source {
        const s = new Source()
        s[kOnAny] = (event) => {
            console.log(event)
        }
        s.dispatchEvent(new Event('ping'))
        return s
    }
}

const kOnAny = Symbol('kOnAny')

class Source extends EventTarget {
    static readonly OPEN = 1
    public readonly OPEN = Source.OPEN
    private [kOnAny]: ((event: string) => void) | null = null
    public on<K extends string>(name: K): void {
        console.log(name)
    }
}
export {}
"#,
        &[7053, 7006, 2739, 2741, 2339, 2322],
    );
}

#[test]
fn class_declared_before_use_still_resolves_symbol_member() {
    // Order control: class first, use after (already-working path must stay).
    assert_no_codes(
        "class-first order control",
        r#"
const kTag = Symbol('kTag')

class Box {
    static readonly N = 1
    public readonly N = Box.N
    private [kTag]: ((event: string) => void) | null = null
    public touch<K extends string>(name: K): void {
        console.log(name)
    }
}

const b = new Box()
b[kTag] = (event) => {
    console.log(event)
}
export {}
"#,
        &[7053, 7006, 2739, 2741, 2322],
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-bug 3: `Symbol.iterator in x` narrowing uses the well-known member name
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn well_known_symbol_in_guard_narrows_iterable_union() {
    // The msw RequestHandler.ts:450 shape.
    assert_no_codes(
        "Symbol.iterator in union",
        r#"
function pick(result: Iterable<number> | AsyncIterable<number>) {
    return Symbol.iterator in result
        ? result[Symbol.iterator]()
        : result[Symbol.asyncIterator]()
}
export { pick }
"#,
        &[2571, 7053],
    );
}

#[test]
fn well_known_symbol_in_guard_narrows_user_interfaces() {
    // Renamed-binder adjacent case with user-declared interfaces.
    assert_no_codes(
        "Symbol.asyncIterator in user union",
        r#"
interface SyncThing {
    [Symbol.iterator](): Iterator<string>
}
interface AsyncThing {
    [Symbol.asyncIterator](): AsyncIterator<string>
}

function choose(thing: SyncThing | AsyncThing) {
    if (Symbol.asyncIterator in thing) {
        return thing[Symbol.asyncIterator]()
    }
    return thing[Symbol.iterator]()
}
export { choose }
"#,
        &[2571, 7053, 2339],
    );
}

#[test]
fn user_unique_symbol_in_guard_still_narrows() {
    // The generic "__unique_<id>" path must keep working for user symbols.
    assert_no_codes(
        "user unique symbol in guard",
        r#"
const kA: unique symbol = Symbol('kA')
const kB: unique symbol = Symbol('kB')

interface WithA {
    [kA]: () => string
}
interface WithB {
    [kB]: () => number
}

function read(value: WithA | WithB) {
    if (kA in value) {
        return value[kA]()
    }
    return value[kB]()
}
export { read }
"#,
        &[2571, 7053, 2339],
    );
}

#[test]
fn wide_symbol_key_indexing_union_still_errors() {
    // Negative control: a non-unique `symbol`-typed key cannot index a union
    // without a matching index signature (tsc reports TS7053 too).
    assert_has_code(
        "wide symbol key negative control",
        r#"
declare const wide: symbol
declare const uni: Iterable<number> | AsyncIterable<number>
const v = uni[wide]
export { v }
"#,
        7053,
    );
}
