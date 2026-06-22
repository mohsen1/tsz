//! `TS2345` must render a `readonly` array / `readonly` tuple ARGUMENT by the
//! non-generic type-alias name it was referenced through (its `aliasSymbol`),
//! matching `tsc`, instead of the structurally-interned form
//! (`readonly number[]`).
//!
//! This is the argument-mismatch follow-up of #14483 (the `TS4104`
//! readonly-to-mutable slice is handled separately). tsz interns array and
//! `readonly` array/tuple types purely structurally, so a shared
//! `readonly number[]` `TypeId` carries no per-reference alias; `tsc` recovers
//! the name from the argument expression's declared annotation, and tsz must do
//! the same. The rule is structural — it holds regardless of the alias name,
//! element types, and tuple arity — and must NOT fire for an inline
//! `readonly number[]` annotation (no alias) or repaint a generic alias
//! application (which already keeps its name via the `Application` path).

use tsz_checker::test_utils::check_source_code_messages;

fn messages(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn message_for(source: &str, code: u32) -> String {
    messages(source)
        .into_iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| m)
        .unwrap_or_else(|| {
            panic!(
                "expected TS{code} for source:\n{source}\ngot: {:?}",
                messages(source)
            )
        })
}

#[test]
fn ts2345_readonly_array_alias_argument_renders_alias_name() {
    let msg = message_for(
        "type Bag = readonly number[]; const b: Bag = []; function need(x: number[]) {} need(b);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Bag' is not assignable to parameter of type 'number[]'."
    );
}

#[test]
fn ts2345_readonly_array_alias_argument_is_structural_not_name_keyed() {
    // Renamed binder + different element type: proves the recovery is structural,
    // not keyed on the alias spelling `Bag` or the element type `number`.
    let msg = message_for(
        "type Grid = readonly string[]; const g: Grid = []; function need(x: string[]) {} need(g);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Grid' is not assignable to parameter of type 'string[]'."
    );
}

#[test]
fn ts2345_readonly_tuple_alias_argument_renders_alias_name() {
    let msg = message_for(
        "type Pair = readonly [number, string]; const p: Pair = [1, 'a']; function need(x: [number, string]) {} need(p);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Pair' is not assignable to parameter of type '[number, string]'."
    );
}

#[test]
fn ts2345_readonly_tuple_alias_argument_arity_three_renders_alias_name() {
    let msg = message_for(
        "type Triple = readonly [number, string, boolean]; const t: Triple = [1, 'a', true]; function need(x: [number, string, boolean]) {} need(t);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Triple' is not assignable to parameter of type '[number, string, boolean]'."
    );
}

#[test]
fn ts2345_inline_readonly_array_argument_keeps_structural_display() {
    // No alias: the inline `readonly number[]` annotation must stay structural,
    // exactly as `tsc` renders it.
    let msg = message_for(
        "const ra: readonly number[] = []; function need(x: number[]) {} need(ra);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'readonly number[]' is not assignable to parameter of type 'number[]'."
    );
}

#[test]
fn ts2345_generic_readonly_array_alias_application_keeps_name() {
    // A generic alias survives as an `Application` and already renders by name;
    // the recovery must not interfere with it.
    let msg = message_for(
        "type Frozen<T> = readonly T[]; const f: Frozen<number> = []; function need(x: number[]) {} need(f);",
        2345,
    );
    assert_eq!(
        msg,
        "Argument of type 'Frozen<number>' is not assignable to parameter of type 'number[]'."
    );
}
