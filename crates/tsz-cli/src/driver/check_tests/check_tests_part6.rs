#[test]
fn function_value_augmentation_has_no_import_type_meaning() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/value-only-home.ts",
            r#"
export function ValueOnly(value: boolean): boolean { return value; }
"#,
        ),
        (
            "/project/value-only-augmentation.ts",
            r#"
import "./value-only-home";
declare module "./value-only-home" {
    function ValueOnly(value: boolean): boolean;
}
"#,
        ),
        (
            "/project/value-only-consumer.ts",
            r#"
type RejectedValueOnly = import("./value-only-home").ValueOnly;
"#,
        ),
    ]);

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2694],
        "a function augmentation adds value-space overloads but no import-type meaning; got: {diagnostics:?}"
    );
    assert!(diagnostics[0].message_text.contains("ValueOnly"));
}

#[test]
fn repeated_local_augmentation_base_declarations_merge_before_heritage() {
    let diagnostics = collect_test_diagnostics(&[
        (
            "/project/repeated-base-home.ts",
            r#"
export interface RepeatedBase { home: true }
export interface RepeatedDerived {}
"#,
        ),
        (
            "/project/repeated-base-augmentation.ts",
            r#"
import "./repeated-base-home";
declare module "./repeated-base-home" {
    interface RepeatedBase {
        first: "first";
        new (value: "specific"): { kind: "specific" };
    }
    interface RepeatedBase {
        second: "second";
        new (value: number): { kind: "number" };
    }
    interface RepeatedDerived extends RepeatedBase { own: true }
}
"#,
        ),
        (
            "/project/repeated-base-consumer.ts",
            r#"
import type { RepeatedDerived } from "./repeated-base-home";
declare const value: RepeatedDerived;
const first: "first" = value.first;
const second: "second" = value.second;
const specific: "specific" = new value("specific").kind;
const numberResult: "number" = new value(1).kind;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "a sibling heritage query must consume every declaration owned by its exact augmentation-local base symbol: {diagnostics:?}"
    );
}
