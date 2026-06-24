//! Member-retention regression guards for the computed-key and transitive
//! heritage shapes behind the `immer` canary false positives (issue #13942).
//!
//! Two distinct structural invariants are pinned here. Both, when broken,
//! surface as the same *member-drop* false-positive family
//! (`TS2322`/`TS2339`/`TS2353`/`TS2741`) the `immer` row reported:
//!
//! 1. **Computed-key named-property retention.** An object literal whose keys
//!    are computed from string-literal-typed `const`s
//!    (`const WRITABLE = "writable"; { [WRITABLE]: true }`) must produce the
//!    *named* property `writable`, not a `string`-keyed index signature — and
//!    must keep doing so when the `const` is reached through an `import`
//!    (`immer` declares these key consts in `common.ts` and consumes them in
//!    `proxy.ts`/`utils/common.ts`). A widened key collapses the literal into
//!    an index signature, which is then rejected against the named optional
//!    members of `PropertyDescriptor` / the `ProxyHandler.getOwnPropertyDescriptor`
//!    return — the `immer` FP #1/#2 shape.
//!
//! 2. **Transitive heritage member retention.** `Set`/`Map`/`Array` iterator
//!    results (`SetIterator<T>` etc.) inherit `next` two `extends` levels up
//!    (`SetIterator<T> -> IteratorObject<T> -> Iterator<T>`). Assigning such a
//!    result to `IterableIterator<T>` must see the inherited `next`; dropping a
//!    transitively-inherited member yields the `immer` FP #3/#4 shape
//!    (`SetIterator` reported as missing `next`, `TS2741`).
//!
//! These are behavior-preserving guards on the current (`tsc`-matching) result,
//! filed as `hold` parity-floor coverage: the member-drop class is actively
//! refactored by the dual-`TypeEnvironment` collapse (#14348) and the
//! lib-heritage representation work (#13933), and these minimal, deterministic
//! cases catch a reintroduction without needing the full `immer` fixture. Per
//! the anti-hardcoding contract the binder names (the key consts, the helper
//! and variable identifiers) are varied across cases so the guard stays
//! structural rather than name-scoped.

use std::sync::Arc;
use std::sync::OnceLock;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_multi_file_with_libs_stamped, check_source_with_libs, load_default_lib_files,
};
use tsz_common::ModuleKind;

fn libs() -> &'static [Arc<LibFile>] {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2020,
        module: ModuleKind::CommonJS,
        no_lib: false,
        ..Default::default()
    }
}

