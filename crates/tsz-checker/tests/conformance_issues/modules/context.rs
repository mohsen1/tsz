use crate::core::*;

#[test]
fn imported_interface_method_signature_keeps_alias_and_forward_param_types() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "kysely.ts",
                r#"
export declare class Kysely<DB> {
    readonly db: DB;
}
"#,
            ),
            (
                "dialect-adapter.ts",
                r#"
import { Kysely } from "./kysely";

export interface DialectAdapter {
    acquireMigrationLock(db: Kysely<any>, options: MigrationLockOptions): Promise<void>;
}

export interface MigrationLockOptions {
    readonly lockTable: string;
}
"#,
            ),
            (
                "mysql-adapter.ts",
                r#"
import { Kysely } from "./kysely";
import { MigrationLockOptions } from "./dialect-adapter";

export declare class MysqlAdapter {
    acquireMigrationLock(db: Kysely<any>, _opt: MigrationLockOptions): Promise<void>;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { DialectAdapter } from "./dialect-adapter";
import { MysqlAdapter } from "./mysql-adapter";

const adapter: DialectAdapter = new MysqlAdapter();
adapter;
"#,
            ),
        ],
        "main.ts",
        CheckerOptions::default(),
    );

    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("MysqlAdapter") && message.contains("DialectAdapter")
        }),
        "Imported interface method signatures should resolve imported generic aliases, forward same-file parameter types, and Promise returns without a cascading TS2322. Got: {diagnostics:#?}"
    );
}

#[test]
fn project_forward_generic_class_computed_name_no_false_ts2339() {
    let diagnostics = compile_named_project_get_diagnostics_with_options(
        &[
            (
                "module.ts",
                r#"
export const marker = 0;
"#,
            ),
            (
                "main.ts",
                r#"
import { marker } from "./module";

declare const rC: RC<"a">;
rC.x;
declare class RC<T extends "a" | "b"> {
    x: T;
    [rC.x]: "b";
}

marker;
"#,
            ),
        ],
        CheckerOptions::default(),
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Forward generic class declarations in modules should expose declared instance properties before computed-name evaluation. Got: {diagnostics:#?}"
    );
}

#[test]
fn module_augmentation_of_reexported_interface_applies_to_original_import() {
    for index_source in [
        r#"export * from "./eventList";"#,
        r#"export { EventList } from "./eventList";"#,
    ] {
        let diagnostics = compile_named_files_get_diagnostics_with_options(
            &[
                ("index.ts", index_source),
                (
                    "test.ts",
                    r#"
import { EventList } from "./eventList";

declare const p012: "p0" | "p1" | "p2";
const t: keyof EventList = p012;
"#,
                ),
                (
                    "eventList.ts",
                    r#"
export interface EventList {
    p0: [];
}
"#,
                ),
                (
                    "foo.ts",
                    r#"
declare module "./index" {
    interface EventList {
        p1: [];
    }
}
export {};
"#,
                ),
                (
                    "bar.ts",
                    r#"
declare module "./index" {
    interface EventList {
        p2: [];
    }
}
export {};
"#,
                ),
            ],
            "test.ts",
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        );

        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2322),
            "Expected keyof EventList to include module augmentations from re-exporting module {index_source:?}. Got: {diagnostics:?}"
        );
    }
}

#[test]
#[ignore = "pending: follow-up required"]
fn module_augmentation_enum_merges_value_side_of_reexported_namespace() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "file.ts",
                r#"
export namespace Root {
    export interface Foo {
        x: number;
    }
}
"#,
            ),
            ("reexport.ts", r#"export * from "./file";"#),
            (
                "augment.ts",
                r#"
import * as ns from "./reexport";

declare module "./reexport" {
    export enum Root {
        A,
        B,
        C
    }
}

declare const f: ns.Root.Foo;
const g: ns.Root = ns.Root.A;
f.x;
"#,
            ),
        ],
        "augment.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("Property 'A'")),
        "Expected enum augmentation members on re-exported namespace value to resolve, got: {diagnostics:#?}"
    );
}

#[test]
fn test_js_constructor_branch_property_visible_cross_file() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "foo.js",
                r#"
class C {
    constructor() {
        if (cond) {
            this.p = null;
        } else {
            this.p = 0;
        }
    }
}
"#,
            ),
            (
                "bar.ts",
                r#"
(new C()).p = "string";
"#,
            ),
        ],
        "bar.ts",
        CheckerOptions {
            allow_js: true,
            check_js: false,
            strict: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected the JS constructor branch property to surface as a number property. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339.is_empty(),
        "Did not expect missing-property TS2339 once branch assignments are collected. Actual diagnostics: {diagnostics:#?}"
    );
}

