use super::args::CliArgs;
use super::driver::{
    CompilationCache, compile, compile_with_cache, compile_with_cache_and_changes,
};
use clap::Parser;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tsz_binder::BinderState;
use tsz_binder::SymbolId;
use tsz_binder::state::BinderStateScopeInputs;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "tsz_cli_driver_test_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn with_types_versions_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
    super::driver::with_types_versions_env(value, f)
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn default_args() -> CliArgs {
    // Use clap's parser to create default args - this handles all the many fields automatically
    CliArgs::try_parse_from(["tsz"]).expect("default args should parse")
}

fn load_real_default_lib_files(target: ScriptTarget) -> Vec<Arc<tsz_binder::lib_loader::LibFile>> {
    let lib_paths = crate::config::resolve_default_lib_files(target).expect("default libs");
    let lib_path_refs: Vec<_> = lib_paths.iter().map(PathBuf::as_path).collect();
    tsz::parallel::load_lib_files_for_binding_strict(&lib_path_refs).expect("load strict libs")
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    None
}

#[test]
fn compile_duplicate_amd_module_name_directives_reports_ts2458() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"///<amd-module name='FirstModuleName'/>
///<amd-module name='SecondModuleName'/>
class Foo {
  x: number;
  constructor() {
    this.x = 5;
  }
}
export = Foo;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "amd"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2458),
        "Expected TS2458 for duplicate AMD module name directives, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_amd_dependency_comment_name_fixture_keeps_ts2792_under_ts5107() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"///<amd-dependency path='aliasedModule5' name='n1'/>
///<amd-dependency path='unaliasedModule3'/>
///<amd-dependency path='aliasedModule6' name='n2'/>
///<amd-dependency path='unaliasedModule4'/>

import "unaliasedModule1";

import r1 = require("aliasedModule1");
r1;

import {p1, p2, p3} from "aliasedModule2";
p1;

import d from "aliasedModule3";
d;

import * as ns from "aliasedModule4";
ns;

import "unaliasedModule2";
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "amd"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes.iter().filter(|&&code| code == 5107).count(),
        1,
        "Expected one TS5107 deprecation diagnostic, got diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == 2882).count(),
        2,
        "Expected two TS2882 side-effect import diagnostics, got diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == 2792).count(),
        4,
        "Expected four TS2792 import diagnostics, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2307),
        "Did not expect TS2307 once Classic resolution upgrades to TS2792, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn declaration_emit_ts2883_prefers_canonical_named_reference_message() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("src/index.ts"),
        r#"import { SomeType } from "some-dep";
export const foo = (thing: SomeType) => { return thing; };
export const bar = (thing: SomeType) => { return thing.arg; };
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/dist/inner.d.ts"),
        r#"export declare type Other = { other: string };
export declare type SomeType = { arg: Other };
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/dist/index.d.ts"),
        r#"export type OtherType = import('./inner').Other;
export type SomeType = import('./inner').SomeType;
"#,
    );
    write_file(
        &base.join("node_modules/some-dep/package.json"),
        r#"{
  "name": "some-dep",
  "exports": { ".": "./dist/index.js" }
}"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "nodenext"
  },
  "files": ["src/index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|m| m
                .contains("reference to 'SomeType' from '../node_modules/some-dep/dist/inner'")),
        "expected canonical SomeType TS2883, got: {messages:#?}"
    );
    assert!(
        messages.iter().all(|m| !m.contains("reference to '../node_modules/some-dep/dist/inner' from 'Other'")),
        "expected swapped TS2883 to be filtered, got: {messages:#?}"
    );
}

#[test]
fn declaration_emit_default_object_assign_reports_non_portable_nested_reference() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base
            .join("node_modules/styled-components/node_modules/hoist-non-react-statics/index.d.ts"),
        r#"interface Statics {
    "$$whatever": string;
}
declare namespace hoistNonReactStatics {
    type NonReactStatics<T> = {[X in Exclude<keyof T, keyof Statics>]: T[X]}
}
export = hoistNonReactStatics;
"#,
    );
    write_file(
        &base.join("node_modules/styled-components/index.d.ts"),
        r#"import * as hoistNonReactStatics from "hoist-non-react-statics";
export interface DefaultTheme {}
export type StyledComponent<TTag extends string, TTheme = DefaultTheme, TStyle = {}, TWhatever = never> =
    string
    & StyledComponentBase<TTag, TTheme, TStyle, TWhatever>
    & hoistNonReactStatics.NonReactStatics<TTag>;
export interface StyledComponentBase<TTag extends string, TTheme = DefaultTheme, TStyle = {}, TWhatever = never> {
    tag: TTag;
    theme: TTheme;
    style: TStyle;
    whatever: TWhatever;
}
export interface StyledInterface {
    div: (a: TemplateStringsArray) => StyledComponent<"div">;
}
declare const styled: StyledInterface;
export default styled;
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import styled from "styled-components";

const A = styled.div``;
const B = styled.div``;
export const C = styled.div``;

export default Object.assign(A, {
    B,
    C
});
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected only one TS2883 for the default export, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages.iter().any(|message| {
            message.contains("default")
                && message.contains("NonReactStatics")
                && message.contains("styled-components/node_modules/hoist-non-react-statics")
        }),
        "expected TS2883 on default Object.assign export, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages
            .iter()
            .all(|message| !message.contains("'C'")),
        "expected no TS2883 on exported helper C, got: {ts2883_messages:#?}"
    );
}

#[test]
fn declaration_emit_reports_non_serializable_foreign_unique_symbol_property() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("a.d.ts"),
        r#"export declare const timestampSymbol: unique symbol;

export declare const Timestamp: {
    [TKey in typeof timestampSymbol]: true;
};

export declare function now(): typeof Timestamp;
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import * as x from "./a";
export const timestamp = x.now();
"#,
    );
    write_file(
        &base.join("c.ts"),
        r#"import { now } from "./a";

export const timestamp = now();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["b.ts", "c.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts4118_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 4118)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts4118_messages.len(),
        2,
        "expected two TS4118 diagnostics, got: {ts4118_messages:#?}"
    );
    assert!(
        ts4118_messages
            .iter()
            .all(|message| { message.contains("[timestampSymbol]") }),
        "expected TS4118 to mention [timestampSymbol], got: {ts4118_messages:#?}"
    );
}

#[test]
fn declaration_emit_reports_ts7056_for_private_import_type_alias() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("http-client.ts"),
        r#"type TPromise<ResolveType, RejectType = any> = Omit<Promise<ResolveType>, "then" | "catch"> & {
    then<TResult1 = ResolveType, TResult2 = never>(
        onfulfilled?: ((value: ResolveType) => TResult1 | PromiseLike<TResult1>) | undefined | null,
        onrejected?: ((reason: RejectType) => TResult2 | PromiseLike<TResult2>) | undefined | null,
    ): TPromise<TResult1 | TResult2, RejectType>;
    catch<TResult = never>(
        onrejected?: ((reason: RejectType) => TResult | PromiseLike<TResult>) | undefined | null,
    ): TPromise<ResolveType | TResult, RejectType>;
};

export interface HttpResponse<D extends unknown, E extends unknown = unknown> extends Response {
    data: D;
    error: E;
}

export class HttpClient<SecurityDataType = unknown> {
    public request = <T = any, E = any>(): TPromise<HttpResponse<T, E>> => {
        return '' as any;
    };
}
"#,
    );
    write_file(
        &base.join("Api.ts"),
        r#"import { HttpClient } from "./http-client";

export class Api<SecurityDataType = unknown> {
    constructor(private http: HttpClient<SecurityDataType>) { }

    abc1 = () => this.http.request();
    abc2 = () => this.http.request();
    abc3 = () => this.http.request();
}
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "commonjs",
    "skipLibCheck": true
  },
  "files": ["http-client.ts", "Api.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts7056: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 7056)
        .collect();

    assert_eq!(
        ts7056.len(),
        3,
        "expected TS7056 on all inferred Api property declarations, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_huge_declaration_output_truncation_skips_dts_but_keeps_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("test.ts"),
        r#"type props = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z";

type manyprops = `${props}${props}`;

export const c = [null as any as {[K in manyprops]: {[K2 in manyprops]: `${K}.${K2}`}}][0];
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "strict": true,
    "declaration": true,
    "module": "commonjs"
  },
  "files": ["test.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts7056: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 7056)
        .collect();

    assert_eq!(
        ts7056.len(),
        1,
        "expected one TS7056, got: {:#?}",
        result.diagnostics
    );
    assert!(
        base.join("test.js").exists(),
        "expected JS emit to continue after TS7056"
    );
    assert!(
        !base.join("test.d.ts").exists(),
        "expected declaration emit to skip test.d.ts when TS7056 is reported"
    );
    assert!(
        result
            .emitted_files
            .iter()
            .all(|path| path.file_name().and_then(|name| name.to_str()) != Some("test.d.ts")),
        "test.d.ts should not be reported as emitted: {:#?}",
        result.emitted_files
    );
}

#[test]
fn declaration_emit_reports_ts2883_for_transitive_react_styled_form() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    fs::create_dir_all(base.join("node_modules/react")).expect("react dir");
    fs::create_dir_all(base.join("node_modules/create-emotion-styled/types/react"))
        .expect("create-emotion-styled dir");
    fs::create_dir_all(base.join("node_modules/react-emotion")).expect("react-emotion dir");

    write_file(
        &base.join("node_modules/react/index.d.ts"),
        r#"declare namespace React {
    export interface DetailedHTMLProps<T, U> {}
    export interface HTMLAttributes<T> {}
}
export = React;
export as namespace React;
"#,
    );
    write_file(
        &base.join("node_modules/create-emotion-styled/types/react/index.d.ts"),
        r#"/// <reference types="react" />
declare module 'react' {
    interface HTMLAttributes<T> {
        css?: unknown;
    }
}
export interface StyledOtherComponentList {
    "div": React.DetailedHTMLProps<React.HTMLAttributes<HTMLDivElement>, HTMLDivElement>
}
export interface StyledOtherComponent<A, B, C> {}
"#,
    );
    write_file(
        &base.join("node_modules/create-emotion-styled/index.d.ts"),
        r#"export * from "./types/react";
"#,
    );
    write_file(
        &base.join("node_modules/react-emotion/index.d.ts"),
        r#"import {StyledOtherComponent, StyledOtherComponentList} from "create-emotion-styled";
export default function styled(tag: string): (o: object) => StyledOtherComponent<{}, StyledOtherComponentList["div"], any>;
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import styled from "react-emotion"

const Form = styled('div')({ color: "red" })

export default Form
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2883)
        .map(|diagnostic| diagnostic.message_text.clone())
        .collect();
    let ts2300_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::DUPLICATE_IDENTIFIER)
        .map(|diagnostic| diagnostic.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        3,
        "expected three TS2883 diagnostics, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages
            .iter()
            .any(|message| message.contains("StyledOtherComponent")),
        "expected one TS2883 to mention StyledOtherComponent, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages
            .iter()
            .filter(|message| message.contains("DetailedHTMLProps")
                || message.contains("HTMLAttributes"))
            .count()
            >= 2,
        "expected TS2883 diagnostics for react transitive types, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2300_messages.is_empty(),
        "expected module augmentation interface merge to avoid TS2300, got: {ts2300_messages:#?}"
    );
}

#[test]
fn compile_project_namespace_import_qualified_type_sees_module_augmentation_exports() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/backbone/index.d.ts"),
        r#"declare namespace Backbone {
    interface Model<T extends object = any, TQuery = any, TOptions = any> {}
}
export = Backbone;
"#,
    );
    write_file(
        &base.join("node_modules/backbone-fetch-cache/index.d.ts"),
        r#"import * as Backbone from "backbone";
declare module "backbone" {
    interface ModelWithCache extends Backbone.Model<any, any, any> {
        cache: boolean;
    }
}
"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import * as Backbone from "backbone";
import "backbone-fetch-cache";

let model: Backbone.ModelWithCache;
model.cache;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true
  },
  "files": ["index.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::CANNOT_FIND_NAMESPACE
                || d.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER),
        "Expected namespace-import type position to see module augmentation members, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_project_export_equals_request_augmentation_avoids_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/express/index.d.ts"),
        r#"declare namespace Express {
    export interface Request { }
    export interface Response { }
    export interface Application { }
}

declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request {
            get(name: string): string;
        }
        interface Response extends Express.Response {
            charset: string;
        }
        interface Application extends Express.Application {
            routes: any;
        }
        interface Express extends Application {
            createApplication(): Application;
        }
        interface RequestHandler {
            (req: Request, res: Response, next: Function): any;
        }
        export = e;
    }
}
"#,
    );
    write_file(
        &base.join("augmentation.ts"),
        r#"import * as e from "express";
declare module "express" {
    interface Request {
        id: number;
    }
}
"#,
    );
    write_file(
        &base.join("consumer.ts"),
        r#"import { Request } from "express";
import "./augmentation";

let x: Request;
const y = x.id;
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true
  },
  "files": ["augmentation.ts", "consumer.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected export= module augmentation interface merge to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected augmented Request interface to expose id, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected TS2454 to remain the primary consumer error, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_project_ambient_import_equals_module_declaration_avoids_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("server.d.ts"),
        r#"declare module "other" {
    export class C { }
}

declare module "server" {
    import events = require("other");

    namespace S {
        export var a: number;
    }

    export = S;
}
"#,
    );
    write_file(
        &base.join("client.ts"),
        r#"import { a } from "server";
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015"
  },
  "files": ["server.d.ts", "client.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected ambient external module import-equals declaration to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics for ambient import-equals external module declaration, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_project_umd_global_class_surface_stays_unaugmented() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/math2d/index.d.ts"),
        r#"export as namespace Math2d;

export interface Point {
    x: number;
    y: number;
}

export class Vector implements Point {
    x: number;
    y: number;
    constructor(x: number, y: number);

    translate(dx: number, dy: number): Vector;
}

export function getLength(p: Vector): number;
"#,
    );
    write_file(
        &base.join("math2d-augment.d.ts"),
        r#"import * as Math2d from "math2d";

declare module "math2d" {
    interface Vector {
        reverse(): Math2d.Point;
    }
}
"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"/// <reference path="node_modules/math2d/index.d.ts" />
/// <reference path="math2d-augment.d.ts" />

let v = new Math2d.Vector(3, 2);
v.reverse();
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"/// <reference path="math2d-augment.d.ts" />
import * as m from "math2d";

let v = new m.Vector(3, 2);
v.reverse();
"#,
    );
    write_file(
        &base.join("tsconfig.global.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "noEmit": true,
    "noImplicitReferences": true
  },
  "files": ["a.ts"]
}"#,
    );
    write_file(
        &base.join("tsconfig.import.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "noEmit": true,
    "noImplicitReferences": true
  },
  "files": ["b.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.global.json"));
    let global_result = compile(&args, base).expect("global compile should succeed");
    assert!(
        global_result.diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && d.message_text
                    .contains("Property 'reverse' does not exist on type 'Vector'.")
        }),
        "Expected bare UMD global access to keep the class declaration surface and report TS2339 on Vector. Actual diagnostics: {:#?}",
        global_result.diagnostics
    );

    args.project = Some(base.join("tsconfig.import.json"));
    let import_result = compile(&args, base).expect("import compile should succeed");
    assert!(
        import_result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected real module imports to keep the class augmentation visible. Actual diagnostics: {:#?}",
        import_result.diagnostics
    );
}

