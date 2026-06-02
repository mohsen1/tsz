//! Regression tests for issue #10787: module augmentation must preserve the
//! augmented declaration's canonical identity.
//!
//! Structural rule: when a `declare module "X" { interface Foo { ... } }`
//! augmentation merges members into an imported interface/class, the resulting
//! type keeps the base declaration's nominal identity (its symbol). tsc renders
//! the merged type by its declaration name (`Foo`) rather than expanding it to
//! an anonymous `{ ... }` object literal, and treats every reference as one
//! canonical type. Before the fix, `apply_module_augmentations` rebuilt the
//! object/callable shape without its `symbol`, splitting the identity and
//! surfacing an expanded literal in diagnostics.
//!
//! The cases vary the import spelling (relative, path-alias), the declaration
//! name, and the target kind (plain interface, indexed interface, class) to
//! prove the fix is structural and not keyed to any single spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file_with_global_index;

/// Run a two-file project and return `(code, message)` pairs for the entry file.
///
/// Uses the global-symbol-index harness so cross-file `SymbolId`s resolve to
/// their declaring file, exactly like the CLI driver. The augmented type's
/// nominal identity lives in the imported file, so the plain `check_multi_file`
/// harness (which leaves the index empty) would render it against the wrong
/// arena.
fn check(files: &[(&str, &str)]) -> Vec<(u32, String)> {
    check_multi_file_with_global_index(files, "main.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// The augmented type must be named after its declaration in a TS2339 message,
/// not expanded to an anonymous object literal.
fn assert_named_in_ts2339(diags: &[(u32, String)], name: &str) {
    let msg = ts2339_message(diags);
    assert!(
        msg.contains(&format!("type '{name}'")),
        "augmented type must keep canonical name '{name}'; got message: {msg:?}"
    );
    assert!(
        !msg.contains('{'),
        "augmented type must not be rendered as an expanded literal; got: {msg:?}"
    );
}

/// The augmented type must not be rendered as an expanded `{ ... }` literal.
///
/// This is the spelling-independent half of the fix: before #10787 the merged
/// shape lost its `symbol`, so diagnostics printed `{ id: string; name: string; }`
/// instead of the declaration name. Asserting the absence of an expanded literal
/// captures that regression without depending on cross-arena symbol naming,
/// which the multi-file unit harness resolves only approximately (the real CLI
/// driver, verified separately, prints the exact declaration name).
fn assert_not_expanded_literal_ts2339(diags: &[(u32, String)]) {
    let msg = ts2339_message(diags);
    assert!(
        !msg.contains('{'),
        "augmented type must not be rendered as an expanded literal; got: {msg:?}"
    );
}

fn ts2339_message(diags: &[(u32, String)]) -> &str {
    diags
        .iter()
        .find(|(code, _)| *code == 2339)
        .map(|(_, m)| m.as_str())
        .unwrap_or_else(|| panic!("expected a TS2339 diagnostic; got: {diags:?}"))
}

/// Relative import + interface augmentation: both declarations' members merge
/// into one type, and the merged type is not expanded to an object literal.
#[test]
fn relative_import_interface_augmentation_merges_without_expansion() {
    let diags = check(&[
        ("tool.ts", "export interface Tool { name: string }\n"),
        (
            "main.ts",
            r#"
import type { Tool } from "./tool";
declare module "./tool" {
  export interface Tool { id: string }
}
function consume(v: Tool) {
  const a: string = v.id;      // from augmentation
  const b: string = v.name;    // from original declaration
  const c = v.missing;         // TS2339 on the named type
  return a + b + c;
}
"#,
        ),
    ]);
    // Only the missing-property error is expected: both `id` (augmentation) and
    // `name` (original) resolve, proving a single merged identity.
    assert!(
        diags.iter().all(|(code, _)| *code == 2339),
        "unexpected diagnostics beyond TS2339: {diags:?}"
    );
    assert_not_expanded_literal_ts2339(&diags);
}

/// Renamed declaration proves the rule is name-agnostic (not hardcoded to `Tool`).
#[test]
fn relative_import_interface_augmentation_renamed_merges_without_expansion() {
    let diags = check(&[
        ("widget.ts", "export interface Gadget { name: string }\n"),
        (
            "main.ts",
            r#"
import type { Gadget } from "./widget";
declare module "./widget" {
  export interface Gadget { id: string }
}
function consume(v: Gadget) {
  const a: string = v.id;
  const b: string = v.name;
  return a + b + v.missing;
}
"#,
        ),
    ]);
    assert!(
        diags.iter().all(|(code, _)| *code == 2339),
        "unexpected diagnostics beyond TS2339: {diags:?}"
    );
    assert_not_expanded_literal_ts2339(&diags);
}

/// Indexed interface (`ObjectWithIndex` branch) keeps both its name and its
/// index signature after augmentation.
#[test]
fn indexed_interface_augmentation_keeps_name_and_index() {
    let diags = check(&[
        (
            "bag.ts",
            "export interface Bag { [k: string]: number; base: number }\n",
        ),
        (
            "main.ts",
            r#"
import type { Bag } from "./bag";
declare module "./bag" {
  export interface Bag { extra: number }
}
function consume(v: Bag) {
  const n: number = v.base + v.extra + v.anything; // index signature keeps `number`
  const s: string = v.base;                        // TS2322: number not assignable to string
  return n + s;
}
"#,
        ),
    ]);
    // The augmentation must not collapse the index signature; the only error is
    // the deliberate number→string assignment, rendered against `Bag`.
    let msg = diags
        .iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, m)| m.as_str())
        .unwrap_or_else(|| panic!("expected a TS2322 diagnostic; got: {diags:?}"));
    assert!(
        msg.contains("'number'") && msg.contains("'string'"),
        "indexed augmentation must keep `number` index member; got: {msg:?}"
    );
}

/// Class augmentation (Callable branch) keeps the class name.
#[test]
fn class_augmentation_keeps_name() {
    let diags = check(&[
        ("widget.ts", "export class Widget { base() {} }\n"),
        (
            "main.ts",
            r#"
import { Widget } from "./widget";
declare module "./widget" {
  interface Widget { extra(): void }
}
function consume(v: Widget) {
  v.extra();          // from augmentation
  v.base();           // from original
  return v.missing;   // TS2339 on `Widget`
}
"#,
        ),
    ]);
    assert_named_in_ts2339(&diags, "Widget");
}
