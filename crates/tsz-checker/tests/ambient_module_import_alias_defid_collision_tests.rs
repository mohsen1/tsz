//! Regression coverage for ambient-module import aliases whose exported type
//! alias name collides with a lib/global declaration name.
//!
//! Structural rule: when a same-arena type-alias body references an imported
//! alias, `DefId` resolution must use the binder-selected symbol plus the
//! syntactic leaf name. Deterministic shared-store `DefId` election must not let
//! the raw symbol-only fallback pick a same-named lib/global declaration.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs_stamped, diagnostic_code_messages, load_compiled_lib_files,
};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_compiled_lib_files(&["lib.es5.d.ts", "lib.es2015.d.ts"]);
    diagnostic_code_messages(check_multi_file_with_libs_stamped(
        &[("ambient-alias.ts", source)],
        "ambient-alias.ts",
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
        &libs,
    ))
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(code, _)| *code).collect()
}

#[test]
fn ambient_module_imported_boolean_alias_beats_lib_global_collision() {
    let diags = diagnostics(
        r#"
declare module "Local/Boolean" {
  export type Boolean = 0 | 1;
}
declare module "Use/Alias" {
  import { Boolean } from "Local/Boolean";

  type Switch<B extends Boolean = 1> = {
    1: string;
    0: number;
  }[B];

  export type Result = Switch<1>;
  const bad: Result = 123;
}
"#,
    );
    let codes = codes(&diags);
    assert!(
        !codes.iter().any(|code| matches!(code, 2344 | 2536 | 2538)),
        "imported Boolean alias should not resolve to the lib/global Boolean; got {diags:#?}"
    );
    assert!(
        codes.contains(&2322),
        "negative assignment should still be checked through the alias body; got {diags:#?}"
    );
}

#[test]
fn ambient_module_imported_renamed_alias_keeps_same_alias_body_path() {
    let diags = diagnostics(
        r#"
declare module "Local/Flag" {
  export type Flag = 0 | 1;
}
declare module "Use/Renamed" {
  import { Flag } from "Local/Flag";

  type Switch<B extends Flag = 1> = {
    1: string;
    0: number;
  }[B];

  export type Result = Switch<1>;
  const bad: Result = 123;
}
"#,
    );
    let codes = codes(&diags);
    assert!(
        !codes.iter().any(|code| matches!(code, 2344 | 2536 | 2538)),
        "renamed imported alias should stay on the same structural path; got {diags:#?}"
    );
    assert!(
        codes.contains(&2322),
        "renamed control should still check the alias body; got {diags:#?}"
    );
}

#[test]
fn ambient_module_imported_number_alias_body_beats_lib_global_collision() {
    let diags = diagnostics(
        r#"
declare module "Local/Number" {
  export type Number = string;
}
declare module "Use/NumberAlias" {
  import { Number } from "Local/Number";

  type IterationOf<N extends Number> = N extends "0" ? 0 : 1;
  type Drop<N extends Number> = IterationOf<N>;

  export type Result = Drop<string>;
  const bad: Result = "wrong";
}
"#,
    );
    let codes = codes(&diags);
    assert!(
        !codes.iter().any(|code| matches!(code, 2344 | 2536 | 2538)),
        "imported Number alias should not resolve to the lib/global Number; got {diags:#?}"
    );
    assert!(
        codes.contains(&2322),
        "negative assignment should still be checked through the alias body; got {diags:#?}"
    );
}

#[test]
fn ambient_module_imported_number_alias_accepts_alias_resolving_to_string() {
    let diags = diagnostics(
        r#"
declare module "Local/Number" {
  export type Number = string;
}
declare module "Local/Key" {
  export type Key<I> = string;
}
declare module "Use/NumberAliasArg" {
  import { Number } from "Local/Number";
  import { Key } from "Local/Key";

  type Drop<N extends Number> = N;
  type Gap<I> = Drop<Key<I>>;

  export type Result = Gap<{}>;
  const bad: Result = 123;
}
"#,
    );
    let codes = codes(&diags);
    assert!(
        !codes.iter().any(|code| matches!(code, 2344 | 2536 | 2538)),
        "alias resolving to string should satisfy imported Number alias; got {diags:#?}"
    );
    assert!(
        codes.contains(&2322),
        "negative assignment should still be checked through the alias body; got {diags:#?}"
    );
}

#[test]
fn ambient_module_imported_number_alias_accepts_generic_key_alias_chain() {
    let diags = diagnostics(
        r#"
declare module "Number/Number" {
  export type Number = string;
}
declare module "Iteration/_Internal" {
  export type Formats = "n" | "s";
}
declare module "Iteration/Iteration" {
  export type Iteration = [string, string, string, number, "-" | "0" | "+"];
}
declare module "Iteration/Format" {
  import { Iteration } from "Iteration/Iteration";
  import { Formats } from "Iteration/_Internal";

  export type Format<I extends Iteration, fmt extends Formats> = {
    s: I[2];
    n: I[3];
  }[fmt];
}
declare module "Iteration/Key" {
  import { Iteration } from "Iteration/Iteration";
  import { Format } from "Iteration/Format";

  export type Key<I extends Iteration> = Format<I, "s">;
}
declare module "List/List" {
  export type List<A = any> = ReadonlyArray<A>;
}
declare module "List/Drop" {
  import { List } from "List/List";
  import { Number } from "Number/Number";

  export type _Drop<L extends List, N extends Number> = N;
}
declare module "Use/Drop" {
  import { _Drop } from "List/Drop";
  import { Key } from "Iteration/Key";
  import { Iteration } from "Iteration/Iteration";

  type Gap<I extends Iteration> = _Drop<[], Key<I>>;
  export type Result = Gap<["0", "1", "2", 3, "+"]>;
  const bad: Result = 123;
}
"#,
    );
    let codes = codes(&diags);
    assert!(
        !codes.iter().any(|code| matches!(code, 2344 | 2536 | 2538)),
        "generic alias chain should satisfy imported Number alias; got {diags:#?}"
    );
    assert!(
        codes.contains(&2322),
        "negative assignment should still be checked through the alias body; got {diags:#?}"
    );
}