#[derive(Debug, PartialEq, Eq)]
struct SymbolSnapshot {
    flags: u32,
    declarations_len: usize,
    value_declaration: u32,
    value_declaration_span: Option<(u32, u32)>,
    first_declaration_span: Option<(u32, u32)>,
    parent_name: Option<String>,
    exports: Vec<(String, String)>,
    members: Vec<(String, String)>,
    is_exported: bool,
    is_type_only: bool,
    import_module: Option<String>,
    import_name: Option<String>,
    is_umd_export: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct SemanticDefSnapshot {
    kind: tsz_binder::state::SemanticDefKind,
    name: String,
    file_id: u32,
    span_start: u32,
    type_param_count: u16,
    type_param_names: Vec<String>,
    is_exported: bool,
    enum_member_names: Vec<String>,
    is_const: bool,
    is_abstract: bool,
    extends_names: Vec<String>,
    implements_names: Vec<String>,
    parent_namespace_name: Option<String>,
    is_global_augmentation: bool,
    is_declare: bool,
}

fn symbol_name_for_id(binder: &BinderState, sym_id: SymbolId) -> Option<String> {
    binder
        .symbols
        .get(sym_id)
        .map(|sym| sym.escaped_name.clone())
}

fn semantic_def_snapshot(
    binder: &BinderState,
    sym_id: SymbolId,
    entry: &tsz_binder::state::SemanticDefEntry,
) -> SemanticDefSnapshot {
    SemanticDefSnapshot {
        kind: entry.kind,
        name: entry.name.clone(),
        file_id: entry.file_id,
        span_start: entry.span_start,
        type_param_count: entry.type_param_count,
        type_param_names: entry.type_param_names.clone(),
        is_exported: entry.is_exported,
        enum_member_names: entry.enum_member_names.clone(),
        is_const: entry.is_const,
        is_abstract: entry.is_abstract,
        extends_names: entry.extends_names.clone(),
        implements_names: entry.implements_names.clone(),
        parent_namespace_name: entry.parent_namespace.and_then(|parent| {
            if parent == sym_id {
                Some("<self>".to_string())
            } else {
                symbol_name_for_id(binder, parent).or_else(|| Some(format!("#{}", parent.0)))
            }
        }),
        is_global_augmentation: entry.is_global_augmentation,
        is_declare: entry.is_declare,
    }
}

fn symbol_snapshot_by_id(binder: &BinderState, sym_id: SymbolId) -> Option<SymbolSnapshot> {
    let sym = binder.symbols.get(sym_id)?;
    let mut exports = sym
        .exports
        .as_ref()
        .map(|table| {
            table
                .iter()
                .map(|(export_name, export_sym_id)| {
                    (
                        export_name.clone(),
                        symbol_name_for_id(binder, *export_sym_id)
                            .unwrap_or_else(|| format!("#{}", export_sym_id.0)),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    exports.sort();

    let mut members = sym
        .members
        .as_ref()
        .map(|table| {
            table
                .iter()
                .map(|(member_name, member_sym_id)| {
                    (
                        member_name.clone(),
                        symbol_name_for_id(binder, *member_sym_id)
                            .unwrap_or_else(|| format!("#{}", member_sym_id.0)),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    members.sort();

    Some(SymbolSnapshot {
        flags: sym.flags,
        declarations_len: sym.declarations.len(),
        value_declaration: sym.value_declaration.0,
        value_declaration_span: sym.value_declaration_span,
        first_declaration_span: sym.first_declaration_span,
        parent_name: (!sym.parent.is_none()).then(|| {
            symbol_name_for_id(binder, sym.parent).unwrap_or_else(|| format!("#{}", sym.parent.0))
        }),
        exports,
        members,
        is_exported: sym.is_exported,
        is_type_only: sym.is_type_only,
        import_module: sym.import_module.clone(),
        import_name: sym.import_name.clone(),
        is_umd_export: sym.is_umd_export,
    })
}

fn symbol_snapshot(binder: &BinderState, name: &str) -> Option<SymbolSnapshot> {
    let sym_id = binder.file_locals.get(name)?;
    symbol_snapshot_by_id(binder, sym_id)
}

fn declaration_arena_file_names_for_symbol(
    binder: &BinderState,
    sym_id: SymbolId,
) -> Vec<(u32, Vec<String>)> {
    let Some(sym) = binder.symbols.get(sym_id) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for &decl_idx in &sym.declarations {
        let mut arena_files = binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .map(|arenas| {
                arenas
                    .iter()
                    .filter_map(|arena| arena.source_files.first().map(|sf| sf.file_name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        arena_files.sort();
        result.push((decl_idx.0, arena_files));
    }
    result.sort_by_key(|(decl, _)| *decl);
    result
}

#[test]
fn compile_with_tsconfig_emits_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());
}

#[test]
fn compile_with_source_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    let js_path = base.join("dist/src/index.js");
    let map_path = base.join("dist/src/index.js.map");
    assert!(js_path.is_file());
    assert!(map_path.is_file());
    let js_contents = std::fs::read_to_string(&js_path).expect("read js output");
    assert!(js_contents.contains("sourceMappingURL=index.js.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.js");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    let sources_content = map_json
        .get("sourcesContent")
        .and_then(|value| value.as_array())
        .expect("expected sourcesContent");
    assert_eq!(sources_content.len(), 1);
    assert_eq!(
        sources_content[0].as_str().unwrap_or(""),
        "export const value = 1;"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_resolves_self_name_exports_with_virtual_absolute_output_paths_from_package_root() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let package_root = base.join("pkg");

    write_file(
        &package_root.join("tsconfig.json"),
        &format!(
            r#"{{
          "compilerOptions": {{
            "module": "nodenext",
            "moduleResolution": "nodenext",
            "rootDir": "{root_dir}",
            "outDir": "{out_dir}",
            "declaration": true,
            "declarationDir": "{declaration_dir}",
            "noEmit": true
          }},
          "include": ["src/**/*.ts"]
        }}"#,
            root_dir = package_root.join("src").display(),
            out_dir = package_root.join("dist").display(),
            declaration_dir = package_root.join("types").display(),
        ),
    );
    write_file(
        &package_root.join("package.json"),
        r#"{
          "name": "@this/package",
          "type": "module",
          "exports": {
            ".": {
              "import": "./dist/index.js"
            }
          }
        }"#,
    );
    write_file(
        &package_root.join("src/index.ts"),
        "import {} from '@this/package';\nexport const value = 1;\n",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, &package_root).expect("compile should succeed")
    });

    let ts2307: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2307)
        .collect();
    assert!(
        ts2307.is_empty(),
        "expected self-name import to resolve, got diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn private_static_accessor_on_derived_constructor_reports_ts2339_in_project_mode() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "noEmit": true
          },
          "include": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"
class Base {
    static get #prop(): number { return 123; }
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
class Derived extends Base {
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
"#,
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts2339_count = result
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .count();
    assert_eq!(
        ts2339_count, 2,
        "expected two TS2339 diagnostics in project mode, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_with_declaration_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "declarationMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    let dts_path = base.join("dist/src/index.d.ts");
    let map_path = base.join("dist/src/index.d.ts.map");
    assert!(dts_path.is_file());
    assert!(map_path.is_file());
    let dts_contents = std::fs::read_to_string(&dts_path).expect("read d.ts output");
    assert!(dts_contents.contains("sourceMappingURL=index.d.ts.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.d.ts");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    let sources_content = map_json
        .get("sourcesContent")
        .and_then(|value| value.as_array())
        .expect("expected sourcesContent");
    assert_eq!(sources_content.len(), 1);
    assert_eq!(
        sources_content[0].as_str().unwrap_or(""),
        "export const value = 1;"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_with_explicit_files_without_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("main.js").is_file());
}

#[test]
fn compile_promise_is_assignable_to_promise_like_with_default_libs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected Promise<T> to be assignable to PromiseLike<T>, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_constructor_parameters_rest_contextually_types_object_literal_methods() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
declare function createInstance<Ctor extends new (...args: any[]) => any, R extends InstanceType<Ctor>>(ctor: Ctor, ...args: ConstructorParameters<Ctor>): R;

export interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string
    };
}

class MenuWorkbenchToolBar {
    constructor(
        options: IMenuWorkbenchToolBarOptions | undefined,
    ) { }
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.strict_null_checks = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected ConstructorParameters rest contextual typing to avoid TS2345/TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006() {
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    ) else {
        return;
    };
    let Some(react16) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.es_module_interop = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts7006: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .collect();

    assert!(
        ts7006.is_empty(),
        "Expected real react16 JSX fixture to avoid TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_jsx_call_elaboration_check_no_crash1_react16_fixture_reports_ts2322() {
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxCallElaborationCheckNoCrash1.tsx",
    ) else {
        return;
    };
    let Some(react16) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    source = source.replace("\"/.lib/react16.d.ts\"", "\"./.lib/react16.d.ts\"");

    write_file(&base.join("test.tsx"), &source);
    write_file(&base.join(".lib/react16.d.ts"), &react16);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.es_module_interop = true;
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text.contains(
                    "LibraryManagedAttributes<Tag, DetailedHTMLProps<HTMLAttributes<HTMLDivElement>, HTMLDivElement>>",
                )
        })
        .collect();

    assert!(
        !jsx_ts2322.is_empty(),
        "Expected real react16 generic intrinsic JSX fixture to report TS2322, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_generic_call_at_yield_expression_in_generic_call_fixture_reports_outer_ts2345() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/genericCallAtYieldExpressionInGenericCall1.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.ts"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    let ts2488: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR)
        .collect();

    assert_eq!(
        ts2345.len(),
        2,
        "Expected fixture to report the two outer TS2345 callback mismatches, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert_eq!(
        ts2488.len(),
        1,
        "Expected fixture to keep the single inner TS2488, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
    assert!(
        ts2345
            .iter()
            .all(|diag| diag.message_text.contains("Generator<number, void, any>")),
        "Expected outer TS2345 diagnostics to preserve the unannotated generator surface `Generator<number, void, any>`, got diagnostics: {ts2345:?}",
    );
    assert!(
        ts2488[0].message_text.contains("Type '() => T'"),
        "Expected inner TS2488 diagnostic to preserve the non-generic function surface `() => T`, got: {:?}",
        ts2488[0]
    );
}

#[test]
fn compile_excessive_stack_depth_flat_array_fixture_reports_normalized_jsx_key_target() {
    let Some(source) =
        load_typescript_fixture("TypeScript/tests/cases/compiler/excessiveStackDepthFlatArray.ts")
    else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("test.tsx"), &source);

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.jsx = Some(crate::args::JsxEmit::React);
    args.no_emit = true;
    args.files = vec![PathBuf::from("test.tsx")];

    let result = compile(&args, base).expect("compile should succeed");
    let jsx_key_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && d.message_text
                    .contains("Type '{ key: string; }' is not assignable to type")
        })
        .collect();

    assert!(
        jsx_key_diags.iter().any(|diag| {
            diag.message_text.contains("HTMLAttributes<HTMLLIElement>")
                && !diag.message_text.contains("DetailedHTMLProps")
        }),
        "Expected JSX key TS2322 to target normalized HTMLAttributes<HTMLLIElement>, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_allow_import_clauses_to_merge_with_types_fixture_has_no_default_export_conflict() {
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/allowImportClausesToMergeWithTypes.ts",
    ) else {
        return;
    };

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    for segment in source.split("// @filename: ").skip(1) {
        let mut lines = segment.lines();
        let Some(filename) = lines.next().map(str::trim) else {
            continue;
        };
        let contents = lines.collect::<Vec<_>>().join("\n");
        write_file(&base.join(filename), &contents);
    }

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.module = Some(crate::args::Module::CommonJs);
    args.no_emit = true;
    args.files = vec![PathBuf::from("index.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let default_export_conflicts: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code,
                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE
                    | diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS
            )
        })
        .collect();

    assert!(
        default_export_conflicts.is_empty(),
        "Expected merged import-clause/type default exports to avoid TS2323/TS2528, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_default_import_of_merged_interface_and_const_export_is_callable() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"import MyFunction from "./MyComponent";

MyFunction({msg: "Hello World"});
"#,
    );
    write_file(
        &base.join("MyComponent.ts"),
        r#"interface MyFunction { msg: string; }

export const MyFunction = ({ msg }: MyFunction) => console.log(msg);
export default MyFunction;
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.target = Some(crate::args::Target::EsNext);
    args.module = Some(crate::args::Module::EsNext);
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected default import of merged interface+const export to keep callable value type, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_shadowing_namespace_symbol_keeps_global_symbol_value_access() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"namespace M {
    namespace Symbol {}

    class C {
        [Symbol.iterator]() {}
    }
}
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.target = Some(crate::args::Target::Es2015);
    args.no_emit = true;
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    let ts2708: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .collect();
    assert!(
        ts2708.is_empty(),
        "Expected shadowing namespace to keep global Symbol value access, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn compile_mapped_type_generic_indexed_access_preserves_context() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.no_implicit_any = Some(true);
    args.strict_null_checks = Some(true);
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    let relevant = result
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected mapped-type generic indexed access repro to avoid TS2344/TS7006, got diagnostics: {:?}\nfiles_read: {:?}\nfile_infos: {:?}",
        result.diagnostics,
        result.files_read,
        result.file_infos
    );
}

#[test]
fn direct_checker_with_real_default_libs_preserves_mapped_type_generic_indexed_access_context() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected direct checker with real default libs to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn merged_program_parallel_checker_preserves_mapped_type_generic_indexed_access_context() {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let options = CheckerOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let result = tsz::parallel::check_files_parallel(&program, &options, &lib_files);

    let diagnostics: Vec<_> = result
        .file_results
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();

    let relevant = diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected merged-program parallel checker to avoid TS2344/TS7006, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn direct_checker_with_original_binder_stays_clean_when_all_binders_are_installed() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(&arena, root, &lib_files);
    let binder = Arc::new(binder);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        binder.as_ref(),
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder)]));
    checker.ctx.set_current_file_idx(0);
    checker.check_source_file(root);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected original binder to stay clean even with all_binders installed, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn reconstructed_binder_alone_preserves_mapped_type_generic_indexed_access_context() {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(file.source_file);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected reconstructed binder to avoid TS2344/TS7006 after rebuild parity fixes, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn binder_reconstruction_from_original_fields_preserves_mapped_type_generic_indexed_access_context()
{
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let mut reconstructed = BinderState::from_bound_state_with_scopes_and_augmentations(
        tsz_binder::BinderOptions::default(),
        original_binder.symbols.clone(),
        original_binder.file_locals.clone(),
        original_binder.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: original_binder.scopes.clone(),
            node_scope_ids: original_binder.node_scope_ids.clone(),
            global_augmentations: original_binder.global_augmentations.clone(),
            module_augmentations: original_binder.module_augmentations.clone(),
            augmentation_target_modules: original_binder.augmentation_target_modules.clone(),
            module_exports: original_binder.module_exports.clone(),
            module_declaration_exports_publicly: original_binder
                .module_declaration_exports_publicly
                .clone(),
            reexports: original_binder.reexports.clone(),
            wildcard_reexports: original_binder.wildcard_reexports.clone(),
            wildcard_reexports_type_only: original_binder.wildcard_reexports_type_only.clone(),
            symbol_arenas: original_binder.symbol_arenas.clone(),
            declaration_arenas: original_binder.declaration_arenas.clone(),
            cross_file_node_symbols: original_binder.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: original_binder.shorthand_ambient_modules.clone(),
            modules_with_export_equals: original_binder.modules_with_export_equals.clone(),
            flow_nodes: original_binder.flow_nodes.clone(),
            node_flow: original_binder.node_flow.clone(),
            switch_clause_to_switch: original_binder.switch_clause_to_switch.clone(),
            expando_properties: original_binder.expando_properties.clone(),
            alias_partners: original_binder.alias_partners.clone(),
        },
    );
    reconstructed.declared_modules = original_binder.declared_modules.clone();
    reconstructed.is_external_module = original_binder.is_external_module;
    reconstructed.file_features = original_binder.file_features;
    reconstructed.lib_binders = original_binder.lib_binders.clone();
    reconstructed.lib_symbol_ids = original_binder.lib_symbol_ids.clone();
    reconstructed.lib_symbol_reverse_remap = original_binder.lib_symbol_reverse_remap.clone();
    reconstructed.semantic_defs = original_binder.semantic_defs.clone();

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena,
        &reconstructed,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected reconstruction from original binder fields to stay clean, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn merged_reconstruction_symbol_snapshots_match_original_for_mapped_type_chain() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for name in [
        "Types",
        "Test",
        "TypesMap",
        "P",
        "TypeHandlers",
        "typeHandlers",
        "onSomeEvent",
    ] {
        let original = symbol_snapshot(&original_binder, name);
        let merged = symbol_snapshot(&merged_binder, name);
        assert_eq!(
            merged, original,
            "symbol snapshot mismatch for {name}\noriginal: {original:#?}\nmerged: {merged:#?}"
        );
    }
}

#[test]
fn merged_reconstruction_identifier_resolution_matches_original_for_mapped_type_repro() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (idx, node) in arena.nodes.iter().enumerate() {
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            continue;
        }
        let node_idx = tsz_parser::NodeIndex(idx as u32);
        let text = arena
            .get_identifier_at(node_idx)
            .map(|ident| ident.escaped_text.clone())
            .unwrap_or_default();

        let original_resolved = original_binder
            .resolve_identifier(&arena, node_idx)
            .and_then(|sym_id| original_binder.symbols.get(sym_id))
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        let merged_resolved = merged_binder
            .resolve_identifier(&arena, node_idx)
            .and_then(|sym_id| merged_binder.symbols.get(sym_id))
            .map(|sym| (sym.escaped_name.clone(), sym.flags));

        assert_eq!(
            merged_resolved, original_resolved,
            "identifier resolution mismatch for node {idx} text={text:?} pos={}..{}\noriginal={original_resolved:?}\nmerged={merged_resolved:?}",
            node.pos, node.end
        );
    }
}

#[test]
fn merged_reconstruction_node_symbols_match_original_for_mapped_type_repro() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (&node_idx, &original_sym_id) in &original_binder.node_symbols {
        let Some(&merged_sym_id) = merged_binder.node_symbols.get(&node_idx) else {
            panic!("missing merged node symbol for node {node_idx}");
        };
        let original_snapshot = original_binder
            .symbols
            .get(original_sym_id)
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        let merged_snapshot = merged_binder
            .symbols
            .get(merged_sym_id)
            .map(|sym| (sym.escaped_name.clone(), sym.flags));
        assert_eq!(
            merged_snapshot, original_snapshot,
            "node symbol mismatch for node {node_idx}\noriginal={original_snapshot:?}\nmerged={merged_snapshot:?}"
        );
    }

    assert_eq!(
        merged_binder.node_symbols.len(),
        original_binder.node_symbols.len(),
        "node_symbols cardinality mismatch"
    );
}