// =============================================================================
// JSX Intrinsic Element Resolution (TS2339)
// =============================================================================

#[test]
fn test_jsx_intrinsic_element_ts2339_for_unknown_tag() {
    // Mirrors tsxElementResolution1.tsx: <span /> should error when only <div> is declared
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: any
    }
}
<div />;
<span />;
"#;
    let diagnostics =
        compile_and_get_diagnostics_named("test.tsx", source, CheckerOptions::default());
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.len() == 1,
        "Expected exactly 1 TS2339 for <span />, got {}: {ts2339_diags:?}",
        ts2339_diags.len()
    );
    assert!(
        ts2339_diags[0].1.contains("span"),
        "Expected TS2339 to mention 'span', got: {}",
        ts2339_diags[0].1
    );
    assert!(
        ts2339_diags[0].1.contains("JSX.IntrinsicElements"),
        "Expected TS2339 to mention 'JSX.IntrinsicElements', got: {}",
        ts2339_diags[0].1
    );
}

#[test]
fn test_jsx_intrinsic_element_no_error_for_known_tag() {
    // Declared tags should not produce TS2339
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: { text?: string; };
        span: any;
    }
}
<div />;
<span />;
"#;
    let diagnostics =
        compile_and_get_diagnostics_named("test.tsx", source, CheckerOptions::default());
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Expected no TS2339 when all tags are declared, got: {ts2339_diags:?}"
    );
}

#[test]
fn test_module_local_jsx_namespace_does_not_satisfy_global_jsx_lookup() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: {
            static?: boolean;
        };
    }
}

export default <div static={true} />;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "index.tsx",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7026),
        "Expected TS7026 because a top-level JSX namespace inside an external module is module-local, got: {diagnostics:#?}"
    );
}

#[test]
fn test_jsx_factory_namespace_reexport_stays_visible_across_namespace_import() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/library.ts",
                r#"
function createElement(element: string, props: any, ...children: any[]): any {}

namespace JSX {
  export interface IntrinsicElements {
    [key: string]: Record<string, any>;
  }
}

export { createElement, JSX };
"#,
            ),
            (
                "/index.tsx",
                r#"
import * as MyLib from "./library";

const content = <my-element/>;
"#,
            ),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::React,
            jsx_factory: "MyLib.createElement".to_string(),
            jsx_factory_from_config: true,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7026),
        "Expected factory-scoped JSX namespace reexport to satisfy JSX lookup without TS7026, got: {diagnostics:#?}"
    );
}