/// Assert the check produced no diagnostics, printing the offending codes and
/// messages (so a regression names the member-drop code directly).
fn assert_clean(diags: &[Diagnostic], context: &str) {
    assert!(
        diags.is_empty(),
        "{context}: expected no diagnostics, got {:?}\n{}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>(),
        diags
            .iter()
            .map(|d| format!("  TS{} {}", d.code, d.message_text))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

fn check_single(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(source, "main.ts", opts(), libs())
}

fn check_modules(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_libs_stamped(files, entry, opts(), libs())
}

// ─── Group A: computed-key named-property retention (immer FP #1/#2) ─────────

/// A string-literal `const` used as a computed key produces the *named*
/// property, so the literal is assignable to a structural target with that
/// named (optional) member. Single-file baseline.
#[test]
fn computed_key_from_local_string_const_yields_named_property() {
    let diags = check_single(
        r#"
const flagKey = "writable";
type Slot = { writable?: boolean };
const slot: Slot = { [flagKey]: true };
const read: boolean | undefined = slot.writable;
"#,
    );
    assert_clean(&diags, "local computed key -> named property");
}

/// The named-property result survives when the key `const` is reached through
/// an `import` — the `immer` `common.ts` -> `proxy.ts` shape. Binder names are
/// deliberately different from the single-file case.
#[test]
fn computed_keys_from_imported_string_consts_yield_named_properties() {
    let files = [
        (
            "descriptor_keys.ts",
            r#"
export const propWritable = "writable";
export const propConfigurable = "configurable";
export const propEnumerable = "enumerable";
export const propValue = "value";
"#,
        ),
        (
            "build_descriptor.ts",
            r#"
import { propWritable, propConfigurable, propEnumerable, propValue } from "./descriptor_keys";
type Descriptor = {
  writable?: boolean;
  configurable?: boolean;
  enumerable?: boolean;
  value?: unknown;
};
export function buildDescriptor(): Descriptor {
  return {
    [propWritable]: true,
    [propConfigurable]: true,
    [propEnumerable]: false,
    [propValue]: 1,
  };
}
"#,
        ),
    ];
    let diags = check_modules(&files, "build_descriptor.ts");
    assert_clean(&diags, "imported computed keys -> structural descriptor");
}

/// Same shape against the real lib `PropertyDescriptor`: the computed-key
/// literal must satisfy `PropertyDescriptor`'s named optional members rather
/// than reading as an excess `string`-indexed shape.
#[test]
fn computed_keys_from_imported_consts_satisfy_property_descriptor() {
    let files = [
        (
            "keys.ts",
            r#"
export const wKey = "writable";
export const vKey = "value";
"#,
        ),
        (
            "make_prop.ts",
            r#"
import { wKey, vKey } from "./keys";
export function makeProp(): PropertyDescriptor {
  return { [wKey]: true, [vKey]: 42 };
}
"#,
        ),
    ];
    let diags = check_modules(&files, "make_prop.ts");
    assert_clean(&diags, "imported computed keys -> PropertyDescriptor");
}

/// The `immer` `proxy.ts:234` shape directly: an object literal with
/// computed keys returned from `ProxyHandler.getOwnPropertyDescriptor`.
#[test]
fn computed_keys_satisfy_proxy_handler_descriptor_return() {
    let files = [
        (
            "handler_keys.ts",
            r#"
export const kWritable = "writable";
export const kConfigurable = "configurable";
export const kEnumerable = "enumerable";
export const kValue = "value";
"#,
        ),
        (
            "handler.ts",
            r#"
import { kWritable, kConfigurable, kEnumerable, kValue } from "./handler_keys";
export const proxyHandler: ProxyHandler<object> = {
  getOwnPropertyDescriptor() {
    return {
      [kWritable]: true,
      [kConfigurable]: true,
      [kEnumerable]: true,
      [kValue]: 1,
    };
  },
};
"#,
        ),
    ];
    let diags = check_modules(&files, "handler.ts");
    assert_clean(&diags, "computed keys -> ProxyHandler descriptor return");
}

/// `Object.defineProperties(obj, { [SET]: ... })` — the `immer`
/// `utils/common.ts:254` family (`TS2353` excess on a computed key).
#[test]
fn computed_key_in_define_properties_descriptor_map() {
    let diags = check_single(
        r#"
const accessorName = "set";
declare const target: {};
declare const setter: () => void;
const updated = Object.defineProperties(target, {
  [accessorName]: { value: setter, writable: true },
});
"#,
    );
    assert_clean(&diags, "computed key in defineProperties map");
}

// ─── Group B: transitive heritage member retention (immer FP #3/#4) ──────────

/// `Set#values`/`#entries` return `SetIterator<T>`, which inherits `next`
/// from `Iterator` two heritage levels up; the result is assignable to
/// `IterableIterator<T>` (no `TS2741` "missing next").
#[test]
fn set_iterator_results_retain_inherited_next() {
    let diags = check_single(
        r#"
const numbers = new Set<number>();
const valueIter: IterableIterator<number> = numbers.values();
const entryIter: IterableIterator<[number, number]> = numbers.entries();
const keyIter: IterableIterator<number> = numbers.keys();
"#,
    );
    assert_clean(&diags, "Set iterator results -> IterableIterator");
}

/// Same invariant for `Map` iterator results.
#[test]
fn map_iterator_results_retain_inherited_next() {
    let diags = check_single(
        r#"
const lookup = new Map<string, number>();
const entryIter: IterableIterator<[string, number]> = lookup.entries();
const keyIter: IterableIterator<string> = lookup.keys();
const valueIter: IterableIterator<number> = lookup.values();
"#,
    );
    assert_clean(&diags, "Map iterator results -> IterableIterator");
}

/// Same invariant for `Array` iterator results.
#[test]
fn array_iterator_results_retain_inherited_next() {
    let diags = check_single(
        r#"
const items = [10, 20, 30];
const valueIter: IterableIterator<number> = items.values();
const entryIter: IterableIterator<[number, number]> = items.entries();
const keyIter: IterableIterator<number> = items.keys();
"#,
    );
    assert_clean(&diags, "Array iterator results -> IterableIterator");
}

/// A bare `SetIterator<T>` reference exposes the transitively-inherited
/// `next` directly and is assignable to `IterableIterator<T>`.
#[test]
fn set_iterator_reference_exposes_inherited_next_member() {
    let diags = check_single(
        r#"
declare const cursor: SetIterator<number>;
const step = cursor.next();
const iterable: IterableIterator<number> = cursor;
"#,
    );
    assert_clean(&diags, "SetIterator reference -> next + IterableIterator");
}

/// Destructuring iteration over `Map#entries` resolves both tuple slots —
/// the inherited iterator protocol drives `for..of` element typing.
#[test]
fn for_of_over_map_entries_resolves_tuple_element_types() {
    let diags = check_single(
        r#"
const registry = new Map<string, number>();
for (const [name, count] of registry.entries()) {
  const label: string = name;
  const total: number = count;
}
"#,
    );
    assert_clean(&diags, "for-of over Map entries tuple destructure");
}
