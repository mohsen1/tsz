//! Tests for TS2353 against `Omit<T, K>` and `Pick<T, K>` mapped-derived
//! targets.
//!
//! Issue #5996: assigning to a non-generic `Omit<Person, "email">` flagged
//! every source property (`name`, `age`) as TS2353 even though tsc accepts
//! the assignment.
//!
//! Root cause: in
//! `check_object_literal_excess_properties`'s "generic mapped receiver lacks
//! explicit property" loop, the original `target` (a Mapped with an
//! unresolved `Exclude<keyof T, K>` constraint) reported "lacks property" for
//! every property because `extract_string_literal_keys` couldn't reduce the
//! constraint without full evaluation. The loop combined three normalized
//! candidates with `any` — so even when `effective_target`/`resolved_target`
//! had been reduced to the concrete `{ name, age }` object and reported
//! "doesn't lack", the unresolved alias view alone was enough to surface
//! TS2353.
//!
//! Structural rule: a generic-mapped-receiver "lacks property X" verdict is
//! authoritative only when EVERY normalized view of the target lacks X.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn diags_without_libs(source: &str) -> Vec<(u32, String)> {
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &[])
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn omit_with_concrete_type_accepts_remaining_properties() {
    let source = r#"
interface Person {
  name: string;
  age: number;
  email: string;
}

const p: Omit<Person, "email"> = {
  name: "Alice",
  age: 30,
};
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for valid Omit assignment, got: {ts2353:?}",
    );
}

#[test]
fn omit_still_flags_excess_property_outside_remaining_set() {
    let source = r#"
interface Person {
  name: string;
  age: number;
  email: string;
}

const p: Omit<Person, "email"> = {
  name: "Alice",
  age: 30,
  email: "x@y", // excess — was Omit'd out
};
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for the omitted key, got: {ts2353:?}",
    );
    assert!(
        ts2353[0].1.contains("'email'"),
        "Expected TS2353 to mention 'email', got: {}",
        ts2353[0].1
    );
}

#[test]
fn pick_with_concrete_type_accepts_kept_properties() {
    let source = r#"
interface Person {
  name: string;
  age: number;
  email: string;
}

const p: Pick<Person, "name" | "age"> = {
  name: "Alice",
  age: 30,
};
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for valid Pick assignment, got: {ts2353:?}",
    );
}

#[test]
fn lib_pick_all_keys_excess_property_display_uses_keyof() {
    let source = r#"
interface Person {
  name: string;
  age: number;
}

const p: Pick<Person, "name" | "age"> = {
  name: "Alice",
  age: 30,
  email: "x@y",
};
"#;
    let ds = diags(source);
    let ts2353 = ds
        .iter()
        .find(|d| d.0 == 2353)
        .expect("expected TS2353 for excess property");
    assert!(
        ts2353.1.contains("Pick<Person, keyof Person>"),
        "Expected actual lib Pick display to collapse all keys to keyof, got: {}",
        ts2353.1
    );
}

#[test]
fn local_pick_alias_named_like_lib_does_not_use_lib_keyof_rewrite() {
    let source = r#"
export {};

type Pick<T, K extends keyof T> = { [P in K]: T[P] };

interface Person {
  name: string;
  age: number;
  phone: string;
}

const p: Pick<Person, "name" | "age"> = {
  name: "Alice",
  age: 30,
  email: "x@y",
};
"#;
    let ds = diags_without_libs(source);
    let ts2353 = ds
        .iter()
        .find(|d| d.0 == 2353)
        .expect("expected TS2353 for excess property");
    assert!(
        !ts2353.1.contains("Pick<Person, keyof Person>"),
        "User-defined Pick must not be recognized through rendered text: {}",
        ts2353.1
    );
}

#[test]
fn omit_single_key_alias_no_false_ts2353() {
    let source = r#"
interface Config {
  host: string;
  port: number;
  secret: string;
}

type Public = Omit<Config, "secret">;

const c: Public = {
  host: "localhost",
  port: 8080,
};
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for Public alias assignment, got: {ts2353:?}",
    );
}

#[test]
fn omit_with_alternate_interface_and_key_names() {
    // Anti-hardcoding (CLAUDE.md §25): rule must not depend on identifier
    // spellings.
    let source = r#"
interface User {
  id: number;
  password: string;
}

const view: Omit<User, "password"> = { id: 1 };
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for alternate-name Omit assignment, got: {ts2353:?}",
    );
}
