//! Cross-module *generic* interface heritage member resolution, and `delete`
//! legality (TS2790) against the *declared* property rather than the
//! flow-narrowed receiver.
//!
//! Structural rules:
//! - When a generic interface declared in another program module is
//!   referenced by name, `tsc` resolves it in its declaring module —
//!   including `extends` heritage — so inherited members must resolve
//!   (TS2339 pins, witnessed by the ofetch project row's
//!   `ResolvedFetchOptions<R>` / `FetchResponse<T>` cascades).
//! - `delete obj.prop` is legal when the *declared* property is optional (or
//!   its declared type includes `undefined`). tsz's truthiness/`in`
//!   narrowing intersects the receiver with a synthetic required slot, which
//!   must not turn a declared-optional property into a TS2790.
//!
//! Binder names are varied across cases so no identifier is load-bearing.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file_with_global_index;
use tsz_common::common::ModuleKind;

fn check(types_src: &str, main_src: &str) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(
        &[("./defs.ts", types_src), ("./main.ts", main_src)],
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn family_errors(diagnostics: &[Diagnostic]) -> Vec<(u32, u32, String)> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                || d.code == diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL
        })
        .map(|d| (d.code, d.start, d.message_text.to_string()))
        .collect()
}

fn assert_clean(diagnostics: &[Diagnostic]) {
    let relevant = family_errors(diagnostics);
    assert!(
        relevant.is_empty(),
        "expected inherited members of the imported generic interface to resolve, got: {relevant:?}",
    );
}

/// Core witness: imported generic interface extending a generic base,
/// referenced with default type args; inherited member access must resolve.
///
/// NOTE: the unit harness checks only the entry file, so *parameter*
/// annotations (lowered to `Application(Lazy(def), args)` without entering
/// the checker's type-reference resolution) hit a separate, pre-existing
/// single-entry resolution gap. These tests pin the type-reference consumer
/// (`declare const` annotations); the parameter-annotation form is covered
/// end-to-end by the ofetch project row and CLI witnesses.
#[test]
fn imported_generic_interface_inherited_member_resolves() {
    let diags = check(
        r#"
export interface Stem<R extends string = string> {
  payload?: string;
  mode?: "half" | undefined;
}
export interface Wrap<R extends string = string> extends Stem<R> {
  labels: number;
}
"#,
        r#"
import type { Wrap } from "./defs";
declare const w: Wrap;
w.labels;
w.payload;
w.mode;
"#,
    );
    assert_clean(&diags);
}

/// Explicit type argument instead of defaults.
#[test]
fn imported_generic_interface_explicit_args_inherited_member_resolves() {
    let diags = check(
        r#"
export interface Root<K extends string = string> { item?: K; }
export interface Leaf<K extends string = string> extends Root<K> { own: number; }
"#,
        r#"
import type { Leaf } from "./defs";
declare const l: Leaf<"a" | "b">;
l.own;
l.item;
"#,
    );
    assert_clean(&diags);
}

/// Member access through a containing object property (the ofetch shape:
/// `context.options.body`), with truthiness + `in` + `delete` narrowing flows.
#[test]
fn imported_generic_interface_member_after_narrowing_flows() {
    let diags = check(
        r#"
export interface CoreOpts<R extends string = string, T = any> {
  payload?: string;
  selector?: Record<string, any>;
  mode?: "half" | undefined;
  budget?: number;
}
export interface FullOpts<R extends string = string, T = any> extends CoreOpts<R, T> {
  labels: Headers;
}
export interface Carrier {
  target: string | Request;
  options: FullOpts;
}
"#,
        r#"
import type { Carrier } from "./defs";
export function go(carrier: Carrier) {
  if (typeof carrier.target === "string") {
    if (carrier.options.selector) {
      delete carrier.options.selector;
    }
    if ("selector" in carrier.options) {
      delete carrier.options.selector;
    }
  }
  if (carrier.options.payload) {
    if (!("mode" in carrier.options)) {
      carrier.options.mode = "half";
    }
  }
  if (carrier.options.budget) {
    carrier.options.budget.toFixed();
  }
}
"#,
    );
    assert_clean(&diags);
}

/// Same-file control: the cross-module delegation must not disturb the
/// already-working local path.
#[test]
fn same_file_generic_interface_heritage_still_resolves() {
    let diags = check(
        r#"export const unrelated = 1;"#,
        r#"
interface Inner<Q extends string = string> {
  data?: string;
  flag?: boolean;
}
interface Outer<Q extends string = string> extends Inner<Q> {
  tags: Headers;
}
export function go(o: Outer) {
  if (o.data) {
    o.data.toUpperCase();
  }
  o.flag;
  o.tags;
}
"#,
    );
    assert_clean(&diags);
}

/// Concrete (non-generic) imported interface control — already worked before
/// the delegation fix and must keep working.
#[test]
fn imported_concrete_interface_inherited_member_resolves() {
    let diags = check(
        r#"
export interface Floor { area?: number; }
export interface Room extends Floor { door: string; }
"#,
        r#"
import type { Room } from "./defs";
export function go(r: Room) {
  r.door;
  r.area;
}
"#,
    );
    assert_clean(&diags);
}

/// Negative control: a genuinely missing member still reports TS2339.
#[test]
fn imported_generic_interface_missing_member_still_errors() {
    let diags = check(
        r#"
export interface Seed<V = any> { kernel?: V; }
export interface Plant<V = any> extends Seed<V> { stem: string; }
"#,
        r#"
import type { Plant } from "./defs";
declare const p: Plant;
p.stem;
p.kernel;
p.absent;
"#,
    );
    let property_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();
    assert_eq!(
        property_errors.len(),
        1,
        "expected exactly one TS2339 for the genuinely missing member, got: {:?}",
        family_errors(&diags),
    );
    assert!(
        property_errors[0].message_text.contains("'absent'"),
        "the surviving TS2339 must be for the missing member, got: {:?}",
        property_errors[0].message_text,
    );
}

/// `delete` on a declared-optional property stays legal even when a
/// truthiness or `in` guard narrowed the receiver (tsz's narrowing promotes
/// the slot to required in a synthetic intersection; tsc checks the declared
/// property symbol, so no TS2790).
#[test]
fn delete_of_declared_optional_property_after_guard_is_legal() {
    let diags = check(
        r#"
export interface Knobs<R extends string = string> {
  filter?: Record<string, any>;
  extras?: Record<string, any>;
}
"#,
        r#"
import type { Knobs } from "./defs";
interface Holder { cfg: Knobs }
export function go(h: Holder) {
  if (h.cfg.filter) {
    delete h.cfg.filter;
  }
  if ("extras" in h.cfg) {
    delete h.cfg.extras;
  }
}
"#,
    );
    let delete_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL)
        .collect();
    assert!(
        delete_errors.is_empty(),
        "declared-optional properties stay deletable after narrowing, got: {delete_errors:?}",
    );
}

/// Positive TS2790 control: deleting a genuinely required property still
/// errors — the declared-type re-check must not swallow real violations.
#[test]
fn delete_of_required_property_still_errors() {
    let diags = check(
        r#"export interface Fixed { anchor: number; }"#,
        r#"
import type { Fixed } from "./defs";
export function go(f: Fixed) {
  delete f.anchor;
}
"#,
    );
    let delete_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL)
        .collect();
    assert_eq!(
        delete_errors.len(),
        1,
        "deleting a required property must still report TS2790, got: {:?}",
        family_errors(&diags),
    );
}
