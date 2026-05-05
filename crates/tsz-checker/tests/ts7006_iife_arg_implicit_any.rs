//! Tests for TS7006 ("Parameter X implicitly has an 'any' type") emission for
//! function expressions inside IIFE arguments.
//!
//! Regression for `contextuallyTypedIifeStrict.ts`: when an IIFE argument is
//! an object literal whose property value is a function expression with an
//! unannotated parameter, tsc emits TS7006 for that parameter. tsz used to
//! suppress it because the object-literal type-refresh path cleared all
//! implicit-any diagnostics in the function's parameter span whenever the
//! refresh saw "a concrete contextual type" — even when that contextual type
//! was the property's own self-referential function shape, which doesn't
//! actually contextually type the parameter.

use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_with_starts_for(source: &str) -> Vec<(u32, u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

fn diagnostics_for(source: &str) -> Vec<(u32, String)> {
    diagnostics_with_starts_for(source)
        .into_iter()
        .map(|(code, _, message)| (code, message))
        .collect()
}

/// `function(n)` lives inside the object literal `{ a: function(n) {…} }`,
/// which is the argument to `(o => o.a(11))`. The IIFE's `o` parameter has no
/// annotation, so the object literal isn't actually contextually typed. tsc
/// emits TS7006 for `n` because the property's "contextual type" is just the
/// object literal's own inferred shape, which doesn't constrain `n`.
#[test]
fn ts7006_for_function_param_inside_iife_object_literal_arg() {
    let source = r#"
let eleven = (o => o.a(11))({ a: function(n) { return n; } });
"#;
    let diags = diagnostics_for(source);
    let ts7006_n: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 7006 && msg.contains("'n'"))
        .collect();
    assert!(
        !ts7006_n.is_empty(),
        "expected TS7006 for parameter 'n' inside the IIFE arg's function expression; got: {diags:?}"
    );
}

/// Sanity: when a property value's function expression DOES have a real
/// contextual type (annotated variable), the existing clear-stale logic
/// should still suppress TS7006. This pins down the asymmetry — the fix must
/// not regress the genuine contextual-typing path.
#[test]
fn no_ts7006_for_function_param_with_real_contextual_type() {
    let source = r#"
let f: { a: (n: number) => number } = { a: function(n) { return n; } };
"#;
    let diags = diagnostics_for(source);
    let ts7006_n: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 7006 && msg.contains("'n'"))
        .collect();
    assert!(
        ts7006_n.is_empty(),
        "TS7006 must NOT fire for 'n' when the property has a real `(n: number) => number` contextual type; got: {ts7006_n:?}"
    );
}

#[test]
fn ts2590_and_ts7006_anchor_to_nested_callback_param_when_context_too_complex() {
    let source = r#"
interface Obj<T> {
    ref: T;
}
interface Func<T> {
    (x: T): void;
}
type UnionToIntersection<U> =
    (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never;
type CtorOf<T> = (arg: UnionToIntersection<T>) => T;

interface Big {
    "0": { common?: string; "0"?: number, ref?: Obj<Big["0"]> | Func<Big["0"]>; }
    "1": { common?: string; "1"?: number, ref?: Obj<Big["1"]> | Func<Big["1"]>; }
    "2": { common?: string; "2"?: number, ref?: Obj<Big["2"]> | Func<Big["2"]>; }
    "3": { common?: string; "3"?: number, ref?: Obj<Big["3"]> | Func<Big["3"]>; }
    "4": { common?: string; "4"?: number, ref?: Obj<Big["4"]> | Func<Big["4"]>; }
    "5": { common?: string; "5"?: number, ref?: Obj<Big["5"]> | Func<Big["5"]>; }
    "6": { common?: string; "6"?: number, ref?: Obj<Big["6"]> | Func<Big["6"]>; }
    "7": { common?: string; "7"?: number, ref?: Obj<Big["7"]> | Func<Big["7"]>; }
    "8": { common?: string; "8"?: number, ref?: Obj<Big["8"]> | Func<Big["8"]>; }
    "9": { common?: string; "9"?: number, ref?: Obj<Big["9"]> | Func<Big["9"]>; }
    "10": { common?: string; "10"?: number, ref?: Obj<Big["10"]> | Func<Big["10"]>; }
    "11": { common?: string; "11"?: number, ref?: Obj<Big["11"]> | Func<Big["11"]>; }
    "12": { common?: string; "12"?: number, ref?: Obj<Big["12"]> | Func<Big["12"]>; }
    "13": { common?: string; "13"?: number, ref?: Obj<Big["13"]> | Func<Big["13"]>; }
    "14": { common?: string; "14"?: number, ref?: Obj<Big["14"]> | Func<Big["14"]>; }
    "15": { common?: string; "15"?: number, ref?: Obj<Big["15"]> | Func<Big["15"]>; }
    "16": { common?: string; "16"?: number, ref?: Obj<Big["16"]> | Func<Big["16"]>; }
    "17": { common?: string; "17"?: number, ref?: Obj<Big["17"]> | Func<Big["17"]>; }
}
declare function getCtor<T extends keyof Big>(comp: T): CtorOf<Big[T]>

declare var all: keyof Big;
const ctor = getCtor(all);
const comp = ctor({ common: "ok", ref: x => console.log(x) });
"#;

    let param_start = source.find("x =>").expect("test source has callback") as u32;
    let diags = diagnostics_with_starts_for(source);

    assert!(
        diags
            .iter()
            .any(|(code, start, _)| *code == 2590 && *start == param_start),
        "expected TS2590 at the nested callback parameter; got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, start, msg)| {
            *code == 7006 && *start == param_start && msg.contains("'x'")
        }),
        "expected TS7006 for the nested callback parameter; got: {diags:?}"
    );
}