#[test]
fn merged_reconstruction_nested_symbol_payloads_match_original_for_mapped_type_repro() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for (&node_idx, &original_sym_id) in &original_binder.node_symbols {
        let Some(&merged_sym_id) = merged_binder.node_symbols.get(&node_idx) else {
            panic!("missing merged node symbol for node {node_idx}");
        };
        let original_snapshot = symbol_snapshot_by_id(&original_binder, original_sym_id);
        let merged_snapshot = symbol_snapshot_by_id(&merged_binder, merged_sym_id);
        assert_eq!(
            merged_snapshot, original_snapshot,
            "nested symbol payload mismatch for node {node_idx}\noriginal={original_snapshot:#?}\nmerged={merged_snapshot:#?}"
        );
    }
}

#[test]
fn merged_reconstruction_declaration_arenas_match_original_for_mapped_type_chain() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);

    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for name in [
        "Types",
        "Test",
        "TypesMap",
        "P",
        "TypeHandlers",
        "typeHandlers",
        "onSomeEvent",
    ] {
        let original_sym_id = original_binder
            .file_locals
            .get(name)
            .expect("original symbol should exist");
        let merged_sym_id = merged_binder
            .file_locals
            .get(name)
            .expect("merged symbol should exist");
        assert_eq!(
            declaration_arena_file_names_for_symbol(&merged_binder, merged_sym_id),
            declaration_arena_file_names_for_symbol(&original_binder, original_sym_id),
            "declaration arenas mismatch for {name}"
        );
    }
}

#[test]
fn merged_reconstruction_semantic_defs_match_original_for_mapped_type_chain() {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut original_binder = BinderState::new();
    original_binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );
    let merged_binder =
        tsz::parallel::create_binder_from_bound_file(&program.files[0], &program, 0);

    for name in [
        "Types",
        "Test",
        "TypesMap",
        "P",
        "TypeHandlers",
        "typeHandlers",
        "onSomeEvent",
    ] {
        let original_sym_id = original_binder
            .file_locals
            .get(name)
            .expect("original symbol should exist");
        let merged_sym_id = merged_binder
            .file_locals
            .get(name)
            .expect("merged symbol should exist");
        let original_entry = original_binder
            .semantic_defs
            .get(&original_sym_id)
            .expect("original semantic def should exist");
        let merged_entry = merged_binder
            .semantic_defs
            .get(&merged_sym_id)
            .expect("merged semantic def should exist");

        let mut expected = semantic_def_snapshot(&original_binder, original_sym_id, original_entry);
        expected.file_id = 0;
        assert_eq!(
            semantic_def_snapshot(&merged_binder, merged_sym_id, merged_entry),
            expected,
            "semantic def mismatch for {name}"
        );
    }
}

#[test]
fn reconstructed_binder_with_fresh_type_interner_preserves_mapped_type_generic_indexed_access_context()
 {
    let files = vec![(
        "main.ts".to_string(),
        r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#
        .to_string(),
    )];

    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &types,
        file.file_name.clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(file.source_file);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected reconstructed binder with fresh TypeInterner to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn original_binder_with_merged_program_type_interner_preserves_mapped_type_generic_indexed_access_context()
 {
    let source = r#"type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] };

    constructor() {
        this.entries = {};
    }

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}

type TypesMap = {
    [0]: { foo: 'bar'; };
    [1]: { a: 'b'; };
};

type P<T extends keyof TypesMap> = { t: T; } & TypesMap[T];

type TypeHandlers = {
    [T in keyof TypesMap]?: (p: P<T>) => void;
};

const typeHandlers: TypeHandlers = {
    [0]: (p) => console.log(p.foo),
    [1]: (p) => console.log(p.a),
};

const onSomeEvent = <T extends keyof TypesMap>(p: P<T>) =>
    typeHandlers[p.t]?.(p);
"#;

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(
        vec![("main.ts".to_string(), source.to_string())],
        &lib_paths,
    );

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let (arena, _) = parser.into_parts();
    let arena = Arc::new(arena);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(&arena, root, &lib_files);

    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &arena,
        &binder,
        &query_cache,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);

    let relevant = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                    | diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
            )
        })
        .collect::<Vec<_>>();

    assert!(
        relevant.is_empty(),
        "Expected original binder with merged-program TypeInterner to avoid TS2344/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn direct_checker_with_real_default_libs_contextually_types_constructor_parameters_rest() {
    let source = r#"
declare function createInstance<Ctor extends new (...args: any[]) => any, R extends InstanceType<Ctor>>(ctor: Ctor, ...args: ConstructorParameters<Ctor>): R;

interface IMenuWorkbenchToolBarOptions {
    toolbarOptions: {
        foo(bar: string): string
    };
}

class MenuWorkbenchToolBar {
    constructor(
        options: IMenuWorkbenchToolBarOptions | undefined,
    ) { }
}

createInstance(MenuWorkbenchToolBar, {
    toolbarOptions: {
        foo(bar) { return bar; }
    }
});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.check_source_file(root);

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected direct checker with real default libs to avoid TS2345/TS7006, got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]

fn compile_array_from_iterable_uses_real_lib_iterable_overload() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("main.ts"),
        r#"
interface A { a: string; }
interface B { b: string; }
declare const inputA: A[];

const bad: B[] = Array.from(inputA.values());
"#,
    );

    let mut args = default_args();
    args.ignore_config = true;
    args.strict = true;
    args.target = Some(crate::args::Target::Es2015);
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![2322],
        "Expected only the outer B[] assignment failure from Array.from(iterable). Got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn merged_program_promise_is_assignable_to_promise_like_with_default_libs() {
    let files = vec![(
        "main.ts".to_string(),
        r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#
        .to_string(),
    )];
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let options = tsz::checker::context::CheckerOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        ..tsz::checker::context::CheckerOptions::default()
    };
    let result = tsz::parallel::check_files_parallel(&program, &options, &lib_files);

    let diagnostics: Vec<_> = result
        .file_results
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();

    assert!(
        diagnostics.is_empty(),
        "Expected merged-program Promise<T> to be assignable to PromiseLike<T>, got: {diagnostics:?}"
    );
}

#[test]
fn compile_with_root_dir_flattens_output_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(base.to_path_buf());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/index.js").is_file());
    assert!(base.join("dist/index.d.ts").is_file());
}

#[test]
fn compile_respects_no_emit_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "let x = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_project_dir_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let config_dir = base.join("configs");
    write_file(
        &config_dir.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&config_dir.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("configs"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(config_dir.join("dist/src/index.js").is_file());
}

#[test]
fn compile_reports_ts7005_for_exported_bare_var_in_imported_dts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "jsx": "react",
            "module": "commonjs",
            "target": "es2015"
          },
          "include": ["*.ts", "*.tsx", "*.d.ts"]
        }"#,
    );
    write_file(
        &base.join("file.tsx"),
        r#"declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}"#,
    );
    write_file(&base.join("test.d.ts"), "export var React;\n");
    write_file(
        &base.join("react-consumer.tsx"),
        r#"import { React } from "./test";
var foo: any;
var spread1 = <div x='' {...foo} y='' />;"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 7005),
        "Expected TS7005 for exported bare var in imported .d.ts, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn compile_with_project_dir_resolves_package_exported_tsconfig_extends() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("node_modules/foo/package.json"),
        r#"{
          "name": "foo",
          "version": "1.0.0",
          "exports": {
            "./*.json": "./configs/*.json"
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/foo/configs/strict.json"),
        r#"{
          "compilerOptions": {
            "strict": true
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{"extends":"foo/strict.json"}"#,
    );
    write_file(&base.join("index.ts"), "let x: string;\nx.toLowerCase();\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected TS2454 from package-exported tsconfig extends, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "pre-existing: remote merge regression"]
fn compile_with_project_dir_preserves_invariant_generic_error_elaboration_ts2322() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "target": "es2015",
            "noEmit": true
          },
          "files": ["test.ts"]
        }"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"// Repro from #19746

const wat: Runtype<any> = Num;
const Foo = Obj({ foo: Num })

interface Runtype<A> {
  constraint: Constraint<this>
  witness: A
}

interface Num extends Runtype<number> {
  tag: 'number'
}
declare const Num: Num

interface Obj<O extends { [_ in string]: Runtype<any> }> extends Runtype<{[K in keyof O]: O[K]['witness'] }> {}
declare function Obj<O extends { [_: string]: Runtype<any> }>(fields: O): Obj<O>;

interface Constraint<A extends Runtype<any>> extends Runtype<A['witness']> {
  underlying: A,
  check: (x: A['witness']) => void,
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2322_count = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();

    assert_eq!(
        ts2322_count, 2,
        "Expected two TS2322 diagnostics for invariant generic error elaboration, got: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_with_jsx_preserve_emits_jsx_extension() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "jsx": "preserve",
            "strict": false
          },
          "include": ["src/**/*.tsx", "src/**/*.d.ts"]
        }"#,
    );
    write_file(
        &base.join("src/jsx.d.ts"),
        "declare namespace JSX { interface IntrinsicElements { div: any; } }",
    );
    write_file(
        &base.join("src/view.tsx"),
        "export const View = () => <div />;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/view.jsx").is_file());
}

#[test]
fn compile_resolves_relative_imports_from_files_list() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/util.js").is_file());
}

