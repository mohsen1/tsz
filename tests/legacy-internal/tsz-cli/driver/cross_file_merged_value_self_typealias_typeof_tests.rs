//! Project-mode coverage for a value merged with a self-referential type alias
//! (`const X = <literal>; type X = typeof X`) used in a *consuming* file.
//!
//! The merged symbol stores the self-referential `TypeQuery(X)` as its
//! type-space body. The declaring file registers `X`'s value-space type while
//! checking the alias, but a consuming file's per-file `type_env` is reset, so
//! the deferred `TypeQuery(X)` self-loops in `resolve_type_query` and every
//! relation against the reference fails: a false `TS2344` when the reference is
//! a generic argument (the original ts-pattern `anonymousSelectKey` row, arch
//! #8225) and a false `TS2322` when it is the source of an assignment. These run
//! the full project driver (shared `DefinitionStore`, every file checked) so the
//! per-file reset that triggers the self-loop is exercised — the simplified
//! single-context checker harness does not reproduce it. See #15078.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS2344: u32 = 2344;
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
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2015", "noEmit": true }}, "files": [{}] }}"#,
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

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

const SYMBOLS: (&str, &str) = (
    "symbols.ts",
    "export const anonymousSelectKey = '@ts-pattern/anonymous-select-key';\n\
     export type anonymousSelectKey = typeof anonymousSelectKey;\n",
);

/// The ts-pattern repro: a string-literal marker merged with its self-`typeof`
/// alias satisfies a `string` constraint across files (no false TS2344).
#[test]
fn string_marker_satisfies_string_constraint_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "patterns.ts",
            "import { anonymousSelectKey } from './symbols';\n\
             type SelectP<key extends string> = key;\n\
             export type Bad = SelectP<anonymousSelectKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "string marker should satisfy the `string` constraint cross-file, got: {diags:?}"
    );
}

/// Renamed import (`as`) keeps the value-space resolution working.
#[test]
fn renamed_marker_satisfies_constraint_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "patterns.ts",
            "import { anonymousSelectKey as Key } from './symbols';\n\
             type SelectP<key extends string> = key;\n\
             export type Bad = SelectP<Key>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "renamed marker should satisfy the constraint, got: {diags:?}"
    );
}

