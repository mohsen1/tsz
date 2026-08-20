//! Project-mode coverage: an imported generic conditional alias whose `extends`
//! operand references a provider-private type.
//!
//! Witness (issue #13618, reduced to a deterministic two-file form):
//! ```ts
//! // pd.ts
//! type Rec = object;
//! export type Pick2<T> = T extends Rec ? T : number;
//! // use.ts
//! import { Pick2 } from './pd';
//! type Widget = { id: string };
//! declare const w: Pick2<Widget>;   // tsc: Widget; tsz: number (wrong branch)
//! ```
//!
//! Structural rule: when a generic conditional alias `A<T> = T extends Ref ? X
//! : Y` is exported from one module and instantiated in another, `tsc` resolves
//! `Ref` against the alias's *declaring* module — even when `Ref` is a
//! non-exported helper invisible in the consumer's scope. tsz must lower the
//! imported alias body in its declaring arena (cross-arena delegation) so `Ref`
//! binds to the provider symbol. Re-lowering the body in the consumer scope
//! leaves `Ref` an unresolved name, the conditional never settles, and the
//! alias degrades to the wrong branch (or a both-branches union), producing a
//! spurious `TS2322` — the cross-arena `error`/`never`-in-type-argument family.
//!
//! Root cause: the delegation gate (`should_delegate_dynamic_type_alias_owner`)
//! suppressed cross-arena delegation for a directly-referenced cross-file
//! `TYPE_ALIAS` whose name matched the consumer-visible symbol, so the body was
//! re-lowered locally. The fix delegates whenever the alias is not declared in
//! the current arena.
//!
//! These run the full project driver (shared `DefinitionStore`, project-mode
//! lib resolution): the divergence only arises under the project pipeline, where
//! the provider's body is lowered in its own arena. The matrix varies binder and
//! file names (anti-hardcoding) and includes the false branch, a private alias
//! chain, a nested provider-private conditional, and negative cases so the alias
//! is proven to resolve to a *concrete* branch type rather than `any`/`error`.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// "X is not assignable to Y" assignability false-positive family.
const TS2322: u32 = 2322;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "esnext", "module": "esnext", "moduleResolution": "bundler", "noEmit": true }}, "files": [{}] }}"#,
        names.join(", ")
    );
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    for (name, source) in files {
        fs::write(dir.path().join(name), source).expect("write source");
    }

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    compile(&args, dir.path())
        .expect("compile succeeds")
        .diagnostics
}

fn assignability_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == TS2322)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn imported_conditional_alias_true_branch_binds_private_extends() {
    // `Widget extends Rec(=object)` is true, so `Pick2<Widget>` is `Widget`.
    // Assigning it to `Widget` must be accepted; before the fix the consumer
    // could not bind the provider-private `Rec`, took the false branch, and
    // reported `number` not assignable to `Widget`.
    let files = &[
        (
            "pd.ts",
            "type Rec = object;\nexport type Pick2<T> = T extends Rec ? T : number;\n",
        ),
        (
            "use.ts",
            "import { Pick2 } from './pd';\n\
             type Widget = { id: string };\n\
             declare const w: Pick2<Widget>;\n\
             const ok: Widget = w;\n",
        ),
    ];
    let errors = assignability_errors(&compile_project(files));
    assert!(
        errors.is_empty(),
        "imported conditional alias must take the true branch (Pick2<Widget> = Widget); \
         expected no TS2322. Got: {errors:#?}"
    );
}

#[test]
fn imported_conditional_alias_true_branch_is_not_any_or_error() {
    // Negative half of the true-branch case: the resolved type is concretely
    // `Widget`, not `any`/`error` (which would silence every assignment). A
    // `number` target must therefore still be rejected.
    let files = &[
        (
            "pd.ts",
            "type Rec = object;\nexport type Pick2<T> = T extends Rec ? T : number;\n",
        ),
        (
            "use.ts",
            "import { Pick2 } from './pd';\n\
             type Widget = { id: string };\n\
             declare const w: Pick2<Widget>;\n\
             const bad: number = w;\n",
        ),
    ];
    let errors = assignability_errors(&compile_project(files));
    assert!(
        !errors.is_empty(),
        "Pick2<Widget> resolves to the concrete `Widget`, so assigning it to `number` \
         must still report TS2322 (proves it is not silenced to any/error). Got: {errors:#?}"
    );
}

#[test]
fn imported_conditional_alias_false_branch_binds_private_extends() {
    // Renamed binders + the *false* branch: `{ a: 1 } extends number` is false,
    // so `OnlyNum<{ a: 1 }>` is `string`. The assignment to `string` must be
    // accepted — the private `Numeric` constraint still has to bind for the
    // condition to settle on the false branch rather than a both-branches union.
    let files = &[
        (
            "numbers.ts",
            "type Numeric = number;\nexport type OnlyNum<P> = P extends Numeric ? P : string;\n",
        ),
        (
            "consumer.ts",
            "import { OnlyNum } from './numbers';\n\
             declare const x: OnlyNum<{ a: 1 }>;\n\
             const ok: string = x;\n",
        ),
    ];
    let errors = assignability_errors(&compile_project(files));
    assert!(
        errors.is_empty(),
        "imported conditional alias must settle on the false branch \
         (OnlyNum<{{ a: 1 }}> = string); expected no TS2322. Got: {errors:#?}"
    );
}

#[test]
fn imported_conditional_alias_private_extends_alias_chain_binds() {
    // The `extends` operand is itself a private alias chain `RecAlias -> Base`.
    // `HasId extends Base` is true, so `Pick3<HasId>` is `HasId`.
    let files = &[
        (
            "shapes.ts",
            "type Base = { id: string };\n\
             type RecAlias = Base;\n\
             export type Pick3<T> = T extends RecAlias ? T : never;\n",
        ),
        (
            "app.ts",
            "import { Pick3 } from './shapes';\n\
             type HasId = { id: string; extra: number };\n\
             declare const h: Pick3<HasId>;\n\
             const ok: HasId = h;\n",
        ),
    ];
    let errors = assignability_errors(&compile_project(files));
    assert!(
        errors.is_empty(),
        "private alias chain in the extends operand must bind in the declaring arena \
         (Pick3<HasId> = HasId); expected no TS2322. Got: {errors:#?}"
    );
}

#[test]
fn imported_nested_conditional_alias_binds_private_extends() {
    // The exported conditional's true branch is *another* provider-private
    // conditional alias (`Inner`) that also references the private `Leaf`.
    // `Outer<Box>` must resolve to `Box` through both nested conditionals.
    let files = &[
        (
            "nested.ts",
            "type Leaf = object;\n\
             type Inner<U> = U extends Leaf ? U : never;\n\
             export type Outer<T> = T extends Leaf ? Inner<T> : null;\n",
        ),
        (
            "box.ts",
            "import { Outer } from './nested';\n\
             type Box = { v: number };\n\
             declare const b: Outer<Box>;\n\
             const ok: Box = b;\n",
        ),
    ];
    let errors = assignability_errors(&compile_project(files));
    assert!(
        errors.is_empty(),
        "nested provider-private conditional must bind in the declaring arena \
         (Outer<Box> = Box); expected no TS2322. Got: {errors:#?}"
    );
}