/// Template expressions in switch cases should narrow discriminated unions.
/// Before the fix, template expression case values resolved to `string` instead
/// of the literal `"cat"`, preventing discriminant narrowing and producing
/// false TS2339 errors on narrowed member accesses like `animal.meow`.
#[test]
fn test_template_expression_switch_narrows_discriminated_union() {
    let source = r#"
enum AnimalType {
  cat = "cat",
  dog = "dog",
}

type Animal =
  | { type: `${AnimalType.cat}`; meow: string; }
  | { type: `${AnimalType.dog}`; bark: string; };

function action(animal: Animal) {
  switch (animal.type) {
    case `${AnimalType.cat}`:
      console.log(animal.meow);
      break;
    case `${AnimalType.dog}`:
      console.log(animal.bark);
      break;
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Template expression switch cases should narrow discriminated unions. Got false TS2339: {ts2339_diags:?}"
    );
}

/// Template expressions with multiple substitutions should also produce
/// literal types for narrowing (e.g. `${prefix}${suffix}`).
#[test]
fn test_template_expression_multi_substitution_narrows() {
    let source = r#"
type Tag = "a-1" | "b-2";
type Item =
  | { tag: "a-1"; alpha: string; }
  | { tag: "b-2"; beta: string; };

declare const prefix: "a" | "b";

function check(item: Item) {
  if (item.tag === `a-1`) {
    const x: string = item.alpha;
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339_diags.is_empty(),
        "Simple template literal (no-substitution) should narrow. Got false TS2339: {ts2339_diags:?}"
    );
}

/// Exhaustiveness check: after narrowing all variants via template expression
/// switch cases, the default branch should reach `never`.
#[test]
fn test_template_expression_switch_exhaustiveness_reaches_never() {
    let source = r#"
enum Kind {
  A = "a",
  B = "b",
}

type Variant =
  | { kind: `${Kind.A}`; a: number; }
  | { kind: `${Kind.B}`; b: number; };

function check(p: never) {
  throw new Error("unreachable");
}

function process(v: Variant) {
  switch (v.kind) {
    case `${Kind.A}`:
      return v.a;
    case `${Kind.B}`:
      return v.b;
    default:
      check(v);
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // No TS2339 (member access after narrowing) and no TS2345 (v not assignable to never)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339 || *code == 2345)
        .collect();
    assert!(
        relevant.is_empty(),
        "Template expression switch should exhaust union to never. Got: {relevant:?}"
    );
}

#[test]
fn test_export_equals_default_property_keeps_default_import_on_export_object() {
    let diagnostics = compile_two_files_get_diagnostics_with_options(
        r#"
var x = {
    greeting: "hello, world",
    default: 42
};

export = x;
"#,
        r#"
import foo from "./a";
foo.toExponential(2);

import { default as namedFoo } from "./a";
namedFoo.toExponential(2);
"#,
        "./a",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    let ts2339_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        2,
        "Expected both default-import forms to stay typed as the export= object, not its `default` property. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339_messages.iter().all(|message| message.contains(
            "Property 'toExponential' does not exist on type '{ greeting: string; default: number; }'."
        )),
        "Expected TS2339 to report against the full export= object surface. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn module_preserve_default_import_from_explicit_esm_export_equals_no_ts1192() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("e.mts", "export = 0;"),
            ("main.ts", r#"import e from "./e.mts"; e;"#),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::Preserve,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 1192),
        "module: preserve should use export= as the default import target even for explicit ESM extensions. Got: {diagnostics:#?}"
    );
}

#[test]
fn node_esm_default_import_from_cts_is_namespace_shaped() {
    let diagnostics = compile_node_esm_importing_cts_default_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        !codes.contains(&2339),
        "Default imports from .cts should expose the namespace .default property. Got: {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&2367),
        "Default import aliases from .cts should share the same namespace shape. Got: {diagnostics:#?}"
    );
    let ts2349_count = codes.iter().filter(|&&code| code == 2349).count();
    assert_eq!(
        ts2349_count, 6,
        "Calling the namespace-shaped default bindings should be TS2349, while .default() should be callable. Got: {diagnostics:#?}"
    );
}

// ---------------------------------------------------------------------------
// Multi-file helpers for cross-file type-only export tests
// ---------------------------------------------------------------------------

fn compile_node_esm_importing_cts_default_diagnostics() -> Vec<(u32, String)> {
    let files = [
        (
            "mod.cts",
            r#"
declare function fun(): void;
export default fun;
"#,
        ),
        (
            "b.mts",
            r#"
import a from "./mod.cjs";
import { default as b } from "./mod.cjs";
import c, { default as d } from "./mod.cjs";
import * as self from "./b.mjs";
export { default } from "./mod.cjs";
export { default as def } from "./mod.cjs";

a === b;
b === c;
c === d;
d === self.default;
self.default === self.def;

a();
b();
c();
d();
self.default();
self.def();

a.default();
b.default();
c.default();
d.default();
self.default.default();
self.def.default();
"#,
        ),
    ];

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    for (name, source) in files {
        let mut parser = ParserState::new(name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "b.mts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2022,
            module: ModuleKind::Node16,
            ..CheckerOptions::default()
        },
    );

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./mod.cjs".to_string()), 0);
    resolved_module_paths.insert((1, "./b.mjs".to_string()), 1);
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./mod.cjs".to_string());
    resolved_modules.insert("./b.mjs".to_string());
    let mut file_is_esm_map: FxHashMap<String, bool> = FxHashMap::default();
    file_is_esm_map.insert("mod.cts".to_string(), false);
    file_is_esm_map.insert("b.mts".to_string(), true);

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.file_is_esm = Some(true);
    checker.ctx.file_is_esm_map = Some(Arc::new(file_is_esm_map));
    checker.check_source_file(roots[1]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_react_jsx_runtime_package_root_self_import_duplicates_intrinsic_index_signature() {
    let diagnostics = compile_named_project_get_diagnostics_with_options(
        &[
            ("/file.tsx", "export const a = <div></div>;"),
            (
                "/node_modules/@types/react/package.json",
                r#"
{
  "name": "@types/react",
  "version": "0.0.1",
  "main": "",
  "types": "index.d.ts",
  "exports": {
    "./*.js": "./*.js",
    "./*": "./*.js"
  }
}
"#,
            ),
            (
                "/node_modules/@types/react/index.d.ts",
                r#"
declare namespace JSX {
    interface IntrinsicElements { [x: string]: any; }
}
"#,
            ),
            (
                "/node_modules/@types/react/jsx-runtime.d.ts",
                "import './';",
            ),
            (
                "/node_modules/@types/react/jsx-dev-runtime.d.ts",
                "import './';",
            ),
        ],
        CheckerOptions {
            module: ModuleKind::NodeNext,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::ReactJsx,
            no_lib: true,
            ..Default::default()
        },
    );

    let ts2374: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2374 && message.contains("Duplicate index signature for type 'string'")
        })
        .collect();
    assert_eq!(
        ts2374.len(),
        1,
        "Expected one TS2374 for duplicate JSX.IntrinsicElements string index signature through react/jsx-runtime package-root self import. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_react_jsx_runtime_comment_import_does_not_duplicate_intrinsic_index_signature() {
    let diagnostics = compile_named_project_get_diagnostics_with_options(
        &[
            (
                "/app.tsx",
                r#"
import { jsx } from "react/jsx-runtime";

const node = <custom-tag />;
jsx;
node;
"#,
            ),
            (
                "/node_modules/@types/react/package.json",
                r#"
{
  "name": "@types/react",
  "version": "0.0.1",
  "main": "",
  "types": "index.d.ts",
  "exports": {
    "./*.js": "./*.js",
    "./*": "./*.js"
  }
}
"#,
            ),
            (
                "/node_modules/@types/react/index.d.ts",
                r#"
export {};

export namespace JSX {
  interface IntrinsicElements {
    [elemName: string]: {};
  }
}
"#,
            ),
            (
                "/node_modules/@types/react/jsx-runtime.d.ts",
                r#"
// A comment that looks like import "."; should not merge another JSX namespace.
export function jsx(type: string, props: unknown, key?: string): unknown;

export namespace JSX {
  interface IntrinsicElements {
    "custom-tag": {};
  }
}
"#,
            ),
        ],
        CheckerOptions {
            module: ModuleKind::NodeNext,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::ReactJsx,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().all(|(code, message)| {
            *code != 2374 || !message.contains("Duplicate index signature for type 'string'")
        }),
        "Did not expect TS2374 from a JSX runtime comment-only import. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_buffer_view_uses_lib_default_type_argument_without_ts2314() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        &[(
            "/test.ts",
            r#"
var obj: Object;
if (ArrayBuffer.isView(obj)) {
    var ab: ArrayBufferView = obj;
}
"#,
        )],
        "/test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "Expected ArrayBufferView to use its lib default type argument without TS2314. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_iterable_uses_lib_default_type_arguments_without_ts2314() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        &[(
            "/test.ts",
            r#"
function getEither<T>(in1: Iterable<T>, in2: ArrayLike<T>) {
    return Math.random() > 0.5 ? in1 : in2;
}
"#,
        )],
        "/test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "Expected Iterable to use its lib default type arguments without TS2314. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_literal_bare_uint8array_does_not_poison_later_defaulted_refs() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
type Arg = { data: string | Uint8Array } | { data: number };
declare function foo(arg: Arg): void;
foo({ data: new Uint8Array([30]) });
const x: string | number | Uint8Array = new Uint8Array([30]);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected bare Uint8Array refs inside type literals to preserve lib defaults without TS2322. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_namespace_import_from_umd_module_includes_global_and_module_augmentations() {
    let files = [
        (
            "/a.d.ts",
            r#"
export as namespace a;
export const x = 0;
export const conflict = 0;
"#,
        ),
        (
            "/b.ts",
            r#"
import * as a2 from "./a";

declare global {
    namespace a {
        export const y = 0;
        export const conflict = 0;
    }
}

declare module "./a" {
    export const z = 0;
    export const conflict = 0;
}

a2.x + a2.y + a2.z + a2.conflict;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/b.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected namespace import from UMD module to keep x/y/z/conflict visible without TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_global_namespace_access_includes_module_and_global_augmentations() {
    let files = [
        (
            "/a.d.ts",
            r#"
export as namespace a;
export const x = 0;
export const conflict = 0;
"#,
        ),
        (
            "/b.ts",
            r#"
import * as a2 from "./a";

declare global {
    namespace a {
        export const y = 0;
        export const conflict = 0;
    }
}

declare module "./a" {
    export const z = 0;
    export const conflict = 0;
}

a.x + a.y + a.z + a.conflict;
a2.x;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/b.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            allow_umd_global_access: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected bare UMD global namespace access to keep x/y/z/conflict visible without TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_export_import_type_only_namespace_under_isolated_modules_emits_ts1269() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/jsx.ts",
                r#"
export namespace JSXInternal {
    export type HTMLAttributes = string;
    export type ComponentChildren = string;
}
"#,
            ),
            (
                "/factory.ts",
                r#"
import { JSXInternal } from "./jsx";
export import JSX = JSXInternal;
"#,
            ),
        ],
        "/factory.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2015,
            isolated_modules: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 1269),
        "Expected TS1269 for export import aliasing a type-only namespace under isolatedModules. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_export_import_jsx_namespace_preserves_ambient_namespace_members() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/node_modules/preact/index.d.ts",
                r#"
type Defaultize<Props, Defaults> =
    Props extends any
        ? Partial<Pick<Props, Extract<keyof Props, keyof Defaults>>> &
            Pick<Props, Exclude<keyof Props, keyof Defaults>>
        : never;
export namespace JSXInternal {
    interface HTMLAttributes<T = {}> {}
    interface SVGAttributes<T = {}> {}
    type LibraryManagedAttributes<Component, Props> = Component extends {
        defaultProps: infer Defaults;
    } ? Defaultize<Props, Defaults> : Props;
    interface IntrinsicAttributes { key?: any; }
    interface Element extends VNode<any> {}
    interface ElementClass extends Component<any, any> {}
    interface ElementAttributesProperty { props: any; }
    interface ElementChildrenAttribute { children: any; }
    interface IntrinsicElements { div: HTMLAttributes; }
}
export const Fragment: unique symbol;
export type ComponentType<T = {}> = {};
export type ComponentChild = {};
export type ComponentChildren = {};
export type VNode<T = {}> = {};
export type Attributes = {};
export type Component<T = {}, U = {}> = {};
"#,
            ),
            (
                "/node_modules/preact/jsx-runtime/index.d.ts",
                r#"
export { Fragment } from "..";
import { ComponentType, ComponentChild, ComponentChildren, VNode, Attributes } from "..";
import { JSXInternal } from "..";

export function jsx(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild },
    key?: string
): VNode<any>;
export function jsx<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild },
    key?: string
): VNode<any>;

export function jsxs(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild[] },
    key?: string
): VNode<any>;
export function jsxs<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild[] },
    key?: string
): VNode<any>;

export function jsxDEV(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChildren },
    key?: string
): VNode<any>;
export function jsxDEV<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChildren },
    key?: string
): VNode<any>;

