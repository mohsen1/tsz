//! Excess-property coverage for a mapped type whose finite constraint is
//! `keyof` an intersection of key-remapped mapped types.
//!
//! The key query must inspect remapped keys only. The source retains the
//! discriminated union and nested mapped aliases that keep the remaps deferred
//! in the canary shape.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_lib_files};

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = load_lib_files(&["es5.d.ts"]);
    assert_eq!(libs.len(), 1, "the regression requires the real Pick alias");
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
}

const REMAPPED_KEYOF_SOURCE: &str = r#"
type Code =
    | "network"
    | "storage"
    | "syntax"
    | "content"
    | "arguments"
    | "return"
    | "resolve"
    | "constraint"
    | "missing"
    | "present"
    | "nothing"
    | "parsing"
    | "instance";

type Fields = {
    ready: false;
    kind: Code;
    note: string;
    wanted: number;
    payload: unknown;
    details: { [name: string]: Packet };
    detail: Packet;
    thrown: unknown;
};

type Packet =
    | ((Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload"> &
        (Pick<Fields, "details"> | Pick<Fields, "detail"> | object)) &
        { kind: "network" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload"> &
        { kind: "storage" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "detail"> &
        { kind: "syntax" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "details"> &
        { kind: "content" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "detail"> &
        { kind: "arguments" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "detail"> &
        { kind: "return" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "detail"> &
        { kind: "resolve" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "thrown"> &
        { kind: "constraint" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted"> &
        { kind: "missing" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload"> &
        { kind: "present" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload"> &
        { kind: "nothing" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "thrown"> &
        { kind: "parsing" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never)
    | (Pick<Fields, "ready" | "note" | "kind" | "wanted" | "payload" | "thrown"> &
        { kind: "instance" } extends infer Branch
            ? { [Field in keyof Branch]: Branch[Field] }
            : never);

type Builder<Selected extends Code> =
    (Packet & { kind: Selected }) extends infer Narrowed
        ? ({
            [Slot in keyof Narrowed as Slot extends "wanted" ? Slot : never]: string
        } & {
            [Slot in keyof Narrowed as
                Slot extends "ready" | "note" | "kind" | "wanted" ? never : Slot
            ]: Narrowed[Slot]
        }) extends infer Joined
            ? { [Field in keyof Joined]: Joined[Field] }
            : never
        : never;

declare function consume(input: Builder<"network">): void;
"#;

#[test]
fn remapped_keyof_intersection_accepts_each_preserved_key() {
    let source = format!(
        r#"{REMAPPED_KEYOF_SOURCE}
consume({{ wanted: "retry", payload: undefined }});
"#
    );
    let diagnostics = check(&source);
    assert!(
        diagnostics.is_empty(),
        "each key preserved by the two remaps must be accepted: {diagnostics:#?}"
    );
}

#[test]
fn remapped_keyof_intersection_rejects_a_filtered_key() {
    let source = format!(
        r#"{REMAPPED_KEYOF_SOURCE}
consume({{ wanted: "retry", payload: undefined, note: "filtered" }});
"#
    );
    let diagnostics = check(&source);
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2353],
        "a genuinely filtered key must remain excess: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].message_text.contains("note"),
        "the excess-property diagnostic must identify the filtered key: {diagnostics:#?}"
    );
}
