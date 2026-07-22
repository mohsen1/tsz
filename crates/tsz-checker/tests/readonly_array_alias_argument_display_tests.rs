//! `TS4104` ("The type 'X' is 'readonly' and cannot be assigned to the mutable
//! type 'Y'") must render a `readonly` array / `readonly` tuple ARGUMENT by the
//! source's alias name where `tsc` does (`Bag`, `Frozen<number>`), and
//! structurally for an inline `readonly number[]` annotation (no alias).
//!
//! tsc reports readonly-to-mutable array/tuple ARGUMENTS as `TS4104`, not the
//! generic `TS2345` (see the `readonlyTupleAndArrayElaboration` conformance
//! fixture). tsz interns array and `readonly` array/tuple types purely
//! structurally, so a shared `readonly number[]` `TypeId` carries no per-reference
//! alias; the alias name is recovered from the source expression's declared
//! annotation, else the structural display is used.
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
fn ts4104_readonly_array_alias_argument_renders_alias_name() {
    let msg = message_for(
        "type Bag = readonly number[]; const b: Bag = []; function need(x: number[]) {} need(b);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'Bag' is 'readonly' and cannot be assigned to the mutable type 'number[]'."
    );
}

#[test]
fn ts4104_readonly_array_alias_argument_is_structural_not_name_keyed() {
    // Renamed binder + different element type: proves the recovery is structural,
    // not keyed on the alias spelling `Bag` or the element type `number`.
    let msg = message_for(
        "type Grid = readonly string[]; const g: Grid = []; function need(x: string[]) {} need(g);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'Grid' is 'readonly' and cannot be assigned to the mutable type 'string[]'."
    );
}

#[test]
fn ts4104_readonly_tuple_alias_argument_renders_alias_name() {
    let msg = message_for(
        "type Pair = readonly [number, string]; const p: Pair = [1, 'a']; function need(x: [number, string]) {} need(p);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'Pair' is 'readonly' and cannot be assigned to the mutable type '[number, string]'."
    );
}

#[test]
fn ts4104_readonly_tuple_alias_argument_arity_three_renders_alias_name() {
    let msg = message_for(
        "type Triple = readonly [number, string, boolean]; const t: Triple = [1, 'a', true]; function need(x: [number, string, boolean]) {} need(t);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'Triple' is 'readonly' and cannot be assigned to the mutable type '[number, string, boolean]'."
    );
}

#[test]
fn ts4104_inline_readonly_array_argument_keeps_structural_display() {
    // No alias: the inline `readonly number[]` annotation must stay structural,
    // exactly as `tsc` renders it.
    let msg = message_for(
        "const ra: readonly number[] = []; function need(x: number[]) {} need(ra);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'readonly number[]' is 'readonly' and cannot be assigned to the mutable type 'number[]'."
    );
}

#[test]
fn ts4104_generic_readonly_array_alias_application_keeps_name() {
    // A generic alias survives as an `Application` and already renders by name;
    // the recovery must not interfere with it.
    let msg = message_for(
        "type Frozen<T> = readonly T[]; const f: Frozen<number> = []; function need(x: number[]) {} need(f);",
        4104,
    );
    assert_eq!(
        msg,
        "The type 'Frozen<number>' is 'readonly' and cannot be assigned to the mutable type 'number[]'."
    );
}