export import JSX = JSXInternal;
"#,
            ),
            ("/index.tsx", r#"export const Comp = () => <div></div>;"#),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::ReactJsx,
            jsx_import_source: "preact".to_string(),
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2694),
        "Expected ambient namespace members inside declaration-file JSXInternal to stay visible through export import JSX aliasing, got: {diagnostics:#?}"
    );
}

#[test]
fn test_declare_global_export_import_jsx_namespace_keeps_global_jsx_surface() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/node_modules/preact/index.d.ts",
                r#"
type Defaultize<Props, Defaults> =
    Props extends any
        ? Partial<Pick<Props, Extract<keyof Props, keyof Defaults>>> &
            Pick<Props, Exclude<keyof Props, keyof Defaults>>
        : never;
export namespace JSXInternal {
    interface HTMLAttributes<T = {}> {}
    interface SVGAttributes<T = {}> {}
    type LibraryManagedAttributes<Component, Props> = Component extends {
        defaultProps: infer Defaults;
    } ? Defaultize<Props, Defaults> : Props;
    interface IntrinsicAttributes { key?: any; }
    interface Element extends VNode<any> {}
    interface ElementClass extends Component<any, any> {}
    interface ElementAttributesProperty { props: any; }
    interface ElementChildrenAttribute { children: any; }
    interface IntrinsicElements { div: HTMLAttributes; }
}
export const Fragment: unique symbol;
export type ComponentType<T = {}> = {};
export type ComponentChild = {};
export type ComponentChildren = {};
export type VNode<T = {}> = {};
export type Attributes = {};
export type Component<T = {}, U = {}> = {};
"#,
            ),
            (
                "/node_modules/preact/jsx-runtime/index.d.ts",
                r#"
export { Fragment } from "..";
import { ComponentType, ComponentChild, ComponentChildren, VNode, Attributes } from "..";
import { JSXInternal } from "..";

export function jsx(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild },
    key?: string
): VNode<any>;
export function jsx<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild },
    key?: string
): VNode<any>;

