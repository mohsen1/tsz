//! Parity tests for the `TS4104` ("The type 'X' is 'readonly' and cannot be
//! assigned to the mutable type 'Y'") source display.
//!
//! Structural rule: when a readonly array / readonly tuple is assigned to a
//! mutable array / tuple, `tsc` renders the source by the **type-alias name** it
//! was referenced through (its `aliasSymbol`), e.g. `The type 'RA' is
//! 'readonly' …`, not the expanded `readonly number[]`. `tsz` interns array and
//! readonly array/tuple types purely structurally, so the per-reference alias is
//! recovered from the source expression's declared annotation rather than from
//! the type. Inline (non-aliased) readonly sources keep their structural display,
//! and generic alias applications keep their `Name<Args>` surface.
//!
//! Owner layer: checker error reporter
//! (`render_failure::ReadonlyToMutableAssignment`), via
//! `declared_source_type_reference_alias_name`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn ts4104_message(diagnostics: &[Diagnostic]) -> String {
    let mut msgs: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error && d.code == 4104)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one TS4104 diagnostic, got: {diagnostics:#?}"
    );
    msgs.remove(0).to_string()
}

#[test]
fn readonly_array_alias_source_renders_alias_name() {
    // `readonly number[]` aliased as `RA`: tsc shows `The type 'RA' is …`.
    let source = r#"
        type RA = readonly number[];
        const value: RA = [1, 2, 3];
        const mutable: number[] = value;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg, "The type 'RA' is 'readonly' and cannot be assigned to the mutable type 'number[]'.",
        "array-alias source must render the alias name, got: {msg}"
    );
}

#[test]
fn application_bodied_alias_source_renders_alias_name() {
    // A non-generic alias whose body is a generic application that resolves to a
    // readonly array (`Frozen = RArr<number>`) still renders by the alias name —
    // tsc keeps the `aliasSymbol` on the freshly-constructed readonly array.
    let source = r#"
        type RArr<T> = readonly T[];
        type Frozen = RArr<number>;
        const xs: Frozen = [1];
        const mutable: number[] = xs;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'Frozen' is 'readonly' and cannot be assigned to the mutable type 'number[]'.",
        "application-bodied alias source must render the alias name, got: {msg}"
    );
}

#[test]
fn readonly_tuple_alias_source_renders_alias_name() {
    // `readonly [number, string]` aliased as `Pair`.
    let source = r#"
        type Pair = readonly [number, string];
        const pair: Pair = [1, "x"];
        const mutable: [number, string] = pair;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'Pair' is 'readonly' and cannot be assigned to the mutable type '[number, string]'.",
        "readonly-tuple-alias source must render the alias name, got: {msg}"
    );
}

#[test]
fn alias_display_is_binder_name_agnostic() {
    // The decision is structural, not keyed to a spelling: renaming the alias
    // changes only the rendered name.
    let source = r#"
        type Immutables = readonly number[];
        const seq: Immutables = [9];
        const mutable: number[] = seq;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'Immutables' is 'readonly' and cannot be assigned to the mutable type 'number[]'.",
        "renamed alias must render under its own name, got: {msg}"
    );
}

#[test]
fn inline_readonly_array_source_keeps_structural_display() {
    // No alias reference: tsc (and tsz) render the structural form.
    let source = r#"
        const value: readonly number[] = [1, 2, 3];
        const mutable: number[] = value;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'readonly number[]' is 'readonly' and cannot be assigned to the mutable type 'number[]'.",
        "inline readonly array must keep structural display, got: {msg}"
    );
}

#[test]
fn inline_readonly_array_source_ignores_coincidental_alias_body() {
    let source = r#"
        type RStrings = readonly string[];
        function f(value: readonly string[]) {
            const mutable: string[] = value;
        }
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'readonly string[]' is 'readonly' and cannot be assigned to the mutable type 'string[]'.",
        "inline readonly array must not borrow a coincidental alias, got: {msg}"
    );
}

#[test]
fn inline_readonly_tuple_source_keeps_structural_display() {
    let source = r#"
        const pair: readonly [number, string] = [1, "x"];
        const mutable: [number, string] = pair;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'readonly [number, string]' is 'readonly' and cannot be assigned to the mutable type '[number, string]'.",
        "inline readonly tuple must keep structural display, got: {msg}"
    );
}

#[test]
fn generic_alias_application_source_keeps_application_surface() {
    // A generic alias application keeps its `Name<Args>` form (it is not a bare
    // alias reference); the structural formatter already renders it correctly.
    let source = r#"
        type Immutable<T> = readonly T[];
        const xs: Immutable<string> = ["a"];
        const mutable: string[] = xs;
    "#;
    let msg = ts4104_message(&check(source));
    assert_eq!(
        msg,
        "The type 'Immutable<string>' is 'readonly' and cannot be assigned to the mutable type 'string[]'.",
        "generic alias application must keep its application surface, got: {msg}"
    );
}
