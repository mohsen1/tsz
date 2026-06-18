//! Project-mode coverage: an imported generic type alias whose body references
//! a **provider-private** (non-exported) named type, in body shapes beyond the
//! narrow top-level-`extends` conditional handled by
//! `cross_file_conditional_alias_private_extends_tests`.
//!
//! Witnesses (issue #13618 family — the `ts-essentials` `DeepPick` micro-bench
//! and its reductions), all reproducible with embedded behavior, binder/file
//! names varied for the anti-hardcoding discipline:
//!
//! ```ts
//! // provider.ts — `Wrapper`/`Keep` are NOT exported
//! type Wrapper<X> = { wrapped: X };
//! export type Unwrap<T> = T extends Wrapper<infer U> ? U : T;   // parameterized extends
//! type Keep = "id";
//! export type PickId<T> = { [K in keyof T as K extends Keep ? K : never]: T[K] }; // mapped
//! ```
//!
//! Structural rule: a type reference inside an exported alias body resolves in
//! the alias's *declaring* module — `tsc` resolves it where it textually
//! appears, never in the importer's scope. When that reference names a
//! provider-private (non-exported) type, the consumer scope cannot bind it, so
//! re-lowering the body in the consumer arena leaves it an `UnresolvedTypeName`:
//! the conditional takes the wrong branch and the mapped key filter / value
//! never settles, producing spurious `TS2322`/`TS2741`/`TS2353`. The fix
//! delegates resolution to the declaring arena whenever the body references a
//! provider-private named type, independent of body shape — generalizing the
//! conditional-only gate that previously handled only `T extends Ref ? …` with a
//! bare `Ref`.
//!
//! The gate stays additive and JSX-safe: it fires only for **non-exported**
//! provider-local references, which a library's public conditional/mapped helper
//! (always exported, or referencing only the caller's type parameter / globals)
//! never matches.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// "X is not assignable to Y" assignability false-positive family.
const TS2322: u32 = 2322;
/// "Property X is missing in type Y" — the other half of the #13618 family.
const TS2741: u32 = 2741;

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

fn codes(diags: &[Diagnostic], wanted: &[u32]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| wanted.contains(&d.code))
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn imported_conditional_alias_parameterized_extends_binds_private_ref() {
    // `T extends Wrapper<infer U> ? U : T` — the `extends` operand is a
    // *parameterized* reference to the provider-private generic `Wrapper`. tsc:
    // `Unwrap<{ wrapped: number }>` is `number` (true branch). Before the fix the
    // consumer could not bind `Wrapper`, took the false branch, and resolved to
    // `{ wrapped: number }` — so `const ok: number = w` reported a false TS2322.
    let files = &[
        (
            "provider.ts",
            "type Wrapper<X> = { wrapped: X };\n\
             export type Unwrap<T> = T extends Wrapper<infer U> ? U : T;\n",
        ),
        (
            "use.ts",
            "import { Unwrap } from './provider';\n\
             declare const w: Unwrap<{ wrapped: number }>;\n\
             const ok: number = w;\n",
        ),
    ];
    let errors = codes(&compile_project(files), &[TS2322, TS2741]);
    assert!(
        errors.is_empty(),
        "Unwrap<{{ wrapped: number }}> must take the true branch (= number) by binding the \
         provider-private `Wrapper`; expected no TS2322/TS2741. Got: {errors:#?}"
    );
}

#[test]
fn imported_conditional_alias_parameterized_extends_is_not_any_or_error() {
    // Negative half: the resolved type is concretely `number`, not `any`/`error`
    // (which would silence every assignment). A `string` target must still fail.
    let files = &[
        (
            "provider.ts",
            "type Wrapper<X> = { wrapped: X };\n\
             export type Unwrap<T> = T extends Wrapper<infer U> ? U : T;\n",
        ),
        (
            "use.ts",
            "import { Unwrap } from './provider';\n\
             declare const w: Unwrap<{ wrapped: number }>;\n\
             const bad: string = w;\n",
        ),
    ];
    let errors = codes(&compile_project(files), &[TS2322]);
    assert!(
        !errors.is_empty(),
        "Unwrap<{{ wrapped: number }}> resolves to the concrete `number`, so assigning it to \
         `string` must still report TS2322 (proves it is not silenced to any/error). \
         Got: {errors:#?}"
    );
}