export function jsxs(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild[] },
    key?: string
): VNode<any>;
export function jsxs<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild[] },
    key?: string
): VNode<any>;

export function jsxDEV(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChildren },
    key?: string
): VNode<any>;
export function jsxDEV<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChildren },
    key?: string
): VNode<any>;

declare global {
    export import JSX = JSXInternal;
}
"#,
            ),
            ("/index.tsx", r#"export const Comp = () => <div></div>;"#),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::Preserve,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Expected declare global export import JSX alias to be accepted and provide JSX globals without bogus TS2666/TS2694/TS7026 diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn test_declare_global_export_import_missing_alias_target_keeps_jsx_missing_interface_error() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/node_modules/preact/index.d.ts",
                r#"
type Defaultize<Props, Defaults> =
    Props extends any
        ? Partial<Pick<Props, Extract<keyof Props, keyof Defaults>>> &
            Pick<Props, Exclude<keyof Props, keyof Defaults>>
        : never;
export namespace JSXInternal {
    interface HTMLAttributes<T = {}> {}
    interface SVGAttributes<T = {}> {}
    type LibraryManagedAttributes<Component, Props> = Component extends {
        defaultProps: infer Defaults;
    } ? Defaultize<Props, Defaults> : Props;
    interface IntrinsicAttributes { key?: any; }
    interface Element extends VNode<any> {}
    interface ElementClass extends Component<any, any> {}
    interface ElementAttributesProperty { props: any; }
    interface ElementChildrenAttribute { children: any; }
    interface IntrinsicElements { div: HTMLAttributes; }
}
export const Fragment: unique symbol;
export type ComponentType<T = {}> = {};
export type ComponentChild = {};
export type ComponentChildren = {};
export type VNode<T = {}> = {};
export type Attributes = {};
export type Component<T = {}, U = {}> = {};
"#,
            ),
            (
                "/node_modules/preact/jsx-runtime/index.d.ts",
                r#"
export { Fragment } from "..";
import { ComponentType, ComponentChild, ComponentChildren, VNode, Attributes } from "..";
import { JSXInternal } from "..";

export function jsx(
    type: string,
    props: JSXInternal.HTMLAttributes &
        JSXInternal.SVGAttributes &
        Record<string, any> & { children?: ComponentChild },
    key?: string
): VNode<any>;
export function jsx<P>(
    type: ComponentType<P>,
    props: Attributes & P & { children?: ComponentChild },
    key?: string
): VNode<any>;

declare global {
    export import JSX = NotFound;
}
"#,
            ),
            ("/index.tsx", r#"export const Comp = () => <div></div>;"#),
        ],
        "/index.tsx",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            jsx_mode: JsxMode::Preserve,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7026),
        "Expected TS7026 when missing global JSX alias target leaves IntrinsicElements unavailable in the local harness, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2666 || *code == 2694),
        "Expected missing alias target case to avoid bogus augmentation-export and namespace-member diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn test_imported_declaration_file_with_top_level_declare_global_still_emits_ts2306() {
    let mut parser_entry = ParserState::new(
        "/src/index.ts".to_string(),
        r#"
import {} from "./react";
export const x = 1;
"#
        .to_string(),
    );
    let root_entry = parser_entry.parse_source_file();
    let mut binder_entry = BinderState::new();
    binder_entry.bind_source_file(parser_entry.get_arena(), root_entry);

    let mut parser_react = ParserState::new(
        "/src/react.d.ts".to_string(),
        "declare global {}".to_string(),
    );
    let root_react = parser_react.parse_source_file();
    let mut binder_react = BinderState::new();
    binder_react.bind_source_file(parser_react.get_arena(), root_react);

    let arena_entry = Arc::new(parser_entry.get_arena().clone());
    let arena_react = Arc::new(parser_react.get_arena().clone());
    let binder_entry = Arc::new(binder_entry);
    let binder_react = Arc::new(binder_react);
    let all_arenas = Arc::new(vec![Arc::clone(&arena_entry), Arc::clone(&arena_react)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_entry), Arc::clone(&binder_react)]);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((0, "./react".to_string()), 1);
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./react".to_string());

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_entry.as_ref(),
        binder_entry.as_ref(),
        &types,
        "/src/index.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root_entry);
    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2306),
        "Expected imported declaration file with top-level declare global to still report TS2306. Actual diagnostics: {diagnostics:#?}"
    );
}
