//! Cross-file interface heritage that `extends` a generic *type-alias
//! application* (`Omit<…>` / `Pick<…>` / `Partial<…>` / `Record<…>`).
//!
//! Structural rule: when `interface I extends Alias<Args>` and `Alias` is a
//! generic type alias resolved across modules/arenas (so the heritage symbol
//! resolves to a nominal `Lazy(DefId)` reference), tsc binds the arguments and
//! inherits the alias's apparent members; tsz must form the generic
//! application `Alias<Args>` and resolve it env-aware to those members.
//!
//! The previous code applied `instantiate_type` to the bare `Lazy`, a no-op
//! that silently dropped the type arguments. The base then degraded to an
//! argument-less `unknown` and the heritage merge discarded it, so cross-file
//! property access on `I` reported every inherited member as missing (TS2339).
//! The defect was cross-file only — single-file resolution expands the alias
//! body inline, so substitution worked there. This is the general form behind
//! the ofetch canary (`FetchOptions extends Omit<RequestInit, "body">`,
//! #13470); it needs the real multi-module driver to reproduce (the in-crate
//! `check_multi_file_with_libs` harness expands lib aliases inline and cannot
//! host it).
//!
//! Cases vary the utility type, the alias base (lib vs user), the root-file
//! order, and the inheritance depth, plus renamed binders and a
//! genuine-missing negative control so the rule follows the type shape rather
//! than identifier names.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) with the given root-file order.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022,dom,dom.iterable",
    ];
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn missing_property_messages(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Assert no TS2339 "property does not exist" in either root-file order
/// (consumer-first is the cross-file regression direction).
fn assert_no_missing_property_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = missing_property_messages(&compile_in_order(files, &names));
    assert!(
        forward.is_empty(),
        "expected no TS2339 in forward root order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = missing_property_messages(&compile_in_order(files, &reversed));
    assert!(
        backward.is_empty(),
        "expected no TS2339 in reversed root order {reversed:?}, got: {backward:?}"
    );
}

#[test]
fn imported_interface_extending_omit_keeps_inherited_members() {
    assert_no_missing_property_both_orders(&[
        (
            "consumer.ts",
            r#"
import type { FetchOptions } from './shapes';
export function read(o: FetchOptions) {
    o.method;
    o.baseURL;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
interface Base { method?: string; cache?: string; }
export interface FetchOptions extends Omit<Base, "cache"> {
    baseURL?: string;
}
"#,
        ),
    ]);
}

#[test]
fn imported_interface_extending_record_keeps_inherited_member() {
    assert_no_missing_property_both_orders(&[
        (
            "consumer.ts",
            r#"
import type { FetchOptions } from './shapes';
export function read(o: FetchOptions) {
    o.method;
    o.baseURL;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
export interface FetchOptions extends Record<"method", string> {
    baseURL?: string;
}
"#,
        ),
    ]);
}

#[test]
fn imported_interface_extending_partial_keeps_inherited_members() {
    assert_no_missing_property_both_orders(&[
        (
            "consumer.ts",
            r#"
import type { FetchOptions } from './shapes';
export function read(o: FetchOptions) {
    o.method;
    o.cache;
    o.baseURL;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
interface Base { method: string; cache: string; }
export interface FetchOptions extends Partial<Base> {
    baseURL?: string;
}
"#,
        ),
    ]);
}

/// Two levels of generic-interface inheritance over a utility-type heritage
/// base must compose: `R<…> extends I<…>` where `I<…> extends Omit<…>`. This
/// is the exact ofetch shape (`ResolvedFetchOptions<R, T> extends
/// FetchOptions<R, T>`). A deferred `Object & Application` intersection base
/// does not survive the second-level merge, so the alias application is
/// resolved to a plain object before merging.
#[test]
fn imported_two_level_generic_interface_keeps_all_members() {
    assert_no_missing_property_both_orders(&[
        (
            "consumer.ts",
            r#"
import type { ResolvedFetchOptions } from './shapes';
export function read(o: ResolvedFetchOptions) {
    o.body;
    o.method;
    o.baseURL;
    o.headers;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
interface Base { method?: string; cache?: string; }
export interface FetchOptions<R = string, T = unknown> extends Omit<Base, "cache"> {
    baseURL?: string;
    body?: string;
}
export interface ResolvedFetchOptions<R = string, T = unknown> extends FetchOptions<R, T> {
    headers: string;
}
"#,
        ),
    ]);
}

/// Anti-hardcoding: the rule follows the type shape, not the identifier
/// spellings.
#[test]
fn imported_interface_extending_omit_renamed_binders_keeps_members() {
    assert_no_missing_property_both_orders(&[
        (
            "consumer.ts",
            r#"
import type { Widget } from './shapes';
export function read(w: Widget) {
    w.alpha;
    w.gamma;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
interface Shape { alpha?: string; beta?: string; }
export interface Widget extends Omit<Shape, "beta"> {
    gamma?: string;
}
"#,
        ),
    ]);
}

/// The inherited members are genuinely present, not silently widened to
/// `any`: `Record<"method", string>` makes `method` required, so an object
/// literal that omits it is rejected cross-file with TS2741.
#[test]
fn imported_interface_extending_record_member_is_required() {
    let files = &[
        (
            "consumer.ts",
            r#"
import type { FetchOptions } from './shapes';
export const bad: FetchOptions = { baseURL: "a" };
export const good: FetchOptions = { method: "GET", baseURL: "a" };
"#,
        ),
        (
            "shapes.ts",
            r#"
export interface FetchOptions extends Record<"method", string> {
    baseURL?: string;
}
"#,
        ),
    ];
    let diags = compile_in_order(files, &["consumer.ts", "shapes.ts"]);
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2741 && d.message_text.contains("method")),
        "omitting the required inherited `method` must report TS2741, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Negative control: a property on neither the interface nor the utility-type
/// heritage base must still report TS2339 (no over-broadening to `any`).
#[test]
fn imported_interface_extending_omit_genuinely_missing_property_still_errors() {
    let files = &[
        (
            "consumer.ts",
            r#"
import type { FetchOptions } from './shapes';
export function read(o: FetchOptions) {
    o.cache;
    o.nonexistent;
}
"#,
        ),
        (
            "shapes.ts",
            r#"
interface Base { method?: string; cache?: string; }
export interface FetchOptions extends Omit<Base, "cache"> {
    baseURL?: string;
}
"#,
        ),
    ];
    let missing =
        missing_property_messages(&compile_in_order(files, &["consumer.ts", "shapes.ts"]));
    assert!(
        missing.iter().any(|m| m.contains("nonexistent")),
        "a genuinely-absent property must still report TS2339, got: {missing:?}"
    );
    assert!(
        missing.iter().any(|m| m.contains("cache")),
        "the Omit-excluded `cache` must still report TS2339, got: {missing:?}"
    );
}