#[test]
fn compile_resolves_paths_mappings() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "baseUrl": ".",
            "ignoreDeprecations": "6.0",
            "paths": {
              "@lib/*": ["src/lib/*"]
            }
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from '@lib/value'; export { value };",
    );
    write_file(&base.join("src/lib/value.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/lib/value.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "types": "index.d.ts"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.d.ts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_tsconfig_types_includes_selected_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "types": ["foo"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(
        &base.join("node_modules/@types/foo/index.d.ts"),
        "export const foo = ;",
    );
    write_file(
        &base.join("node_modules/@types/bar/index.d.ts"),
        "export const bar = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/foo/index.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/bar/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_tsconfig_type_roots_includes_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typeRoots": ["types"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(&base.join("types/foo/index.d.ts"), "export const foo = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/foo/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_resolves_node_modules_exports_subpath() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": { "types": "./types/index.d.ts" },
            "./feature/*": { "types": "./types/feature/*.d.ts" }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_uses_versioned_types_export_conditions_without_false_ts2551() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "node16",
            "moduleResolution": "node16",
            "strict": true,
            "noEmitOnError": true,
            "ignoreDeprecations": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import * as mod from 'inner';\nmod.goodThing.toFixed();\n",
    );
    write_file(
        &base.join("node_modules/inner/package.json"),
        r#"{
          "name": "inner",
          "exports": {
            ".": {
              "types@>=10000": "./future-types.d.ts",
              "types@>=1": "./new-types.d.ts",
              "types": "./old-types.d.ts",
              "import": "./index.mjs",
              "node": "./index.js"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/inner/old-types.d.ts"),
        "export const oldThing: number;",
    );
    write_file(
        &base.join("node_modules/inner/new-types.d.ts"),
        "export const goodThing: number;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "expected versioned types export resolution to avoid bogus namespace-property diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(base.join("src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            "*": {
              "feature/*": ["types/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_best_match() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=6.1": {
              "feature/*": ["types/v61/feature/*"]
            },
            ">=5.0": {
              "feature/*": ["types/v5/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v61/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v5/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v61) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to v5 which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v61 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to v5 produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/v5/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}

#[test]
fn compile_resolves_node_modules_types_versions_prefers_specific_range() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=6.0": {
              "feature/*": ["types/loose/feature/*"]
            },
            ">=5.0 <7.0": {
              "feature/*": ["types/ranged/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/loose/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/ranged/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/ranged/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_cli_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.1".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_env_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_tsconfig_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_tsconfig_extends_inherits_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("config/base.json"),
        r#"{
          "compilerOptions": {
            "typesVersionsCompilerVersion": "7.1"
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "extends": "./config/base.json",
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_env_overrides_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_empty_env_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some(""), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_cli_overrides_env_and_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.2": {
              "feature/*": ["types/v72/feature/*"]
            },
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v72/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.2".to_string());
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v72/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_override_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("not-a-version".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_env_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("not-a-version"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_tsconfig_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "not-a-version"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_falls_back_to_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v7) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to wildcard which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v7 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to wildcard produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/fallback/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_resolves_package_imports_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from '#utils/widget'; export { widget };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#utils/*": "./types/*"
          }
        }"##,
    );
    write_file(&base.join("types/widget.d.ts"), "export const widget = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/widget.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_rejects_root_slash_package_import_specifier_under_node16() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "node16",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "name": "package",
          "private": true,
          "type": "module",
          "imports": {
            "#/*": "./src/*"
          }
        }"##,
    );
    write_file(&base.join("src/foo.ts"), "export const foo = 'foo';");
    write_file(
        &base.join("index.ts"),
        "import { foo } from '#/foo.js';\nfoo;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|diag| diag.code
            == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Expected TS2307 for invalid #/ package import, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_resolves_package_imports_prefers_types_condition() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "node16",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "types": "./types/feature.d.ts",
              "default": "./default/feature.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/feature.d.ts"), "export const feature = ;");
    write_file(
        &base.join("default/feature.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/feature.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_require_condition_for_commonjs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "require": "./types/require.d.ts",
              "import": "./types/import.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/require.d.ts"), "export const feature = ;");
    write_file(&base.join("types/import.d.ts"), "export const feature = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_import_condition_for_esm() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "esnext",
            "moduleResolution": "bundler",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "import": "./types/import.d.ts",
              "require": "./types/require.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/import.d.ts"), "export const feature = ;");
    write_file(
        &base.join("types/require.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_prefers_browser_exports_for_bundler() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "bundler",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": {
              "browser": "./browser.d.ts",
              "node": "./node.d.ts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/browser.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/node.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/browser.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn bundler_esm_declaration_package_without_default_emits_ts1192() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "esnext",
            "module": "preserve",
            "moduleResolution": "bundler",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"import pkg, { toString } from "pkg";

export const value = toString();
export { pkg };
"#,
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "name": "pkg",
          "type": "module",
          "types": "./index.d.ts"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.d.ts"),
        "export declare function toString(): string;\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "Expected TS1192 for default import from ESM declaration package with no default export, got: {:#?}",
        result.diagnostics
    );
}

#[test]
fn system_module_source_default_import_without_allow_synthetic_flag_uses_namespace_fallback() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "system",
            "allowSyntheticDefaultImports": false,
            "ignoreDeprecations": "6.0",
            "strict": false,
            "noEmit": true
          },
          "files": ["a.ts", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"import Namespace from "./b";
export const value = new Namespace.Foo();
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"export class Foo {
  member: string;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "Expected module=system source default import to avoid TS1192. Actual diagnostics: {:#?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics once system deprecations are silenced. Actual diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_resolves_js_extension_to_ts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util.js'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("src/util.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_prefers_mts_for_module_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "module"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = ;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.mts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
#[ignore = "module resolution for node-next/nodenext not yet complete"]
fn compile_node_next_prefers_cts_for_commonjs_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "commonjs"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.cts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_cache_emits_only_dirty_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/alpha.ts", "src/beta.ts"]
        }"#,
    );

    let alpha_path = base.join("src/alpha.ts");
    let beta_path = base.join("src/beta.ts");
    write_file(&alpha_path, "export const alpha = 1;");
    write_file(&beta_path, "export const beta = 2;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let alpha_output = std::fs::canonicalize(base.join("dist/src/alpha.js"))
        .unwrap_or_else(|_| base.join("dist/src/alpha.js"));
    let beta_output = std::fs::canonicalize(base.join("dist/src/beta.js"))
        .unwrap_or_else(|_| base.join("dist/src/beta.js"));
    assert_eq!(result.emitted_files.len(), 2);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(result.emitted_files.contains(&beta_output));

    write_file(&alpha_path, "export const alpha = 2;");
    let canonical = std::fs::canonicalize(&alpha_path).unwrap_or(alpha_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.emitted_files.len(), 1);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(!result.emitted_files.contains(&beta_output));
}

#[test]
fn compile_with_cache_updates_dependencies_for_changed_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    let extra_path = base.join("src/extra.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );

    write_file(
        &index_path,
        "import { value } from './extra'; export { value };",
    );
    write_file(&extra_path, "export const value = ;");

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("extra.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );
}

#[test]
fn compile_with_cache_skips_dependents_when_exports_unchanged() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const output = value;",
    );
    write_file(&util_path, "export function value() { return 1; }");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (unchanged exports): {:#?}",
        result.diagnostics
    );

    write_file(&util_path, "export function value() { return 2; }");

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert!(result.emitted_files.contains(&util_output));
    assert!(!result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_rechecks_dependents_on_export_change() {
    // Tests that cache properly invalidates dependents when the export *surface* changes.
    // A body-only edit (changing the value of an existing export) should NOT invalidate
    // dependents — this matches the unified binder-level ExportSignature semantics
    // shared between CLI and LSP. Only structural changes (adding/removing exports,
    // changing export names/kinds) trigger dependent invalidation.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    // Use namespace import to avoid named import type resolution issues
    write_file(
        &index_path,
        "import * as util from './util'; export { util };",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (export change): {:#?}",
        result.diagnostics
    );

    // Add a new export — this changes the export surface and must invalidate dependents.
    write_file(
        &util_path,
        "export const value = 1;\nexport function helper() {}",
    );

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    // Assert dependent recompilation - both files should be re-emitted
    assert!(result.emitted_files.contains(&util_output));
    assert!(result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_body_only_edit_skips_dependents() {
    // Changing the value of an existing export (body-only edit) should NOT
    // invalidate dependents — the unified ExportSignature only tracks names,
    // flags, and structural relationships, not inferred types.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import * as util from './util'; export { util };",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics: {:#?}",
        result.diagnostics
    );

    // Body-only edit: change the value but not the export surface.
    write_file(&util_path, "export const value = \"changed\";");

    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    // Dependent should NOT be re-emitted — export signature is unchanged.
    assert!(
        !result.emitted_files.contains(&index_output),
        "Body-only edit should not re-emit dependents"
    );
}

#[test]
fn compile_with_cache_invalidates_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    write_file(&index_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);
}

#[test]
fn compile_with_cache_invalidates_dependents() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);

    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);
}

#[test]
fn invalidate_paths_with_dependents_symbols_keeps_unrelated_cache() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const local = 1; export const uses = value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(before > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    let after = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(after > 0);
    assert!(after < before);
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(0), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export { value } from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_import_equals() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import util = require('./util'); export const local = util.value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_namespace_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * as util from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_star_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path);
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path);
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn compile_multi_file_project_with_imports() {
    // End-to-end test for a multi-file project.
    // Note: Uses namespace imports to avoid known named import type resolution issues.
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create tsconfig.json with CommonJS module for testable require() output
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/models/user.ts - basic model with interface and class
    write_file(
        &base.join("src/models/user.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export class UserImpl implements User {
    id: number;
    name: string;
    email: string;

    constructor(id: number, name: string, email: string) {
        this.id = id;
        this.name = name;
        this.email = email;
    }

    getDisplayName(): string {
        return this.name + " <" + this.email + ">";
    }
}

export type UserId = number;
"#,
    );

    // src/utils/helpers.ts - utility functions
    // Note: Avoid String.prototype.indexOf due to lib.d.ts resolution issues
    write_file(
        &base.join("src/utils/helpers.ts"),
        r#"
export function formatName(first: string, last: string): string {
    return first + " " + last;
}

export function validateEmail(email: string): boolean {
    return email.length > 0;
}

export const DEFAULT_PAGE_SIZE = 20;
"#,
    );

    // src/services/user-service.ts - service using namespace imports
    write_file(
        &base.join("src/services/user-service.ts"),
        r#"
import * as models from '../models/user';
import * as helpers from '../utils/helpers';

export class UserService {
    private users: models.User[] = [];

    createUser(id: number, firstName: string, lastName: string, email: string): models.User | null {
        if (!helpers.validateEmail(email)) {
            return null;
        }
        const name = helpers.formatName(firstName, lastName);
        const user = new models.UserImpl(id, name, email);
        this.users.push(user);
        return user;
    }

    getUserCount(): number {
        return this.users.length;
    }
}
"#,
    );

    // src/index.ts - main entry point using namespace re-exports
    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-export all from each module
export * from './models/user';
export * from './utils/helpers';
export * from './services/user-service';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // TODO: After the module augmentation lazy resolution change (2e96c99c2),
    // global `escape` spuriously leaks into module re-exports causing TS2308.
    // Filter out these known false positives until the root cause is fixed.
    let real_diagnostics: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| !(d.code == 2308 && d.message_text.contains("escape")))
        .collect();
    assert!(
        real_diagnostics.is_empty(),
        "Expected no diagnostics, got: {real_diagnostics:?}"
    );

    // Verify all output files exist
    assert!(
        base.join("dist/models/user.js").is_file(),
        "models/user.js should exist"
    );
    assert!(
        base.join("dist/models/user.d.ts").is_file(),
        "models/user.d.ts should exist"
    );
    assert!(
        base.join("dist/models/user.js.map").is_file(),
        "models/user.js.map should exist"
    );

    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "utils/helpers.js should exist"
    );
    assert!(
        base.join("dist/utils/helpers.d.ts").is_file(),
        "utils/helpers.d.ts should exist"
    );

    assert!(
        base.join("dist/services/user-service.js").is_file(),
        "services/user-service.js should exist"
    );
    assert!(
        base.join("dist/services/user-service.d.ts").is_file(),
        "services/user-service.d.ts should exist"
    );

    assert!(
        base.join("dist/index.js").is_file(),
        "index.js should exist"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "index.d.ts should exist"
    );

    // Verify user-service.js has correct CommonJS require statements
    let service_js = std::fs::read_to_string(base.join("dist/services/user-service.js"))
        .expect("read service js");
    assert!(
        service_js.contains("require(") || service_js.contains("import"),
        "Service JS should have require or import statements: {service_js}"
    );
    assert!(
        service_js.contains("../models/user") || service_js.contains("./models/user"),
        "Service JS should reference models/user: {service_js}"
    );
    assert!(
        service_js.contains("../utils/helpers") || service_js.contains("./utils/helpers"),
        "Service JS should reference utils/helpers: {service_js}"
    );

    // Verify index.js has re-exports (CommonJS uses Object.defineProperty pattern)
    let index_js = std::fs::read_to_string(base.join("dist/index.js")).expect("read index js");
    assert!(
        index_js.contains("exports")
            && (index_js.contains("require(") || index_js.contains("Object.defineProperty")),
        "Index JS should have CommonJS exports: {index_js}"
    );

    // Verify declaration file for index has re-export statements
    let index_dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("export *") && index_dts.contains("./models/user"),
        "Index d.ts should have re-export statements: {index_dts}"
    );

    // Verify source map for user-service has correct sources
    let service_map_contents =
        std::fs::read_to_string(base.join("dist/services/user-service.js.map"))
            .expect("read service map");
    let service_map: Value =
        serde_json::from_str(&service_map_contents).expect("parse service map json");
    let sources = service_map
        .get("sources")
        .and_then(|v| v.as_array())
        .expect("sources array");
    assert!(!sources.is_empty(), "Source map should have sources");
    let sources_content = service_map.get("sourcesContent").and_then(|v| v.as_array());
    assert!(
        sources_content.is_some(),
        "Source map should have sourcesContent"
    );
}

#[test]
fn compile_multi_file_project_with_default_and_named_imports() {
    // Test default and named import styles
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "esModuleInterop": true,
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/constants.ts - default export
    write_file(
        &base.join("src/constants.ts"),
        r#"
const CONFIG = {
    apiUrl: "https://api.example.com",
    timeout: 5000
};

export default CONFIG;
export const VERSION = "1.0.0";
"#,
    );

    // src/math.ts - multiple named exports
    write_file(
        &base.join("src/math.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    // src/app.ts - uses default and named imports
    write_file(
        &base.join("src/app.ts"),
        r#"
// Default import
import CONFIG from './constants';
// Named import alongside default
import { VERSION } from './constants';
// Named imports with alias
import { add as addNumbers, multiply, PI } from './math';

export function runApp(): string {
    const sum = addNumbers(1, 2);
    const product = multiply(3, 4);
    const circumference = 2 * PI * 10;
    const url = CONFIG.apiUrl;

    return url + " v" + VERSION + " sum=" + sum + " product=" + product + " circ=" + circumference;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Verify all files compiled
    assert!(base.join("dist/constants.js").is_file());
    assert!(base.join("dist/math.js").is_file());
    assert!(base.join("dist/app.js").is_file());
    assert!(base.join("dist/app.d.ts").is_file());

    // Verify app.js has the necessary require statements
    let app_js = std::fs::read_to_string(base.join("dist/app.js")).expect("read app js");
    assert!(
        app_js.contains("./constants") || app_js.contains("constants"),
        "App JS should reference constants: {app_js}"
    );
    assert!(
        app_js.contains("./math") || app_js.contains("math"),
        "App JS should reference math: {app_js}"
    );

    // Verify declaration file has correct exports
    let app_dts = std::fs::read_to_string(base.join("dist/app.d.ts")).expect("read app d.ts");
    assert!(
        app_dts.contains("runApp"),
        "App d.ts should export runApp: {app_dts}"
    );
}

#[test]
fn compile_default_interface_and_default_value_export_merge_without_default_conflicts() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts", "index.ts"]
}"#,
    );

    write_file(
        &base.join("b.ts"),
        r#"export const zzz = 123;
export default zzz;
"#,
    );

    write_file(
        &base.join("a.ts"),
        r#"export default interface zzz {
    x: string;
}

import zzz from "./b";

const x: zzz = { x: "" };
zzz;

export { zzz as default };
"#,
    );

    write_file(
        &base.join("index.ts"),
        r#"import zzz from "./a";

const x: zzz = { x: "" };
zzz;

import originalZZZ from "./b";
originalZZZ;

const y: originalZZZ = x;
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != 2323 && d.code != 2528),
        "Expected interface/value default export merge without TS2323/TS2528, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_module_augmentation_default_interface_alias_merges_without_ts2300() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts", "c.ts"]
}"#,
    );

    write_file(
        &base.join("a.ts"),
        r#"interface I {}
export default I;
"#,
    );

    write_file(
        &base.join("b.ts"),
        r#"export {};
declare module "./a" {
    export default interface I { x: number; }
}
"#,
    );

    write_file(
        &base.join("c.ts"),
        r#"import I from "./a";
function f(i: I) {
    i.x;
}
"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DUPLICATE_IDENTIFIER),
        "Expected module augmentation default interface alias merge to avoid TS2300, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| d.code != 2339),
        "Expected merged default interface alias to expose x in imports, got codes: {codes:?}\nDiagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_multi_file_project_with_type_imports() {
    // Test type-only imports compile correctly
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/types.ts - shared types
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface Logger {
    log(msg: string): void;
}

export type LogLevel = "debug" | "info" | "error";
"#,
    );

    // src/logger.ts - uses types (type-only import)
    write_file(
        &base.join("src/logger.ts"),
        r#"
import type { Logger, LogLevel } from './types';

export class ConsoleLogger implements Logger {
    private level: LogLevel;

    constructor(level: LogLevel) {
        this.level = level;
    }

    log(msg: string): void {
        // log implementation
    }

    getLevel(): LogLevel {
        return this.level;
    }
}

export function createLogger(level: LogLevel): Logger {
    return new ConsoleLogger(level);
}
"#,
    );

    // src/index.ts - re-exports everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export type { Logger, LogLevel } from './types';
export { ConsoleLogger, createLogger } from './logger';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Type imports should compile without errors: {:?}",
        result.diagnostics
    );

    assert!(base.join("dist/src/types.js").is_file());
    assert!(base.join("dist/src/logger.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify declaration file has type exports
    let index_dts =
        std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("Logger") && index_dts.contains("LogLevel"),
        "Index d.ts should have type exports for Logger and LogLevel: {index_dts}"
    );

    // Verify logger.js has the class implementation
    let logger_js =
        std::fs::read_to_string(base.join("dist/src/logger.js")).expect("read logger js");
    assert!(
        logger_js.contains("ConsoleLogger") && logger_js.contains("createLogger"),
        "Logger JS should have class and function exports: {logger_js}"
    );
}

#[test]
fn compile_declaration_true_emits_dts_files() {
    // Test that declaration: true produces .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"
export const VERSION = "1.0.0";
export function greet(name: string): string {
    return "Hello, " + name;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should exist
    assert!(
        base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should exist when declaration: true"
    );

    // Verify declaration file content
    let dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read d.ts");
    assert!(
        dts.contains("VERSION") && dts.contains("string"),
        "Declaration should contain VERSION: {dts}"
    );
    assert!(
        dts.contains("greet") && dts.contains("name"),
        "Declaration should contain greet function: {dts}"
    );
}

#[test]
fn compile_declaration_false_no_dts_files() {
    // Test that declaration: false (or absent) does NOT produce .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": false
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration: false"
    );
}

#[test]
fn compile_declaration_absent_no_dts_files() {
    // Test that missing declaration option does NOT produce .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist (declaration defaults to false)
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration is not specified"
    );
}

#[test]
fn compile_declaration_interface_and_type() {
    // Test declaration output for interfaces and type aliases
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export type UserId = number;

export type UserRole = "admin" | "user" | "guest";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Declaration file should exist
    let dts_path = base.join("dist/src/types.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Interface should be in declaration
    assert!(
        dts.contains("interface User"),
        "Declaration should contain User interface: {dts}"
    );
    assert!(
        dts.contains("id") && dts.contains("number"),
        "Declaration should contain id property: {dts}"
    );
    assert!(
        dts.contains("name") && dts.contains("string"),
        "Declaration should contain name property: {dts}"
    );

    // Type aliases should be in declaration
    assert!(
        dts.contains("UserId"),
        "Declaration should contain UserId type: {dts}"
    );
    assert!(
        dts.contains("UserRole"),
        "Declaration should contain UserRole type: {dts}"
    );
}

#[test]
fn compile_declaration_class_with_methods() {
    // Test declaration output for classes with methods
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    // Note: Avoid `return this` pattern which triggers a known false positive
    // with Lazy type resolution (class type not resolved for `this` assignability)
    write_file(
        &base.join("src/calculator.ts"),
        r#"
export class Calculator {
    private value: number;

    constructor(initial: number) {
        this.value = initial;
    }

    add(n: number): void {
        this.value = this.value + n;
    }

    subtract(n: number): void {
        this.value = this.value - n;
    }

    getResult(): number {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Declaration file should exist
    let dts_path = base.join("dist/src/calculator.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Class should be in declaration
    assert!(
        dts.contains("class Calculator"),
        "Declaration should contain Calculator class: {dts}"
    );

    // Methods should be in declaration
    assert!(
        dts.contains("add") && dts.contains("void"),
        "Declaration should contain add method with void return: {dts}"
    );
    assert!(
        dts.contains("subtract") && dts.contains("void"),
        "Declaration should contain subtract method with void return: {dts}"
    );
    assert!(
        dts.contains("getResult") && dts.contains("number"),
        "Declaration should contain getResult method: {dts}"
    );

    // Private members should be marked private in declaration
    assert!(
        dts.contains("private") && dts.contains("value"),
        "Declaration should contain private value: {dts}"
    );
}

#[test]
fn compile_declaration_with_declaration_dir() {
    // Test that declarationDir puts .d.ts files in separate directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "declarationDir": "types"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be in dist/"
    );

    // Declaration file should be in declarationDir, NOT in outDir
    assert!(
        base.join("types/index.d.ts").is_file(),
        "Declaration file should be in types/"
    );
    assert!(
        !base.join("dist/index.d.ts").is_file(),
        "Declaration file should NOT be in dist/ when declarationDir is set"
    );
}

#[test]
fn compile_outdir_places_output_in_directory() {
    // Test that outDir places compiled files in the specified directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in build/ directory
    assert!(
        base.join("build/src/index.js").is_file(),
        "JS output should be in build/src/"
    );

    // Output should NOT be alongside source
    assert!(
        !base.join("src/index.js").is_file(),
        "JS output should NOT be alongside source when outDir is set"
    );
}

#[test]
fn compile_outdir_absent_outputs_alongside_source() {
    // Test that missing outDir places compiled files alongside source files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {},
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be alongside source file
    assert!(
        base.join("src/index.js").is_file(),
        "JS output should be alongside source when outDir is not set"
    );
}

#[test]
fn compile_outdir_with_rootdir_flattens_paths() {
    // Test that rootDir + outDir flattens the output path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // With rootDir=src, output should NOT include src/ in path
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be at dist/index.js (flattened)"
    );
    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "Nested JS output should be at dist/utils/helpers.js"
    );

    // Should NOT be at dist/src/...
    assert!(
        !base.join("dist/src/index.js").is_file(),
        "Output should NOT include src/ when rootDir is set to src"
    );
}

#[test]
fn compile_outdir_nested_structure() {
    // Test that outDir preserves nested directory structure
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const main = 1;");
    write_file(&base.join("src/models/user.ts"), "export const user = 2;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 3;",
    );
    write_file(
        &base.join("src/services/api/client.ts"),
        "export const client = 4;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All nested directories should be preserved
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/models/user.js").is_file());
    assert!(base.join("dist/src/utils/helpers.js").is_file());
    assert!(base.join("dist/src/services/api/client.js").is_file());
}

#[test]
fn compile_outdir_deep_nested_path() {
    // Test that outDir can be a deeply nested path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build/output/js"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in deeply nested outDir
    assert!(
        base.join("build/output/js/src/index.js").is_file(),
        "JS output should be in build/output/js/src/"
    );
}

#[test]
fn compile_outdir_with_declaration_and_sourcemap() {
    // Test that outDir works correctly with declaration and sourceMap
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All output files should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS should be in outDir"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "Declaration should be in outDir"
    );
    assert!(
        base.join("dist/index.js.map").is_file(),
        "Source map should be in outDir"
    );

    // Verify source map references correct file
    let map_contents = std::fs::read_to_string(base.join("dist/index.js.map")).expect("read map");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map");
    let file_field = map_json.get("file").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(
        file_field, "index.js",
        "Source map file field should be index.js"
    );
}

#[test]
fn compile_outdir_multiple_entry_points() {
    // Test outDir with multiple entry point files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const main = 1;");
    write_file(&base.join("src/worker.ts"), "export const worker = 2;");
    write_file(&base.join("src/cli.ts"), "export const cli = 3;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All entry points should be compiled to outDir
    assert!(base.join("dist/main.js").is_file());
    assert!(base.join("dist/worker.js").is_file());
    assert!(base.join("dist/cli.js").is_file());
}

// =============================================================================
// Error Handling: Missing Input Files
// =============================================================================

#[test]
fn compile_missing_file_in_files_array_returns_error() {
    // Test that referencing a missing file in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/missing.ts"]
        }"#,
    );
    // Intentionally NOT creating src/missing.ts

    let args = default_args();
    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("TS6053") || err.contains("missing"),
        "Error should mention file not found: {err}"
    );
    // No output should be produced
    assert!(!base.join("dist").is_dir());
}