// NOTE: a pure *mapped*-body alias whose key filter references a
// provider-private type (`{ [K in keyof T as K extends Keep ? K : never]: … }`)
// is intentionally NOT covered here. Delegation binds the name, but the `as`
// filter's reference is re-resolved at mapped-evaluation time by a resolver that
// cannot bind the cross-arena name — the deeper cross-arena member/name
// resolution residual tracked by #13044/#13484, which PR #13706 already noted is
// not closed by delegation. An imported mapped alias whose filter uses an inline
// literal (`K extends 'id' ? …`) already reduces correctly cross-module, so the
// gap is specifically that residual, not mapped evaluation in general.

#[test]
fn recursive_deep_pick_with_never_leaf_binds_private_builtin() {
    // The full ts-essentials DeepPick shape: recursive, with a private `Builtin`
    // guard, array recursion, and a mapped type whose value uses a `never`-leaf
    // marker. Cross-module, this must resolve `DeepPick<User, { id: never }>` to
    // `{ id: string }` (not `never`, not the whole `User`).
    let files = &[
        (
            "essentials.ts",
            "type Builtin = string | number | boolean | bigint | symbol | null | undefined;\n\
             export type DeepPick<Source, Spec> = Source extends Builtin\n\
               ? Source\n\
               : Source extends ReadonlyArray<infer Elem>\n\
                 ? ReadonlyArray<DeepPick<Elem, Spec>>\n\
                 : { [Key in keyof Source as Key extends keyof Spec ? Key : never]:\n\
                       Spec[Key] extends never ? Source[Key] : DeepPick<Source[Key], Spec[Key]> };\n",
        ),
        (
            "app.ts",
            "import { DeepPick } from './essentials';\n\
             type User = { id: string; right: 'read' | 'readwrite' };\n\
             type Spec = { id: never };\n\
             type Org = { admin: User; engineers: User[] };\n\
             type OrgSpec = { admin: Spec; engineers: Spec };\n\
             const rights: DeepPick<Org, OrgSpec> = {\n\
               admin: { id: 'admin_id' },\n\
               engineers: [{ id: 'engineer_id' }],\n\
             };\n\
             const leaf: DeepPick<User, Spec> = { id: 'x' };\n",
        ),
    ];
    let errors = codes(&compile_project(files), &[TS2322, TS2741]);
    assert!(
        errors.is_empty(),
        "DeepPick must resolve the never-leaf picked members to `{{ id: string }}` across \
         modules; expected no TS2322/TS2741. Got: {errors:#?}"
    );
}

#[test]
fn imported_alias_referencing_only_type_parameter_is_unaffected() {
    // Safety / no-over-delegation control: a library-helper-shaped alias whose
    // body references ONLY the caller's type parameter (no provider-private
    // name) must keep its existing consumer-arena behavior and resolve correctly.
    // `Decorate<{ a: 1 }>` is `{ a: 1 } & { tagged: true }`.
    let files = &[
        (
            "lib.ts",
            "export type Decorate<P> = P & { tagged: true };\n",
        ),
        (
            "consumer.ts",
            "import { Decorate } from './lib';\n\
             declare const d: Decorate<{ a: 1 }>;\n\
             const a: number = d.a;\n\
             const t: true = d.tagged;\n",
        ),
    ];
    let errors = codes(&compile_project(files), &[TS2322, TS2741]);
    assert!(
        errors.is_empty(),
        "an alias referencing only the caller's type parameter must resolve normally \
         (no spurious diagnostics from delegation changes). Got: {errors:#?}"
    );
}