/// Negative control: a numeric marker must NOT satisfy a `string` constraint —
/// the value side is genuinely checked, not blindly accepted.
#[test]
fn number_marker_violates_string_constraint_cross_file() {
    let diags = compile_project(&[
        (
            "nsym.ts",
            "export const numKey = 42;\nexport type numKey = typeof numKey;\n",
        ),
        (
            "nmain.ts",
            "import { numKey } from './nsym';\n\
             type NeedsString<k extends string> = k;\n\
             export type Bad = NeedsString<numKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        1,
        "numeric marker must violate the `string` constraint, got: {diags:?}"
    );
}

/// The same numeric marker satisfies a `number` constraint (value side accepted).
#[test]
fn number_marker_satisfies_number_constraint_cross_file() {
    let diags = compile_project(&[
        (
            "nsym.ts",
            "export const numKey = 42;\nexport type numKey = typeof numKey;\n",
        ),
        (
            "nmain.ts",
            "import { numKey } from './nsym';\n\
             type NeedsNumber<k extends number> = k;\n\
             export type Ok = NeedsNumber<numKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "numeric marker should satisfy the `number` constraint, got: {diags:?}"
    );
}

/// Assignment position: the marker's literal is assignable to `string`, so the
/// previously-deferred self-loop no longer produces a false assignment error.
#[test]
fn marker_assignable_to_string_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "use.ts",
            "import { anonymousSelectKey } from './symbols';\n\
             export const ok: string = null as unknown as anonymousSelectKey;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        0,
        "string marker should be assignable to `string`, got: {diags:?}"
    );
}

/// fp-ts-style HKT defunctionalization: a concrete instance object assigned to
/// an interface whose generic method signatures are typed in `Kind2<F, E, _>`
/// must instantiate `F` through the `typeof URI` value-space literal.
#[test]
fn hkt_instance_assignable_through_typeof_uri_tag_project() {
    let diags = compile_project(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind2<E, A> {}
export type URIS2 = keyof URItoKind2<any, any>;
export type Kind2<F extends URIS2, E, A> = F extends URIS2 ? URItoKind2<E, A>[F] : never;
export interface Functor2<F extends URIS2> {
    readonly map: <E, A, B>(fa: Kind2<F, E, A>, f: (a: A) => B) => Kind2<F, E, B>;
}
"#,
        ),
        (
            "IOEither.ts",
            r#"
import { Functor2, URItoKind2 } from "./HKT";
declare module "./HKT" {
    interface URItoKind2<E, A> {
        readonly IOEither: IOEither<E, A>;
    }
}
export interface IOEither<E, A> { (): E | A }
export const URI = "IOEither";
export type URI = typeof URI;

declare const ioMap: <E, A, B>(fa: IOEither<E, A>, f: (a: A) => B) => IOEither<E, B>;

export const Functor: Functor2<URI> = {
    map: ioMap,
};
"#,
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        0,
        "HKT instance through `typeof URI` tag should be assignable, got: {diags:?}"
    );
}

/// Anti-hardcoding: renamed registry, tag, alias, and member names still obey
/// the same value-space `typeof` rule.
#[test]
fn hkt_instance_assignability_is_binder_name_independent_project() {
    let diags = compile_project(&[
        (
            "registry.ts",
            r#"
export interface Slots2<E, A> {}
export type Tags2 = keyof Slots2<any, any>;
export type Pick2<G extends Tags2, E, A> = G extends Tags2 ? Slots2<E, A>[G] : never;
export interface Mapper2<G extends Tags2> {
    readonly transform: <E, A, B>(fa: Pick2<G, E, A>, f: (a: A) => B) => Pick2<G, E, B>;
}
"#,
        ),
        (
            "widget.ts",
            r#"
import { Mapper2, Slots2 } from "./registry";
declare module "./registry" {
    interface Slots2<E, A> {
        readonly Widget: Widget<E, A>;
    }
}
export interface Widget<E, A> { (): E | A }
export const TAG = "Widget";
export type TAG = typeof TAG;

declare const widgetMap: <E, A, B>(fa: Widget<E, A>, f: (a: A) => B) => Widget<E, B>;

export const Mapper: Mapper2<TAG> = {
    transform: widgetMap,
};
"#,
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        0,
        "renamed-binder HKT registry instance should be assignable, got: {diags:?}"
    );
}

/// Direct indexed-access lookup: `URItoKind2<E, A>[typeof URI]` must use the
/// value-space literal key and find the augmented registry member.
#[test]
fn typeof_uri_tag_indexes_augmented_registry_member_project() {
    let diags = compile_project(&[
        (
            "HKT.ts",
            r#"
export interface URItoKind2<E, A> {}
"#,
        ),
        (
            "Either.ts",
            r#"
import { URItoKind2 } from "./HKT";
declare module "./HKT" {
    interface URItoKind2<E, A> {
        readonly Either: { left: E; right: A };
    }
}
export const URI = "Either";
export type URI = typeof URI;

export const cell: URItoKind2<string, number>[URI] = { left: "e", right: 1 };
"#,
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        0,
        "augmented member indexed by the `typeof URI` tag should accept the value, got: {diags:?}"
    );
}

/// The tag remains its exact literal, not widened to `string`.
#[test]
fn typeof_uri_tag_preserves_literal_narrowing_project() {
    let diags = compile_project(&[(
        "tag.ts",
        r#"
const URI = "IOEither";
type URI = typeof URI;

export const bad: URI = "Other";
"#,
    )]);
    assert_eq!(
        count_code(&diags, TS2322),
        1,
        "non-matching literal assigned to `type URI = typeof URI` should be rejected, got: {diags:?}"
    );
}

/// Negative control: once the HKT tag resolves, return-position mismatch is
/// still rejected instead of being hidden behind `any`.
#[test]
fn hkt_functor2_wrapped_renamed_instance_rejects_incompatible_map_project() {
    let diags = compile_project(&[
        (
            "registry.ts",
            r#"
export interface Slots2<Env, Item> {}
export type Tags2 = keyof Slots2<any, any>;
export type Select2<Token extends Tags2, Env, Item> =
    Token extends Tags2 ? Slots2<Env, Item>[Token] : never;
export type Wrapped2<Token extends Tags2, Env, Item> = {
    readonly value: Select2<Token, Env, Item>;
};
export interface Functor2<Token extends Tags2> {
    readonly map: <Env, Input, Output>(
        fa: Wrapped2<Token, Env, Input>,
        f: (value: Input) => Output,
    ) => Wrapped2<Token, Env, Output>;
}
"#,
        ),
        (
            "packet.ts",
            r#"
import { Functor2, Slots2, Wrapped2 } from "./registry";
declare module "./registry" {
    interface Slots2<Env, Item> {
        readonly Packet: Packet<Env, Item>;
    }
}
export interface Packet<Err, Value> {
    readonly run: () => Err | Value;
}
export const TOKEN = "Packet";
export type TOKEN = typeof TOKEN;

declare const badMap: <Env, Input, Output>(
    fa: Wrapped2<TOKEN, Env, Input>,
    f: (value: Input) => Output,
) => Wrapped2<TOKEN, Env, Input>;

export const badFunctor: Functor2<TOKEN> = {
    map: badMap,
};
"#,
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        1,
        "incompatible wrapped HKT map implementation should produce one TS2322, got: {diags:?}"
    );
}