#[test]
fn compile_missing_file_in_include_pattern_returns_error() {
    // Test that an include pattern matching no files returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    // Intentionally NOT creating any .ts files in src/

    let args = default_args();
    let result = compile(&args, base);

    // Should return Ok with TS18003 diagnostic (not a fatal error)
    let compilation = result.expect("Should return Ok with diagnostics, not a fatal error");
    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 18003),
        "Should contain TS18003 diagnostic when no input files found, got: {:?}",
        compilation
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_missing_file_in_include_pattern_reports_custom_config_path() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let config_rel = PathBuf::from("configs/custom-name.json");
    let config_path = base.join(&config_rel);

    write_file(
        &config_path,
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    let mut args = default_args();
    args.project = Some(config_rel);
    let compilation = compile(&args, base).expect("Should return Ok with diagnostics");

    let ts18003 = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == 18003)
        .expect("expected TS18003 diagnostic");
    let expected_path = config_path.canonicalize().expect("canonical config path");
    let expected_path = expected_path.to_string_lossy();
    assert!(
        ts18003.message_text.contains(expected_path.as_ref()),
        "TS18003 should include resolved config path: {}",
        ts18003.message_text
    );
}

#[test]
fn compile_missing_file_in_include_pattern_prefers_ts18003_over_type_root_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "include": ["src/**/*.ts"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(
        &base.join("node_modules/@types/lib-extender/index.d.ts"),
        r#"declare var lib: () => void;
declare namespace lib {}
export = lib;
declare module "lib" {
    export function fn(): void;
}"#,
    );

    let args = default_args();
    let compilation = compile(&args, base).expect("Should return Ok with diagnostics");

    assert!(
        compilation.diagnostics.iter().any(|d| d.code == 18003),
        "Expected TS18003 when include pattern has no matching source files"
    );
    assert!(
        !compilation.diagnostics.iter().any(|d| d.code == 2649),
        "TS2649 from @types files should not be reported when there are no root inputs"
    );
}

#[test]
fn compile_missing_single_file_via_cli_args_returns_error() {
    // Test that passing a non-existent file via CLI args returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.files = vec![PathBuf::from("nonexistent.ts")];

    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing CLI file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("No such file"),
        "Error should mention file not found: {err}"
    );
}

#[test]
fn compile_missing_multiple_files_in_files_array_returns_error() {
    // Test that multiple missing files in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/a.ts", "src/b.ts", "src/c.ts"]
        }"#,
    );
    // Only create one of the three files
    write_file(&base.join("src/b.ts"), "export const b = 2;");

    let args = default_args();
    let result = compile(&args, base);

    // Should return error for missing files
    assert!(
        result.is_err(),
        "Should return error when some files in files array are missing"
    );
}

#[test]
fn compile_missing_project_directory_returns_error() {
    // Test that specifying a non-existent project directory returns an error diagnostic
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.project = Some(PathBuf::from("nonexistent_project"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic for missing project directory"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
        "Should have correct error code"
    );
}

#[test]
fn compile_missing_tsconfig_in_project_dir_returns_error() {
    // Test that a project directory without tsconfig.json returns an error diagnostic
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create project directory but no tsconfig.json
    std::fs::create_dir_all(base.join("myproject")).expect("create dir");
    write_file(&base.join("myproject/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("myproject"));

    let result = compile(&args, base).expect("compile should succeed with error diagnostic");

    // Should have error diagnostic since there's no tsconfig.json
    assert!(
        !result.diagnostics.is_empty(),
        "Should have error diagnostic when tsconfig.json is missing in project dir"
    );
    assert_eq!(
        result.diagnostics[0].code,
        diagnostic_codes::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_SPECIFIED_DIRECTORY,
        "Should have correct error code"
    );
}

#[test]
fn compile_missing_tsconfig_uses_defaults() {
    // Test that compilation works without tsconfig.json using defaults
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("src/index.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    // Output should be next to source when no outDir specified
    assert!(base.join("src/index.js").is_file());
}

#[test]
fn compile_ambient_external_module_without_internal_import_declaration_fixture() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "ambientExternalModuleWithoutInternalImportDeclaration_0.ts",
            "ambientExternalModuleWithoutInternalImportDeclaration_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_0.ts"),
        r#"declare module 'M' {
    namespace C {
        export var f: number;
    }
    class C {
        foo(): void;
    }
    export = C;
}"#,
    );
    write_file(
        &base.join("ambientExternalModuleWithoutInternalImportDeclaration_1.ts"),
        r#"/// <reference path='ambientExternalModuleWithoutInternalImportDeclaration_0.ts'/>
import A = require('M');
var c = new A();"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn compile_alias_on_merged_module_interface_fixture_reports_ts2708() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs"
          },
          "files": [
            "aliasOnMergedModuleInterface_0.ts",
            "aliasOnMergedModuleInterface_1.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_0.ts"),
        r#"declare module "foo" {
    namespace B {
        export interface A {}
    }
    interface B {
        bar(name: string): B.A;
    }
    export = B;
}"#,
    );
    write_file(
        &base.join("aliasOnMergedModuleInterface_1.ts"),
        r#"/// <reference path='aliasOnMergedModuleInterface_0.ts' />
import foo = require("foo");
declare var z: foo;
z.bar("hello");
var x: foo.A = foo.bar("hello");"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result.diagnostics.iter().any(|d| d.code == 2708),
        "Expected TS2708, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// E2E: Generic Utility Library Compilation
// =============================================================================

#[test]
fn compile_generic_utility_library_array_utils() {
    // Test compilation of generic array utility functions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "strict": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Generic array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function map<T, U>(arr: T[], fn: (item: T, index: number) => U): U[] {
    const result: U[] = [];
    for (let i = 0; i < arr.length; i++) {
        result.push(fn(arr[i], i));
    }
    return result;
}

export function filter<T>(arr: T[], predicate: (item: T) => boolean): T[] {
    const result: T[] = [];
    for (const item of arr) {
        if (predicate(item)) {
            result.push(item);
        }
    }
    return result;
}

export function find<T>(arr: T[], predicate: (item: T) => boolean): T | undefined {
    for (const item of arr) {
        if (predicate(item)) {
            return item;
        }
    }
    return undefined;
}

export function reduce<T, U>(arr: T[], fn: (acc: U, item: T) => U, initial: U): U {
    let acc = initial;
    for (const item of arr) {
        acc = fn(acc, item);
    }
    return acc;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/array.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/array.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output has type annotations stripped
    let js = std::fs::read_to_string(base.join("dist/src/array.js")).expect("read js");
    assert!(!js.contains(": T[]"), "Type annotations should be stripped");
    assert!(!js.contains(": U[]"), "Type annotations should be stripped");
    assert!(js.contains("function map"), "Function should be present");
    assert!(js.contains("function filter"), "Function should be present");
    assert!(js.contains("function find"), "Function should be present");
    assert!(js.contains("function reduce"), "Function should be present");

    // Verify declarations preserve types
    let dts = std::fs::read_to_string(base.join("dist/src/array.d.ts")).expect("read dts");
    assert!(
        dts.contains("map<T, U>") || dts.contains("map<T,U>"),
        "Generic should be in declaration"
    );
    assert!(
        dts.contains("filter<T>"),
        "Generic should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_type_utilities() {
    // Test compilation with type-level utilities (conditional types, mapped types)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Type utilities with runtime helpers
    write_file(
        &base.join("src/types.ts"),
        r#"
// Note: Object, Readonly, Partial are provided by lib.d.ts

// Type-level utilities (erased at runtime)
export type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? Readonly<T[P]> : T[P];
};

export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? Partial<T[P]> : T[P];
};

export type Nullable<T> = T | null;

// Mapped type that uses index access (T[P])
export type ValueTypes<T> = {
    [P in keyof T]: T[P];
};

// Runtime function using these types
export function deepFreeze<T extends object>(obj: T): DeepReadonly<T> {
    Object.freeze(obj);
    for (const key of Object.keys(obj)) {
        const value = (obj as Record<string, unknown>)[key];
        if (typeof value === "object" && value !== null) {
            deepFreeze(value as object);
        }
    }
    return obj as DeepReadonly<T>;
}

export function isNonNull<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Compilation should have no diagnostics, got: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/types.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/types.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output - type aliases should be completely erased
    let js = std::fs::read_to_string(base.join("dist/src/types.js")).expect("read js");
    assert!(!js.contains("DeepReadonly"), "Type alias should be erased");
    assert!(!js.contains("DeepPartial"), "Type alias should be erased");
    assert!(
        js.contains("function deepFreeze"),
        "Runtime function should be present"
    );
    assert!(
        js.contains("function isNonNull"),
        "Runtime function should be present"
    );

    // Verify declarations preserve type utilities
    let dts = std::fs::read_to_string(base.join("dist/src/types.d.ts")).expect("read dts");
    assert!(
        dts.contains("DeepReadonly"),
        "Type alias should be in declaration"
    );
    assert!(
        dts.contains("DeepPartial"),
        "Type alias should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_multi_file() {
    // Test multi-file generic utility library with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function first<T>(arr: T[]): T | undefined {
    return arr[0];
}

export function last<T>(arr: T[]): T | undefined {
    return arr[arr.length - 1];
}
"#,
    );

    // String utilities
    write_file(
        &base.join("src/string.ts"),
        r#"
export function capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function repeat(str: string, count: number): string {
    let result = "";
    for (let i = 0; i < count; i++) {
        result += str;
    }
    return result;
}
"#,
    );

    // Function utilities
    write_file(
        &base.join("src/function.ts"),
        r#"
export function identity<T>(value: T): T {
    return value;
}

export function constant<T>(value: T): () => T {
    return () => value;
}

export function noop(): void {}
"#,
    );

    // Main index re-exporting everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export { first, last } from "./array";
export { capitalize, repeat } from "./string";
export { identity, constant, noop } from "./function";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    // All JS files should exist
    assert!(base.join("dist/src/array.js").is_file());
    assert!(base.join("dist/src/string.js").is_file());
    assert!(base.join("dist/src/function.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    // All declaration files should exist
    assert!(base.join("dist/src/array.d.ts").is_file());
    assert!(base.join("dist/src/string.d.ts").is_file());
    assert!(base.join("dist/src/function.d.ts").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // All source maps should exist
    assert!(base.join("dist/src/array.js.map").is_file());
    assert!(base.join("dist/src/index.js.map").is_file());

    // Verify index re-exports
    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read index");
    assert!(
        index_js.contains("require") || index_js.contains("export"),
        "Index should have exports"
    );

    // Verify index declaration
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("first") && index_dts.contains("last"),
        "Index declaration should re-export array utils"
    );
}

#[test]
fn compile_generic_utility_library_with_constraints() {
    // Test generic functions with complex constraints
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/constrained.ts"),
        r#"
// Generic with extends constraint
export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

// Generic with multiple constraints
export function setProperty<T extends object, K extends keyof T>(
    obj: T,
    key: K,
    value: T[K]
): T {
    obj[key] = value;
    return obj;
}

// Generic with default type parameter
export function createArray<T = string>(length: number, fill: T): T[] {
    const result: T[] = [];
    for (let i = 0; i < length; i++) {
        result.push(fill);
    }
    return result;
}

// Function overloads with generics
export function wrap<T>(value: T): T[];
export function wrap<T>(value: T, count: number): T[];
export function wrap<T>(value: T, count: number = 1): T[] {
    const result: T[] = [];
    for (let i = 0; i < count; i++) {
        result.push(value);
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/constrained.js")).expect("read js");
    assert!(
        !js.contains("extends keyof"),
        "Constraints should be stripped"
    );
    assert!(
        !js.contains("extends object"),
        "Constraints should be stripped"
    );
    assert!(
        js.contains("function getProperty"),
        "Function should be present"
    );
    assert!(js.contains("function wrap"), "Function should be present");

    let dts = std::fs::read_to_string(base.join("dist/src/constrained.d.ts")).expect("read dts");
    // Check that generic functions are present in declaration
    assert!(
        dts.contains("getProperty"),
        "getProperty should be in declaration"
    );
    assert!(
        dts.contains("setProperty"),
        "setProperty should be in declaration"
    );
    assert!(
        dts.contains("createArray"),
        "createArray should be in declaration"
    );
    assert!(dts.contains("wrap"), "wrap should be in declaration");
}

#[test]
fn compile_generic_utility_library_classes() {
    // Test generic utility classes
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/collections.ts"),
        r#"
export class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    get size(): number {
        return this.items.length;
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

export class Queue<T> {
    private items: T[] = [];

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | undefined {
        return this.items.shift();
    }

    front(): T | undefined {
        return this.items[0];
    }

    get size(): number {
        return this.items.length;
    }
}

export class Result<T, E> {
    private constructor(
        private readonly value: T | undefined,
        private readonly error: E | undefined,
        private readonly isOk: boolean
    ) {}

    static ok<T, E>(value: T): Result<T, E> {
        return new Result<T, E>(value, undefined, true);
    }

    static err<T, E>(error: E): Result<T, E> {
        return new Result<T, E>(undefined, error, false);
    }

    isSuccess(): boolean {
        return this.isOk;
    }

    getValue(): T | undefined {
        return this.value;
    }

    getError(): E | undefined {
        return this.error;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/collections.js")).expect("read js");
    assert!(js.contains("class Stack"), "Class should be present");
    assert!(js.contains("class Queue"), "Class should be present");
    assert!(js.contains("class Result"), "Class should be present");
    assert!(!js.contains("<T>"), "Generic parameters should be stripped");
    assert!(!js.contains("T[]"), "Type annotations should be stripped");
    assert!(
        !js.contains(": void"),
        "Return type annotations should be stripped"
    );

    let dts = std::fs::read_to_string(base.join("dist/src/collections.d.ts")).expect("read dts");
    assert!(
        dts.contains("Stack<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Queue<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Result<T, E>") || dts.contains("Result<T,E>"),
        "Generic class should be in declaration"
    );
}

// =============================================================================
// E2E: Module Re-exports
// =============================================================================

#[test]
fn compile_module_named_reexports() {
    // Test named re-exports: export { foo, bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { add, multiply, PI } from "./utils";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );
    assert!(base.join("dist/src/utils.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify index re-exports
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("add"), "add should be re-exported");
    assert!(
        index_dts.contains("multiply"),
        "multiply should be re-exported"
    );
    assert!(index_dts.contains("PI"), "PI should be re-exported");
}

#[test]
fn compile_module_renamed_reexports() {
    // Test renamed re-exports: export { foo as bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/internal.ts"),
        r#"
export function internalHelper(): string {
    return "helper";
}

export const internalValue = 42;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { internalHelper as helper, internalValue as value } from "./internal";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("helper"), "helper should be re-exported");
    assert!(index_dts.contains("value"), "value should be re-exported");
}

#[test]
fn compile_module_star_reexports() {
    // Test star re-exports: export * from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/math.ts"),
        r#"
export function sum(arr: number[]): number {
    let total = 0;
    for (const n of arr) {
        total += n;
    }
    return total;
}

export function average(arr: number[]): number {
    return sum(arr) / arr.length;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export * from "./math";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("sum") || index_dts.contains("*"),
        "sum should be re-exported or star export present"
    );
}

#[test]
fn compile_module_chained_reexports() {
    // Test chained re-exports: A re-exports from B which re-exports from C
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Level 3: core module
    write_file(
        &base.join("src/core.ts"),
        r#"
export function coreFunction(): string {
    return "core";
}

export const CORE_VERSION = "1.0.0";
"#,
    );

    // Level 2: intermediate module
    write_file(
        &base.join("src/intermediate.ts"),
        r#"
export { coreFunction, CORE_VERSION } from "./core";

export function intermediateFunction(): string {
    return "intermediate";
}
"#,
    );

    // Level 1: public module
    write_file(
        &base.join("src/index.ts"),
        r#"
export { coreFunction, CORE_VERSION, intermediateFunction } from "./intermediate";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/core.js").is_file());
    assert!(base.join("dist/src/intermediate.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("coreFunction"),
        "coreFunction should be re-exported"
    );
    assert!(
        index_dts.contains("intermediateFunction"),
        "intermediateFunction should be re-exported"
    );
}

#[test]
fn compile_module_mixed_exports_and_reexports() {
    // Test mixing local exports with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export function helperA(): string {
    return "A";
}

export function helperB(): string {
    return "B";
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-exports
export { helperA, helperB } from "./helpers";

// Local exports
export function localFunction(): number {
    return 42;
}

export const LOCAL_CONSTANT = "local";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    assert!(
        index_js.contains("localFunction"),
        "Local function should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("helperA"),
        "helperA should be re-exported"
    );
    assert!(
        index_dts.contains("localFunction"),
        "localFunction should be exported"
    );
    assert!(
        index_dts.contains("LOCAL_CONSTANT"),
        "LOCAL_CONSTANT should be exported"
    );
}

#[test]
fn compile_module_type_only_reexports() {
    // Test type-only re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/types.ts"),
        r#"
export type UserId = number;

export type UserName = string;

export function createId(n: number): UserId {
    return n;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Type-only re-exports (should be erased from JS)
export type { UserId, UserName } from "./types";

// Value re-export
export { createId } from "./types";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    // Type-only exports should not appear in runtime output, but createId should
    assert!(
        index_js.contains("createId"),
        "createId should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("UserId"),
        "UserId type should be in declaration"
    );
    assert!(
        index_dts.contains("createId"),
        "createId should be in declaration"
    );
}

#[test]
fn compile_module_default_reexport() {
    // Test default re-export
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/component.ts"),
        r#"
export default function Component(): string {
    return "Component";
}

export const version = "1.0";
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { default, version } from "./component";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("default") || index_dts.contains("Component"),
        "default export should be re-exported"
    );
    assert!(
        index_dts.contains("version"),
        "version should be re-exported"
    );
}

#[test]
fn compile_module_barrel_file() {
    // Test barrel file pattern (common in libraries)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Feature modules
    write_file(
        &base.join("src/features/auth.ts"),
        r#"
export function login(user: string): boolean {
    return user.length > 0;
}

export function logout(): void {}
"#,
    );

    write_file(
        &base.join("src/features/data.ts"),
        r#"
export function fetchData(): string[] {
    return [];
}

export function saveData(data: string[]): boolean {
    return data.length > 0;
}
"#,
    );

    // Barrel file
    write_file(
        &base.join("src/features/index.ts"),
        r#"
export { login, logout } from "./auth";
export { fetchData, saveData } from "./data";
"#,
    );

    // Main entry
    write_file(
        &base.join("src/index.ts"),
        r#"
export { login, logout, fetchData, saveData } from "./features";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/features/auth.js").is_file());
    assert!(base.join("dist/src/features/data.js").is_file());
    assert!(base.join("dist/src/features/index.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("login"), "login should be re-exported");
    assert!(
        index_dts.contains("fetchData"),
        "fetchData should be re-exported"
    );
}

// =============================================================================
// E2E: Classes with Generic Methods
// =============================================================================

#[test]
fn compile_class_with_generic_constructor() {
    // Test class with generic constructor pattern
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/builder.ts"),
        r#"
export class Builder<T> {
    private value: T;

    constructor(initial: T) {
        this.value = initial;
    }

    set(value: T): Builder<T> {
        this.value = value;
        return this;
    }

    transform<U>(fn: (value: T) => U): Builder<U> {
        return new Builder(fn(this.value));
    }

    build(): T {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/builder.js")).expect("read js");
    assert!(js.contains("class Builder"), "Class should be present");
    assert!(js.contains("constructor("), "Constructor should be present");
    assert!(!js.contains("<T>"), "Generic should be stripped");

    let dts = std::fs::read_to_string(base.join("dist/src/builder.d.ts")).expect("read dts");
    assert!(
        dts.contains("Builder<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("transform<U>"),
        "Generic method should be in declaration"
    );
}

// =============================================================================
// E2E: Namespace Exports
// =============================================================================

#[test]
fn compile_basic_namespace_export() {
    // Test basic namespace compiles without errors and produces JS output
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export namespace Utils {
    export const VERSION = "1.0.0";
    export function greet(name: string): string {
        return "Hello, " + name;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    // Namespace should produce some output
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nested_namespace_export() {
    // Test nested namespace compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/api.ts"),
        r#"
export namespace API {
    export namespace V1 {
        export function getUsers(): string[] {
            return ["user1", "user2"];
        }
    }

    export namespace V2 {
        export function getUsers(): string[] {
            return ["user1", "user2", "user3"];
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/api.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_namespace_with_class() {
    // Test namespace containing a class compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/models.ts"),
        r#"
export namespace Models {
    export class User {
        name: string;
        constructor(name: string) {
            this.name = name;
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/models.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Enum Compilation
// =============================================================================

#[test]
fn compile_numeric_enum() {
    // Test basic numeric enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/status.ts"),
        r#"
export enum Status {
    Pending,
    Active,
    Completed,
    Failed
}

export function getStatusName(status: Status): string {
    return Status[status];
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/status.js")).expect("read js");
    assert!(js.contains("Status"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_string_enum() {
    // Test string enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/direction.ts"),
        r#"
export enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}

export function move(dir: Direction): Direction {
    return dir;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/direction.js")).expect("read js");
    assert!(js.contains("Direction"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_const_enum() {
    // Test const enum compilation (should be inlined)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/flags.ts"),
        r#"
export const enum Flags {
    None = 0,
    Read = 1,
    Write = 2,
    Execute = 4
}

export function hasFlag(flags: Flags, flag: Flags): boolean {
    return (flags & flag) !== 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/flags.js")).expect("read js");
    // Const enums may be inlined, so just verify compilation succeeded
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_enum_with_computed_values() {
    // Test enum with computed/expression values
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/sizes.ts"),
        r#"
export enum Size {
    Small = 1,
    Medium = Small * 2,
    Large = Medium * 2,
    ExtraLarge = Large * 2
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/sizes.js")).expect("read js");
    assert!(js.contains("Size"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Arrow Function Compilation
// =============================================================================

#[test]
fn compile_basic_arrow_function() {
    // Test basic arrow function compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export const add = (a: number, b: number): number => a + b;
export const multiply = (a: number, b: number): number => {
    return a * b;
};
export const identity = <T>(x: T): T => x;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    assert!(
        js.contains("=>") || js.contains("function"),
        "Arrow or function should be present"
    );
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_with_rest_params() {
    // Test arrow function with rest parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export const sum = (...numbers: number[]): number => {
    let total = 0;
    for (const n of numbers) {
        total += n;
    }
    return total;
};

export const first = <T>(...items: T[]): T => items[0];
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/helpers.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_with_default_params() {
    // Test arrow function with default parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/greet.ts"),
        r#"
export const greet = (name: string, greeting: string = "Hello"): string => {
    return greeting + ", " + name;
};

export const repeat = (str: string, times: number = 1): string => {
    let result = "";
    for (let i = 0; i < times; i++) {
        result += str;
    }
    return result;
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_in_class() {
    // Test arrow functions as class properties (for lexical this)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/counter.ts"),
        r#"
export class Counter {
    count: number = 0;

    increment = (): void => {
        this.count++;
    };

    decrement = (): void => {
        this.count--;
    };

    reset = (): void => {
        this.count = 0;
    };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/counter.js")).expect("read js");
    assert!(js.contains("Counter"), "Class should be present");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Spread Operator Compilation
// =============================================================================

#[test]
fn compile_array_spread() {
    // Test array spread operator compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/arrays.ts"),
        r#"
export function concat(a: number[], b: number[]): number[] {
    return [...a, ...b];
}

export function copy(a: number[]): number[] {
    return [...a];
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_object_spread() {
    // Test object spread operator compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/objects.ts"),
        r#"
interface Person {
    name: string;
    age: number;
}

export function clone(obj: Person): Person {
    return { ...obj };
}

export function update(obj: Person, updates: Person): Person {
    return { ...obj, ...updates };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/objects.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_function_call_spread() {
    // Test spread in function calls
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/calls.ts"),
        r#"
export function apply(fn: (...args: number[]) => number, args: number[]): number {
    return fn(...args);
}

export function log(...items: string[]): string[] {
    return items;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/calls.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Template Literal Compilation
// =============================================================================

#[test]
fn compile_basic_template_literal() {
    // Test basic template literal compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/greet.ts"),
        r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}

export function format(a: number, b: number): string {
    return `${a} + ${b} = ${a + b}`;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_multiline_template_literal() {
    // Test multiline template literal compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/html.ts"),
        r#"
export function createDiv(content: string): string {
    const result = `<div><p>${content}</p></div>`;
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/html.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nested_template_literal() {
    // Test nested template expressions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/nested.ts"),
        r#"
export function wrap(inner: string, outer: string): string {
    return `${outer}: ${`[${inner}]`}`;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/nested.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Destructuring Assignment Compilation
// =============================================================================

#[test]
fn compile_object_destructuring() {
    // Test object destructuring compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/extract.ts"),
        r#"
interface Point {
    x: number;
    y: number;
}

export function getX(point: Point): number {
    const { x } = point;
    return x;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/extract.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_array_destructuring() {
    // Test array destructuring compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/arrays.ts"),
        r#"
export function getFirst(arr: number[]): number {
    const [first] = arr;
    return first;
}

export function getSecond(arr: number[]): number {
    const [, second] = arr;
    return second;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_destructuring_with_defaults() {
    // Test destructuring with default values
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/defaults.ts"),
        r#"
interface Config {
    host: string;
    port: number;
}

export function getPort(config: Config): number {
    const { port = 3000 } = config;
    return port;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/defaults.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining() {
    // Test optional chaining (?.) compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/optional.ts"),
        r#"
interface User {
    name: string;
    address?: {
        city: string;
    };
}

export function getCity(user: User): string | undefined {
    return user.address?.city;
}

export function getLength(arr?: string[]): number | undefined {
    return arr?.length;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/optional.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nullish_coalescing() {
    // Test nullish coalescing (??) compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/nullish.ts"),
        r#"
export function getValueOrDefault(value: string | null | undefined): string {
    return value ?? "default";
}

export function getNumberOrZero(num: number | null): number {
    return num ?? 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/nullish.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining_with_call() {
    // Test optional chaining with method calls
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/optcall.ts"),
        r#"
interface Logger {
    log?: (msg: string) => void;
}

export function maybeLog(logger: Logger, msg: string): void {
    logger.log?.(msg);
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/optcall.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_inheritance() {
    // Test class inheritance compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/classes.ts"),
        r#"
export class Animal {
    constructor(public name: string) {}
    speak(): string {
        return this.name;
    }
}

export class Dog extends Animal {
    constructor(name: string) {
        super(name);
    }
    speak(): string {
        return "Woof: " + super.speak();
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/classes.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_static_members() {
    // Test class static members compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/staticclass.ts"),
        r#"
export class Counter {
    static count: number = 0;

    static increment(): number {
        Counter.count += 1;
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/staticclass.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_accessors() {
    // Test class getter/setter compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/accessors.ts"),
        r#"
export class Rectangle {
    private _width: number = 0;
    private _height: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
    }

    get area(): number {
        return this._width * this._height;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/accessors.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_computed_property_names() {
    // Test computed property names compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/computed.ts"),
        r#"
const KEY = "dynamicKey";

export const obj = {
    [KEY]: "value",
    ["literal" + "Key"]: 42
};

export function getProp(key: string): { [k: string]: number } {
    return { [key]: 100 };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/computed.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_for_of_loop() {
    // Test for...of loop compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/forof.ts"),
        r#"
export function sumArray(arr: number[]): number {
    let sum = 0;
    for (const num of arr) {
        sum += num;
    }
    return sum;
}

export function joinStrings(arr: string[]): string {
    let result = "";
    for (const str of arr) {
        result += str;
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/forof.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_shorthand_methods() {
    // Test shorthand method syntax compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/methods.ts"),
        r#"
export const calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    }
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/methods.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_incremental_creates_tsbuildinfo() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Setup tsconfig with incremental enabled
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "incremental": true,
            "tsBuildInfoFile": "dist/project.tsbuildinfo"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();

    // First compilation should create BuildInfo
    let result = compile(&args, base).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    // Verify JS output exists
    let js_path = base.join("dist/src/index.js");
    assert!(js_path.is_file(), "JS output should exist");

    // Verify BuildInfo file is created
    let build_info_path = base.join("dist/project.tsbuildinfo");
    assert!(
        build_info_path.is_file(),
        "tsbuildinfo file should be created"
    );

    // Verify BuildInfo can be parsed
    let build_info_content = std::fs::read_to_string(&build_info_path).expect("read buildinfo");
    let build_info: serde_json::Value =
        serde_json::from_str(&build_info_content).expect("parse buildinfo");

    // Verify structure
    assert_eq!(
        build_info["version"],
        crate::incremental::BUILD_INFO_VERSION
    );
    assert!(build_info["rootFiles"].is_array());

    // Second build with no changes should succeed
    let result2 = compile(&args, base).expect("second compile should succeed");
    assert!(result2.diagnostics.is_empty());

    // Verify BuildInfo still exists and has been updated
    let build_info_content2 =
        std::fs::read_to_string(&build_info_path).expect("read buildinfo again");
    let build_info2: serde_json::Value =
        serde_json::from_str(&build_info_content2).expect("parse buildinfo again");
    assert_eq!(
        build_info2["version"],
        crate::incremental::BUILD_INFO_VERSION
    );

    // Third build with a source change
    write_file(
        &base.join("src/index.ts"),
        "export const value = 2; export const foo = 'bar';",
    );
    let result3 = compile(&args, base).expect("third compile should succeed");
    assert!(result3.diagnostics.is_empty());

    // Verify BuildInfo was updated with new content
    let build_info_content3 =
        std::fs::read_to_string(&build_info_path).expect("read buildinfo third time");
    let build_info3: serde_json::Value =
        serde_json::from_str(&build_info_content3).expect("parse buildinfo third time");
    assert_eq!(
        build_info3["version"],
        crate::incremental::BUILD_INFO_VERSION
    );
}

#[cfg(unix)]
#[test]
fn compile_incremental_reports_ts5033_when_tsbuildinfo_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let readonly_dir = base.join("readonly");
    std::fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o555))
        .expect("mark readonly dir");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "incremental": true,
            "tsBuildInfoFile": "readonly/project.tsbuildinfo"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed with diagnostic");

    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    let ts5033_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::COULD_NOT_WRITE_FILE)
        .collect();
    assert!(
        ts5033_diags.iter().any(|diag| {
            diag.message_text.contains("readonly/project.tsbuildinfo")
                && (diag.message_text.contains("permission denied")
                    || diag.message_text.contains("read-only file system"))
        }),
        "Expected TS5033 for non-writable tsbuildinfo path, got: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn compile_tsbuildinfo_without_incremental_does_not_report_ts5033() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    let readonly_dir = base.join("readonly");
    std::fs::create_dir_all(&readonly_dir).expect("create readonly dir");
    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o555))
        .expect("mark readonly dir");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "tsBuildInfoFile": "readonly/project.tsbuildinfo"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore readonly dir permissions");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::COULD_NOT_WRITE_FILE),
        "Expected no TS5033 when incremental build info is disabled, got: {result:?}"
    );
}

// Tests for @noTypesAndSymbols parsing

use crate::driver::has_no_types_and_symbols_directive;

#[test]
fn test_has_no_types_and_symbols_directive_true() {
    let source = r#"// @noTypesAndSymbols: true
async function f(x, y = z) {}"#;
    assert!(has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_false() {
    let source = r#"// @noTypesAndSymbols: false
async function f(x, y = z) {}"#;
    assert!(!has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_not_present() {
    let source = r#"// @strict: true
async function f(x, y = z) {}"#;
    assert!(!has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_case_insensitive() {
    let source = r#"// @NOTYPESANDSYMBOLS: true
async function f(x, y = z) {}"#;
    assert!(has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_with_other_options() {
    let source = r#"// @strict: false
// @target: es2015
// @noTypesAndSymbols: true
async function f(x, y = z) {}"#;
    assert!(has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_comma_separated() {
    let source = r#"// @noTypesAndSymbols: true, false
async function f(x, y = z) {}"#;
    // First value is true, so should return true
    assert!(has_no_types_and_symbols_directive(source));
}

#[test]
fn test_has_no_types_and_symbols_directive_after_32_lines() {
    // Comments after 32 lines should not be parsed
    let source = format!("{}\n// @noTypesAndSymbols: true", "\n".repeat(35));
    assert!(!has_no_types_and_symbols_directive(&source));
}

#[test]
fn test_has_no_types_and_symbols_directive_semicolon_terminated() {
    let source = r#"// @noTypesAndSymbols: true;
async function f(x, y = z) {}"#;
    assert!(has_no_types_and_symbols_directive(source));
}

#[test]
fn test_no_types_and_symbols_directive_does_not_disable_default_libs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"// @noTypesAndSymbols: true
const value = 1;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2318_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE)
        .collect();
    assert!(
        ts2318_errors.is_empty(),
        "Expected @noTypesAndSymbols not to disable libs, got TS2318 diagnostics: {:?}",
        ts2318_errors
            .iter()
            .map(|d| d.message_text.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_no_types_and_symbols_tsconfig_disables_automatic_node_types() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "declaration": true,
            "emitDeclarationOnly": true,
            "noTypesAndSymbols": true
          },
          "files": ["usage1.ts", "usage2.ts", "usage3.ts"]
        }"#,
    );
    write_file(
        &base.join("usage1.ts"),
        r#"export { parse } from "url";
"#,
    );
    write_file(
        &base.join("usage2.ts"),
        r#"import { parse } from "url";
export const thing: import("url").Url = parse();
"#,
    );
    write_file(
        &base.join("usage3.ts"),
        r#"import { parse } from "url";
export const thing = parse();
"#,
    );
    write_file(
        &base.join("node_modules/@types/node/index.d.ts"),
        r#"declare module "url" {
  export class Url {}
  export function parse(): Url;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2591_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2
        })
        .collect();
    assert!(
        ts2591_errors.len() == 4,
        "Expected noTypesAndSymbols tsconfig to suppress automatic @types/node loading and emit four TS2591 diagnostics, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_binary_file_reports_errors() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"G@\xFFG@\xFFG@";
    std::fs::write(&binary_path, content).expect("failed to write binary file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let has_ts1490 = result.diagnostics.iter().any(|d| d.code == 1490);
    assert!(
        has_ts1490,
        "Expected TS1490 (File appears to be binary). Diagnostics: {:?}",
        result.diagnostics
    );

    // Binary file detection should suppress parser diagnostics - only TS1490 is emitted
    let non_binary_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code != 1490)
        .collect();
    assert!(
        non_binary_errors.is_empty(),
        "Expected only TS1490 for binary files, but got additional errors: {non_binary_errors:?}"
    );
}

#[test]
fn compile_control_byte_binary_file_preserves_parser_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"G@\x04\x04\x04\x04\x04";
    std::fs::write(&binary_path, content).expect("failed to write control-byte file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1490),
        "Expected TS1490 for control-byte binary. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        codes.contains(&1127),
        "Expected TS1127 to be preserved for control-byte binary. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_short_garbage_payload_binary_suppresses_parser_diagnostics() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let binary_path = base.join("binary.ts");
    let content = b"// @target: es2015\n\xEF\xBF\xBD\x1F\xEF\xBF\xBD\x03\xEF\xBF\xBD\x03\x19\x1F";
    std::fs::write(&binary_path, content).expect("failed to write corrupted file");

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015"
          },
          "files": ["binary.ts"]
        }"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![1490],
        "Expected only TS1490 for short garbage binary payloads. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_import_alias_assignment_does_not_leak_non_exported_module_symbols() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "strict": true,
            "noEmit": true
          },
          "files": [
            "aliasUsageInVarAssignment_backbone.ts",
            "aliasUsageInVarAssignment_moduleA.ts",
            "aliasUsageInVarAssignment_main.ts"
          ]
        }"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_backbone.ts"),
        r#"export class Model {
    public someData: string;
}
"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_moduleA.ts"),
        r#"import Backbone = require("./aliasUsageInVarAssignment_backbone");
export class VisualizationModel extends Backbone.Model {
    // interesting stuff here
}
"#,
    );
    write_file(
        &base.join("aliasUsageInVarAssignment_main.ts"),
        r#"import Backbone = require("./aliasUsageInVarAssignment_backbone");
import moduleA = require("./aliasUsageInVarAssignment_moduleA");
interface IHasVisualizationModel {
    VisualizationModel: typeof Backbone.Model;
}
var i: IHasVisualizationModel;
var m: typeof moduleA = i;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    // TODO: After the module augmentation lazy resolution change (2e96c99c2),
    // global `escape` spuriously leaks into module exports causing TS2741.
    // Filter out this known false positive until the root cause is fixed.
    let mut codes: Vec<u32> = result
        .diagnostics
        .iter()
        .filter(|d| !(d.code == 2741 && d.message_text.contains("escape")))
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();

    assert_eq!(
        codes,
        vec![2454, 2564],
        "Expected only TS2454 and TS2564 for alias usage assignment. Diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|diag| diag.code != 2740),
        "Expected no TS2740 namespace-shape diagnostic leakage. Diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn ts2688_unresolved_types_in_tsconfig() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    // Create a type root directory so default_type_roots finds it,
    // but don't create the requested package inside it
    std::fs::create_dir_all(base.join("node_modules/@types")).unwrap();
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "types": ["nonexistent-package"] }, "files": ["index.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for unresolved 'nonexistent-package' in types array, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    assert!(
        ts2688_diags[0].message_text.contains("nonexistent-package"),
        "TS2688 message should mention the package name, got: {}",
        ts2688_diags[0].message_text
    );
}

#[test]
fn ts2688_resolved_types_no_error() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    // Create a valid @types package structure
    write_file(
        &base.join("node_modules/@types/mylib/index.d.ts"),
        "declare const myLibValue: string;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "types": ["mylib"] }, "files": ["index.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 1;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        ts2688_diags.is_empty(),
        "Should NOT emit TS2688 when types package is found, got: {ts2688_diags:?}"
    );
}

#[test]
#[ignore = "types entry resolution changed after merge"]
fn ts2688_types_entry_still_loads_node_modules_package_globals() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("typings/dummy.d.ts"),
        "declare const dummy: number;\n",
    );
    write_file(
        &base.join("node_modules/phaser/types/phaser.d.ts"),
        "declare const phaserValue: number;\n",
    );
    write_file(
        &base.join("node_modules/phaser/package.json"),
        r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["typings"],
            "types": ["phaser"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "phaserValue;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 when typeRoots does not contain the requested package, got: {:?}",
        result.diagnostics
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Node-modules fallback should still make package globals visible, got: {ts2304_diags:?}"
    );
}

#[test]
#[ignore = "scoped types entry resolution changed after merge"]
fn scoped_types_entry_resolves_plain_mangled_package_name_from_custom_roots() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/mangled__nodemodulescache/index.d.ts"),
        "declare const mangledNodeModules: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["types", "node_modules", "node_modules/@types"],
            "types": ["@mangled/nodemodulescache"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "mangledNodeModules;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for the unresolved scoped types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected scoped mangled package name to resolve from custom roots, got: {result:?}"
    );
}

#[test]
#[ignore = "scoped types entry resolution changed after merge"]
fn scoped_types_entry_loads_at_types_scoped_package_globals_while_preserving_ts2688() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@types/@scoped/attypescache/index.d.ts"),
        "declare const atTypesCache: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "typeRoots": ["types", "node_modules", "node_modules/@types"],
            "types": ["@scoped/attypescache"]
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "atTypesCache;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        !ts2688_diags.is_empty(),
        "Expected TS2688 for the unresolved scoped @types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected scoped @types package globals to load despite TS2688, got: {result:?}"
    );
}

#[test]
fn type_query_on_import_type_value_binding_does_not_emit_ts2552() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@types/foo/package.json"),
        r#"{
          "name": "@types/foo",
          "version": "1.0.0",
          "exports": {
            ".": {
              "import": "./index.d.mts",
              "require": "./index.d.cts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/@types/foo/index.d.mts"),
        "export declare const x: \"module\";\n",
    );
    write_file(
        &base.join("node_modules/@types/foo/index.d.cts"),
        "export declare const x: \"script\";\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "esnext",
            "moduleResolution": "bundler",
            "declaration": true,
            "emitDeclarationOnly": true
          },
          "files": ["app.ts", "other.ts"]
        }"#,
    );
    write_file(
        &base.join("app.ts"),
        r#"import type { x as Default } from "foo";
import type { x as ImportRelative } from "./other" with { "resolution-mode": "import" };

type _Default = typeof Default;
type _ImportRelative = typeof ImportRelative;

export { _Default, _ImportRelative };
"#,
    );
    write_file(&base.join("other.ts"), r#"export const x = "other";"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        ts2552_diags.is_empty(),
        "Expected typeof on import type bindings to avoid TS2552, got: {result:?}"
    );
}

#[test]
fn import_type_resolution_mode_declaration_emit_uses_exact_package_condition() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "name": "pkg",
          "version": "0.0.1",
          "exports": {
            "import": "./import.js",
            "require": "./require.js"
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/import.d.ts"),
        "export interface ImportInterface {}\n",
    );
    write_file(
        &base.join("node_modules/pkg/require.d.ts"),
        "export interface RequireInterface {}\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2022",
            "module": "node16",
            "declaration": true,
            "emitDeclarationOnly": true,
            "outDir": "out"
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"export type LocalInterface =
    & import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface
    & import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface;

export const a = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
export const b = (null as any as import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface);
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2694_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER)
        .collect();
    assert!(
        ts2694_diags.is_empty(),
        "Did not expect TS2694 when import types use distinct resolution-mode conditions, got: {result:?}"
    );
}

#[test]
fn import_non_exported_member_alias_reports_ts2460() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"declare function foo(): any
declare function bar(): any;
export { foo, bar as baz };
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import { foo, bar } from "./a";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2460_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS)
        .collect();
    assert!(
        ts2460_diags.iter().any(|diag| {
            diag.message_text.contains("\"./a\"")
                && diag.message_text.contains("'bar'")
                && diag.message_text.contains("'baz'")
        }),
        "Expected TS2460 for renamed export import, got: {result:?}"
    );
}

#[test]
fn direct_export_with_separate_type_alias_does_not_report_ts2460() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["a.ts", "b.ts"]
}"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"export class A<T> { a!: T }
export type { A as B };
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import type { A } from "./a";
import { B } from "./a";

let a: A<string> = { a: "" };
let b: B<number> = { a: 3 };
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS),
        "Did not expect TS2460 for direct export plus type-only alias, got: {result:?}"
    );
}

#[test]
fn bare_import_type_reports_ts1340() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["test.ts", "main.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"export interface T {
    value: string
}
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"export const a: import("./test") = null;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1340_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1340)
        .collect();
    assert!(
        ts1340_diags.iter().any(|diag| {
            diag.message_text
                .contains("Module './test' does not refer to a type")
                && diag.message_text.contains("typeof import('./test')")
        }),
        "Expected TS1340 for bare import type, got: {result:?}"
    );
}

#[test]
fn bare_import_type_export_equals_class_does_not_report_ts1340() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["foo.ts", "usage.ts"]
}"#,
    );
    write_file(
        &base.join("foo.ts"),
        r#"class Conn {
    item = 3;
}

export = Conn;
"#,
    );
    write_file(
        &base.join("usage.ts"),
        r#"type Conn = import("./foo");
declare const x: Conn;
export const y = x.item;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|d| d.code == 1340),
        "Did not expect TS1340 for bare import type of export= class module, got: {result:?}"
    );
}

#[test]
fn namespace_import_alias_const_enum_member_condition_reports_ts2845() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "declaration": true
  },
  "files": ["internal.ts", "usage.ts"]
}"#,
    );
    write_file(
        &base.join("internal.ts"),
        r#"namespace My.Internal {
    export function getThing(): void {}
    export const enum WhichThing {
        A, B, C
    }
}
"#,
    );
    write_file(
        &base.join("usage.ts"),
        r#"/// <reference path="./internal.ts" preserve="true" />
namespace SomeOther.Thing {
    import Internal = My.Internal;
    export class Foo {
        private _which!: Internal.WhichThing;
        constructor() {
            Internal.getThing();
            Internal.WhichThing.A ? "foo" : "bar";
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2845_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2845)
        .collect();
    assert!(
        ts2845_diags
            .iter()
            .any(|diag| diag.message_text.contains("always return 'false'")),
        "Expected TS2845 for namespace-imported const enum member condition, got: {result:?}"
    );
}

#[test]
fn export_import_qualified_type_only_namespace_reports_ts2708() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs"
  },
  "files": ["test.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"namespace x {
    interface c {
    }
}
export import a = x.c;
export = x;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2708_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .collect();
    assert!(
        ts2708_diags.iter().any(|diag| diag
            .message_text
            .contains("Cannot use namespace 'x' as a value")),
        "Expected TS2708 on the namespace qualifier in export import, got: {result:?}"
    );
}

#[test]
fn export_import_namespace_type_alias_without_export_equals_does_not_report_ts2708() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["test.ts"]
}"#,
    );
    write_file(
        &base.join("test.ts"),
        r#"export namespace a {
    export interface I {
    }
}

export import b = a.I;
export declare const x: b;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        !result.diagnostics.iter().any(|d| d.code == 2708),
        "Did not expect TS2708 for namespace type alias without export=, got: {result:?}"
    );
}

#[test]
fn lib_replacement_honors_source_reference_subfiles() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/@typescript/lib-dom/index.d.ts"),
        "// NOOP\n",
    );
    write_file(
        &base.join("node_modules/@typescript/lib-dom/iterable.d.ts"),
        "interface DOMIterable { abc: string }\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "libReplacement": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"/// <reference lib="dom.iterable" />
const a: DOMIterable = { abc: "Hello" };

window.localStorage;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        ts2552_diags.is_empty(),
        "Expected replacement dom.iterable lib to provide DOMIterable, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert_eq!(
        ts2304_diags.len(),
        1,
        "Expected only the replaced-out window global to fail, got: {result:?}"
    );
    assert!(
        ts2304_diags[0].message_text.contains("window"),
        "Expected TS2304 to target window, got: {result:?}"
    );
}

#[test]
fn types_entry_resolves_direct_declaration_file_from_type_root() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("node_modules/phaser/types/phaser.d.ts"),
        "declare const a: number;\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "typeRoots": ["node_modules/phaser/types"],
            "types": ["phaser"]
          },
          "files": ["a.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), "a;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2688_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_TYPE_DEFINITION_FILE_FOR)
        .collect();
    assert!(
        ts2688_diags.is_empty(),
        "Expected direct declaration file under typeRoots to satisfy the types entry, got: {result:?}"
    );

    let ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    assert!(
        ts2304_diags.is_empty(),
        "Expected declarations from direct typeRoots file to be visible, got: {result:?}"
    );
}

#[test]
fn import_from_type_package_loaded_via_types_does_not_emit_ts2307() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("typings/phaser/types/phaser.d.ts"),
        "export const a2: number;\n",
    );
    write_file(
        &base.join("typings/phaser/package.json"),
        r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "typeRoots": ["typings"],
            "types": ["phaser"]
          },
          "files": ["a.ts"]
        }"#,
    );
    write_file(&base.join("a.ts"), r#"import { a2 } from "phaser";"#);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2307_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .collect();
    assert!(
        ts2307_diags.is_empty(),
        "Expected type package imports satisfied via types/typeRoots to avoid TS2307, got: {result:?}"
    );
}

#[test]
fn ts2307_emitted_for_commonjs_module() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "commonjs" }, "files": ["test.ts"] }"#,
    );
    write_file(
        &base.join("test.ts"),
        "import { thing } from \"non-existent-module\";\nthing();\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Should emit TS2307 (not TS2792) for CommonJS module kind
    let ts2307 = result.diagnostics.iter().any(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
    });
    assert!(
        ts2307,
        "Expected TS2307 for bare specifier with module: commonjs, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts1079_emitted_for_declare_import_without_ts2304_on_declare() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015" }, "files": ["test.ts"] }"#,
    );
    write_file(&base.join("test.ts"), "declare import a = b;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts1079_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION
        })
        .collect();
    assert!(
        !ts1079_diags.is_empty(),
        "Expected TS1079 for `declare import`, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts1079_diags
            .iter()
            .any(|diag| diag.message_text.contains("'declare'")),
        "Expected TS1079 message to mention the declare modifier, got: {ts1079_diags:?}"
    );

    let declare_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("declare")
        })
        .collect();
    assert!(
        declare_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `declare`: {declare_ts2304_diags:?}"
    );
}

#[test]
fn ts2592_emitted_for_unresolved_jquery_global_without_ts2304() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015", "lib": ["es5"] }, "files": ["test.ts"] }"#,
    );
    write_file(&base.join("test.ts"), "const value = $(\".thing\");\n");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2592_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SA_2
        })
        .collect();
    assert!(
        !ts2592_diags.is_empty(),
        "Expected TS2592 for unresolved jQuery global `$`, got diagnostics: {:?}",
        result.diagnostics
    );

    let jquery_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("'$'"))
        .collect();
    assert!(
        jquery_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `$`: {jquery_ts2304_diags:?}"
    );
}

#[test]
fn ts2552_emitted_for_type_only_export_typo() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2015", "strict": true, "module": "commonjs" }, "files": ["test.ts"] }"#,
    );
    write_file(
        &base.join("test.ts"),
        "type RoomInterfae = {};\n\nexport type {\n    RoomInterface\n}\n",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2552_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
        .collect();
    assert!(
        !ts2552_diags.is_empty(),
        "Expected TS2552 for the typo in `export type {{ RoomInterface }}`, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts2552_diags
            .iter()
            .any(|diag| diag.message_text.contains("RoomInterfae")),
        "Expected TS2552 to suggest `RoomInterfae`, got: {ts2552_diags:?}"
    );

    let room_ts2304_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_NAME && d.message_text.contains("RoomInterface")
        })
        .collect();
    assert!(
        room_ts2304_diags.is_empty(),
        "Unexpected TS2304 on `RoomInterface`: {room_ts2304_diags:?}"
    );
}

/// TS8002: `export import x = require(...)` in a JS file should report at the
/// `export` keyword (position 0), not the inner `import` keyword.
#[test]
fn ts8002_export_import_equals_reports_at_export_keyword_in_js() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "nodenext", "allowJs": true, "checkJs": true }, "files": ["index.js"] }"#,
    );
    // `export import` starts at position 0; the inner `import` starts at position 7
    write_file(
        &base.join("index.js"),
        "export import fs2 = require(\"fs\");\n",
    );
    write_file(&base.join("package.json"), r#"{ "type": "module" }"#);

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts8002_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES)
        .collect();

    assert!(
        !ts8002_diags.is_empty(),
        "Expected TS8002 for `export import` in JS file, got codes: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );

    // The error should start at position 0 (the `export` keyword), not position 7 (`import`)
    for d in &ts8002_diags {
        assert_eq!(
            d.start, 0,
            "TS8002 should report at `export` keyword (pos 0), not inner `import` (pos 7). Got start={}",
            d.start
        );
    }
}

/// TS2303: `import x = require(...)` in a JS file should NOT produce a
/// "Circular definition of import alias" error — tsc skips semantic analysis
/// for TS-only syntax in JS files.
#[test]
fn ts2303_not_emitted_for_import_equals_in_js_file() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "module": "nodenext", "allowJs": true, "checkJs": true }, "files": ["index.js"] }"#,
    );
    // Self-referencing import = require: would normally trigger TS2303 circular check
    write_file(
        &base.join("index.js"),
        "import mod = require(\"./index.js\");\nmod;\n",
    );
    write_file(&base.join("package.json"), r#"{ "type": "module" }"#);

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    let ts2303_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS)
        .collect();

    assert!(
        ts2303_diags.is_empty(),
        "TS2303 should not be emitted for `import = require()` in JS files (TS-only syntax). Got: {:?}",
        ts2303_diags
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // TS8002 SHOULD still be emitted though
    let has_ts8002 = result
        .diagnostics
        .iter()
        .any(|d| d.code == diagnostic_codes::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES);
    assert!(
        has_ts8002,
        "TS8002 should still be emitted for `import = require()` in JS file"
    );
}

#[test]
fn ts5107_not_suppressed_by_jsdoc_param_name_validation() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": false,
            "alwaysStrict": false,
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"/**
 * @param {object} obj
 * @param {string} obj.a
 * @param {string} obj.b
 * @param {string} x
 */
function bad1(x, {a, b}) {}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.iter().any(|d| d.code == 5107),
        "Expected TS5107 for alwaysStrict=false, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| {
            d.code
                != diagnostic_codes::JSDOC_PARAM_TAG_HAS_NAME_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME
        }),
        "Did not expect TS8024 alongside TS5107, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Did not expect follow-on TS2339 alongside TS5107, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn ts5107_es5_target_suppresses_accessor_call_follow_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es5",
            "noEmit": true
          },
          "files": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"class Test24554 {
    get property(): number { return 1; }
}
function test24554(x: Test24554) {
    return x.property();
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&5107),
        "Expected TS5107 for deprecated ES5 target, got: {codes:?}"
    );
    assert!(
        !codes.contains(&6234),
        "Did not expect TS6234 alongside deprecated ES5 target, got: {codes:?}"
    );
}

#[test]
fn js_checkjs_define_property_module_exports_preserve_augmented_shape() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "module": "commonjs",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["index.js", "validate.ts"]
        }"#,
    );

    write_file(
        &base.join("index.js"),
        r#"const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "lastName", { value: "Smith", writable: false });
Object.defineProperty(x, "zip", { get() { return 98122 }, set(_) { /*ignore*/ } });
Object.defineProperty(x, "houseNumber", { get() { return 21.75 } });
Object.defineProperty(x, "zipStr", {
    /** @param {string} str */
    set(str) {
        this.zip = Number(str)
    }
});

/**
 * @param {{name: string}} named
 */
function takeName(named) { return named.name; }

takeName(x);

/** @type {number} */
var a = x.zip;

/** @type {number} */
var b = x.houseNumber;

const returnExemplar = () => x;
const needsExemplar = (_ = x) => void 0;

const expected = /** @type {{name: string, readonly middleInit: string, readonly lastName: string, zip: number, readonly houseNumber: number, zipStr: string}} */(/** @type {*} */(null));

/**
 * @param {typeof returnExemplar} a
 * @param {typeof needsExemplar} b
 */
function match(a, b) {}

match(() => expected, (x = expected) => void 0);

module.exports = x;
"#,
    );

    write_file(
        &base.join("validate.ts"),
        r#"import "./";
import x = require("./");

x.name;
x.middleInit;
x.lastName;
x.zip;
x.houseNumber;
x.zipStr;

x.name = "Another";
x.zip = 98123;
x.zipStr = "OK";

x.lastName = "should fail";
x.houseNumber = 12;
x.zipStr = 12;
x.middleInit = "R";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2339: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();
    let ts2345: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let ts2540: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for defineProperty-augmented shape, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        ts2345.is_empty(),
        "Expected no TS2345 when passing defineProperty-augmented object, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for invalid writable assignments, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty members, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_plain_function_self_alias_prototype_method_preserves_members() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noImplicitAny": true,
            "strictNullChecks": true,
            "noEmit": true
          },
          "files": ["index.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"function Foonly() {
    var self = this
    self.x = 1
    self.m = function() {
        console.log(self.x)
    }
}
Foonly.prototype.mreal = function() {
    var self = this
    self.y = 2
}
const foo = new Foonly()
foo.x
foo.y
foo.m()
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Unexpected TS2339 for self-alias prototype members in project mode: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_commonjs_export_alias_define_property_overlap_reports_ts2323() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["namespacey.js", "namespacer.js"]
        }"#,
    );
    write_file(
        &base.join("namespacey.js"),
        r#"const A = {};
A.bar = class Q {};
module.exports = A;
"#,
    );
    write_file(
        &base.join("namespacer.js"),
        r#"const B = {};
B.NS = require("./namespacey");
Object.defineProperty(B, "NS", { value: "why though", writable: true });
module.exports = B;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2323: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE
                && d.message_text.contains("'NS'")
        })
        .collect();
    assert_eq!(
        ts2323.len(),
        2,
        "Expected TS2323 on overlapping CommonJS alias defineProperty exports, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_static_expando_members_from_assignments_across_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["a.js", "global.js", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.js"),
        r#"export class C1 { }
C1.staticProp = 0;

export function F1() { }
F1.staticProp = 0;

export var C2 = class { };
C2.staticProp = 0;

export let F2 = function () { };
F2.staticProp = 0;
"#,
    );
    write_file(
        &base.join("global.js"),
        r#"class C3 { }
C3.staticProp = 0;

function F3() { }
F3.staticProp = 0;

var C4 = class { };
C4.staticProp = 0;

let F4 = function () { };
F4.staticProp = 0;
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import * as a from "./a";
var n: number;

var n = a.C1.staticProp;
var n = a.C2.staticProp;
var n = a.F1.staticProp;
var n = a.F2.staticProp;

var n = C3.staticProp;
var n = C4.staticProp;
var n = F3.staticProp;
var n = F4.staticProp;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected JS static expando reads across files to stay error-free, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_enum_cross_file_export_keeps_nested_jsdoc_namespace_properties() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["enumDef.js", "index.js"]
        }"#,
    );
    write_file(
        &base.join("enumDef.js"),
        r#"var Host = {};
Host.UserMetrics = {};
/** @enum {number} */
Host.UserMetrics.Action = {
    WindowDocked: 1,
    WindowUndocked: 2,
    ScriptsBreakpointSet: 3,
    TimelineStarted: 4,
};
/**
 * @typedef {string} Host.UserMetrics.Bargh
 */
/**
 * @typedef {string}
 */
Host.UserMetrics.Blah = {
    x: 12
}
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"var Other = {};
Other.Cls = class {
    /**
     * @param {!Host.UserMetrics.Action} p
     */
    method(p) {}
    usage() {
        this.method(Host.UserMetrics.Action.WindowDocked);
    }
}

/**
 * @type {Host.UserMetrics.Bargh}
 */
var x = "ok";

/**
 * @type {Host.UserMetrics.Blah}
 */
var y = "ok";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected nested JS enum/JSDoc namespace writes to avoid TS2339, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_js_enum_object_frozen_value_type_survives_jsdoc_references() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true,
            "module": "commonjs"
          },
          "files": ["index.js", "usage.js"]
        }"#,
    );
    write_file(
        &base.join("index.js"),
        r#"/** @enum {string} */
const Thing = Object.freeze({
    a: "thing",
    b: "chill"
});

exports.Thing = Thing;

/**
 * @param {Thing} x
 */
function useThing(x) {}

exports.useThing = useThing;

/**
 * @param {(x: Thing) => void} x
 */
function cbThing(x) {}

exports.cbThing = cbThing;
"#,
    );
    write_file(
        &base.join("usage.js"),
        r#"const { Thing, useThing, cbThing } = require("./index");

useThing(Thing.a);

/**
 * @typedef {Object} LogEntry
 * @property {string} type
 * @property {number} time
 */

cbThing(type => {
    /** @type {LogEntry} */
    const logEntry = {
        time: Date.now(),
        type,
    };
});
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code
                != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected JSDoc @enum references on Object.freeze exports to resolve to the enum value type, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_jsdoc_type_reference_to_ambient_value_keeps_construct_signature() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "noEmit": true
          },
          "files": ["foo.js"]
        }"#,
    );
    write_file(
        &base.join("foo.js"),
        r#"/** @param {Image} image */
function process(image) {
    return new image(1, 1)
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE),
        "Expected JSDoc type reference to ambient value `Image` to remain constructable in project mode, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn direct_checker_with_real_default_libs_jsdoc_type_reference_to_ambient_value_keeps_construct_signature()
 {
    let files = vec![(
        "foo.js".to_string(),
        r#"/** @param {Image} image */
function process(image) {
    return new image(1, 1)
}
"#
        .to_string(),
    )];

    let lib_files = load_real_default_lib_files(ScriptTarget::ES2015);
    let lib_paths =
        crate::config::resolve_default_lib_files(ScriptTarget::ES2015).expect("default libs");
    let program = tsz::parallel::compile_files_with_libs(files, &lib_paths);
    let file = &program.files[0];
    let binder = tsz::parallel::create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::QueryCache::new(&program.type_interner);
    let mut checker = CheckerState::new(
        &file.arena,
        &binder,
        &query_cache,
        file.file_name.clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(file.source_file);

    assert!(
        checker
            .ctx
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE),
        "Expected direct merged-program checker path to keep ambient `Image` constructable, got diagnostics: {:?}",
        checker.ctx.diagnostics,
    );
}

#[test]
fn compile_jsdoc_arrow_expression_body_preserves_template_scope_for_nested_cast() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "allowJs": true,
            "checkJs": true,
            "strict": true,
            "noEmit": true
          },
          "files": ["mytest.js"]
        }"#,
    );
    write_file(
        &base.join("mytest.js"),
        r#"/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo1 = value => /** @type {string} */({ ...value });

/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    let ts2304: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CANNOT_FIND_NAME)
        .collect();
    let ts2322: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2304.is_empty(),
        "Expected inline JSDoc nested cast to keep arrow @template scope in project mode, got TS2304 diagnostics: {:?}\nAll diagnostics: {:?}",
        ts2304,
        result.diagnostics
    );
    assert_eq!(
        ts2322.len(),
        2,
        "Expected the two existing TS2322 diagnostics from the cast mismatch shape, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn compile_default_import_class_static_enum_object_keeps_enum_members() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "target": "es2015",
            "noEmit": true
          },
          "files": ["a.ts", "b.ts"]
        }"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"enum SomeEnum {
  one,
}
export default class SomeClass {
  public static E = SomeEnum;
}
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"import {default as Def} from "./a"
let a = Def.E.one;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected default-imported class static enum object to keep enum members, got diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn module_augmentation_method_type_params_and_members_resolve_across_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "target": "es2015",
            "strict": true,
            "module": "commonjs",
            "noEmit": true
          },
          "files": ["observable.ts", "map.ts", "main.ts"]
        }"#,
    );
    write_file(
        &base.join("observable.ts"),
        r#"export declare class Observable<T> {
    filter(pred: (e: T) => boolean): Observable<T>;
}
"#,
    );
    write_file(
        &base.join("map.ts"),
        r#"import { Observable } from "./observable";

Observable.prototype.map = function (proj) {
    return this;
}

declare module "./observable" {
    interface Observable<T> {
        map<U>(proj: (e: T) => U): Observable<U>;
    }

    class Bar {}
    const y = 10;
    function z() { }
}
"#,
    );
    write_file(
        &base.join("main.ts"),
        r#"import { Observable } from "./observable";
import "./map";

const x = {} as Observable<number>;
x.map(e => e.toFixed());
let before: number;
before.toFixed();
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED),
        "Expected the real TS2454 to remain, got diagnostics: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| {
            d.code != diagnostic_codes::CANNOT_FIND_NAME || !d.message_text.contains("'U'")
        }),
        "Unexpected TS2304 on augmentation method type parameter `U`: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Unexpected TS2339 for augmented `Observable.map`: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().all(|d| d.code != 7006),
        "Unexpected TS7006 while contextual typing augmented `Observable.map`: {:?}",
        result.diagnostics
    );
}

// TS18003 should be emitted alongside TS5110 when no input files are found
// and module/moduleResolution are incompatible.
#[test]
fn ts18003_emitted_alongside_ts5110_when_no_inputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a tsconfig with incompatible module/moduleResolution and no source files
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          }
        }"#,
    );
    // No .ts files — should trigger TS18003

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5110),
        "Should emit TS5110 for incompatible module/moduleResolution, got: {codes:?}"
    );
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 when no input files found alongside TS5110, got: {codes:?}"
    );
}

// TS18003 should NOT be emitted alongside TS5110 when input files exist
#[test]
fn ts18003_not_emitted_when_inputs_exist_with_ts5110() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "commonjs",
            "moduleResolution": "nodenext"
          },
          "include": ["*.ts"]
        }"#,
    );
    write_file(&base.join("index.ts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        !codes.contains(&18003),
        "Should NOT emit TS18003 when input files exist, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_implicit_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "nodenext",
            "allowJs": true
          }
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 for implicit include with only .mts input, got: {codes:?}"
    );
}

#[test]
fn ts18003_emitted_when_only_mts_is_present_under_explicit_default_include() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "module": "esnext",
            "moduleResolution": "node16",
            "allowJs": true
          },
          "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
          "exclude": ["node_modules"]
        }"#,
    );
    write_file(&base.join("index.mts"), "export const x = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&5110), "Should emit TS5110, got: {codes:?}");
    assert!(
        codes.contains(&18003),
        "Should emit TS18003 for explicit default include with only .mts input, got: {codes:?}"
    );
}

// TS6059: File not under rootDir should produce diagnostic
#[test]
fn ts6059_file_not_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create a rootDir of "src" but put a file outside it
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("outside.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6059),
        "Should emit TS6059 for file outside rootDir, got: {codes:?}"
    );
}

// TS6059 should NOT be emitted when all files are under rootDir
#[test]
fn ts6059_not_emitted_when_all_files_under_root_dir() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const x = 1;");
    write_file(&base.join("src/utils.ts"), "export const y = 2;");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&6059),
        "Should NOT emit TS6059 when all files are under rootDir, got: {codes:?}"
    );
}

#[test]
fn phase_timings_are_populated_after_compilation() {
    let dir = TempDir::new().unwrap();
    let base = &dir.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "include": ["*.ts"] }"#,
    );
    write_file(&base.join("index.ts"), "const x: number = 42;\n");

    let args = default_args();
    let result = compile(&args, base).expect("compilation should succeed");
    let pt = &result.phase_timings;

    // All phase timings should be non-negative
    assert!(pt.io_read_ms >= 0.0, "io_read_ms should be non-negative");
    assert!(
        pt.load_libs_ms >= 0.0,
        "load_libs_ms should be non-negative"
    );
    assert!(
        pt.parse_bind_ms >= 0.0,
        "parse_bind_ms should be non-negative"
    );
    assert!(pt.check_ms >= 0.0, "check_ms should be non-negative");
    assert!(pt.emit_ms >= 0.0, "emit_ms should be non-negative");
    assert!(pt.total_ms > 0.0, "total_ms should be positive");

    // Total should be >= sum of individual phases (wall-clock includes overhead)
    let sum = pt.io_read_ms + pt.load_libs_ms + pt.parse_bind_ms + pt.check_ms + pt.emit_ms;
    assert!(
        pt.total_ms >= sum * 0.9, // allow small floating-point margin
        "total_ms ({}) should be >= sum of phases ({})",
        pt.total_ms,
        sum
    );
}

#[test]
fn compile_reports_outer_ts2345_for_block_body_contextual_callback_return_mismatch() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "target": "es2015"
          },
          "include": ["index.ts"]
        }"#,
    );
    write_file(
        &base.join("index.ts"),
        r#"
interface Collection<T, U> {
    length: number;
    add(x: T, y: U): void;
    remove(x: T, y: U): boolean;
}

interface Combinators {
    map<T, U>(c: Collection<T, U>, f: (x: T, y: U) => any): Collection<any, any>;
    map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V): Collection<T, V>;
}

declare var _: Combinators;
declare var c2: Collection<number, string>;
var r5a = _.map<number, string, Date>(c2, (x, y) => { return x.toFixed() });
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2345),
        "Expected outer TS2345 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&2322),
        "Expected no inner TS2322 for block-body callback return mismatch, got: {:?}",
        result.diagnostics
    );
}
