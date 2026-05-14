//! Tests for interface-extends-Array patterns used in recursive JSON types.
//!
//! Covers issue #6528: `string[] <: JSONValue` where `JSONValue` uses
//! `interface JSONArray extends Array<JSONValue> {}` returns a false TS2322.
//!
//! Structural rule: when `interface I extends Array<T>{}` with no own properties,
//! any `T[]` is assignable to `I` (and to unions containing `I`) because `I` is
//! structurally equivalent to `Array<T>` and array element subtyping applies.

use std::path::PathBuf;
use std::sync::OnceLock;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_with_libs, load_compiled_lib_files, load_default_lib_files,
};

use std::sync::Arc;

/// Load lib files from the bundled website lib directory (always available in-tree).
/// These are the same compiled TypeScript libs used by the binary.
fn load_website_libs(names: &[&str]) -> Vec<Arc<LibFile>> {
    // The website lib lives at <workspace_root>/crates/tsz-website/src/lib/
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let website_lib = manifest.join("../tsz-website/src/lib");
    let mut out = Vec::new();
    for &name in names {
        let p = website_lib.join(name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            out.push(Arc::new(LibFile::from_source(name.to_string(), content)));
        }
    }
    out
}

/// Returns the same lib files loaded by the binary for ES2024 target (es2024.full).
///
/// The binary's `PrinterOptions` defaults to `ScriptTarget::ES2024`, which resolves
/// to `lib.es2024.full.d.ts`. That file references: es2024, dom, webworker.importscripts,
/// scripthost, dom.iterable, dom.asynciterable — exactly what this list covers.
/// Unlike `ESNext`, ES2024 does NOT include `lib.esnext.iterator.d.ts` (which adds
/// `map`/`filter`/etc. to `IteratorObject`). The missing `esnext.iterator` is what
/// causes `ArrayIterator` to be modelled differently and triggers the false TS2322.
fn website_libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(|| {
        load_website_libs(&[
            "lib.es5.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2015.collection.d.ts",
            "lib.es2015.generator.d.ts",
            "lib.es2015.iterable.d.ts",
            "lib.es2015.promise.d.ts",
            "lib.es2015.proxy.d.ts",
            "lib.es2015.reflect.d.ts",
            "lib.es2015.symbol.d.ts",
            "lib.es2015.symbol.wellknown.d.ts",
            "lib.es2016.array.include.d.ts",
            "lib.es2016.intl.d.ts",
            "lib.es2017.arraybuffer.d.ts",
            "lib.es2017.date.d.ts",
            "lib.es2017.intl.d.ts",
            "lib.es2017.object.d.ts",
            "lib.es2017.sharedmemory.d.ts",
            "lib.es2017.string.d.ts",
            "lib.es2017.typedarrays.d.ts",
            "lib.es2018.asyncgenerator.d.ts",
            "lib.es2018.asynciterable.d.ts",
            "lib.es2018.intl.d.ts",
            "lib.es2018.promise.d.ts",
            "lib.es2018.regexp.d.ts",
            "lib.es2019.array.d.ts",
            "lib.es2019.intl.d.ts",
            "lib.es2019.object.d.ts",
            "lib.es2019.string.d.ts",
            "lib.es2019.symbol.d.ts",
            "lib.es2020.bigint.d.ts",
            "lib.es2020.date.d.ts",
            "lib.es2020.intl.d.ts",
            "lib.es2020.number.d.ts",
            "lib.es2020.promise.d.ts",
            "lib.es2020.sharedmemory.d.ts",
            "lib.es2020.string.d.ts",
            "lib.es2020.symbol.wellknown.d.ts",
            "lib.es2021.intl.d.ts",
            "lib.es2021.promise.d.ts",
            "lib.es2021.string.d.ts",
            "lib.es2021.weakref.d.ts",
            "lib.es2022.array.d.ts",
            "lib.es2022.error.d.ts",
            "lib.es2022.intl.d.ts",
            "lib.es2022.object.d.ts",
            "lib.es2022.regexp.d.ts",
            "lib.es2022.string.d.ts",
            "lib.es2023.array.d.ts",
            "lib.es2023.collection.d.ts",
            "lib.es2023.intl.d.ts",
            "lib.es2024.arraybuffer.d.ts",
            "lib.es2024.collection.d.ts",
            "lib.es2024.object.d.ts",
            "lib.es2024.promise.d.ts",
            "lib.es2024.regexp.d.ts",
            "lib.es2024.sharedmemory.d.ts",
            "lib.es2024.string.d.ts",
            "lib.dom.d.ts",
            "lib.dom.iterable.d.ts",
            "lib.dom.asynciterable.d.ts",
            "lib.webworker.importscripts.d.ts",
            "lib.scripthost.d.ts",
            "lib.decorators.d.ts",
            "lib.decorators.legacy.d.ts",
        ])
    })
}

fn stripped_libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

fn full_libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(|| {
        // Load the same lib set the binary uses for --target esnext
        load_compiled_lib_files(&[
            "lib.es5.d.ts",
            "lib.es2015.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2015.collection.d.ts",
            "lib.es2015.iterable.d.ts",
            "lib.es2015.generator.d.ts",
            "lib.es2015.promise.d.ts",
            "lib.es2015.proxy.d.ts",
            "lib.es2015.reflect.d.ts",
            "lib.es2015.symbol.d.ts",
            "lib.es2015.symbol.wellknown.d.ts",
            "lib.es2016.array.include.d.ts",
            "lib.es2017.d.ts",
            "lib.es2017.date.d.ts",
            "lib.es2017.object.d.ts",
            "lib.es2017.sharedmemory.d.ts",
            "lib.es2017.string.d.ts",
            "lib.es2017.intl.d.ts",
            "lib.es2017.typedarrays.d.ts",
            "lib.es2018.asyncgenerator.d.ts",
            "lib.es2018.asynciterable.d.ts",
            "lib.es2018.intl.d.ts",
            "lib.es2018.promise.d.ts",
            "lib.es2018.regexp.d.ts",
            "lib.es2019.array.d.ts",
            "lib.es2019.object.d.ts",
            "lib.es2019.string.d.ts",
            "lib.es2019.symbol.d.ts",
            "lib.es2020.bigint.d.ts",
            "lib.es2020.date.d.ts",
            "lib.es2020.number.d.ts",
            "lib.es2020.promise.d.ts",
            "lib.es2020.string.d.ts",
            "lib.es2020.symbol.wellknown.d.ts",
            "lib.es2021.intl.d.ts",
            "lib.es2021.promise.d.ts",
            "lib.es2021.string.d.ts",
            "lib.es2021.weakref.d.ts",
            "lib.es2022.array.d.ts",
            "lib.es2022.error.d.ts",
            "lib.es2022.intl.d.ts",
            "lib.es2022.object.d.ts",
            "lib.es2022.regexp.d.ts",
            "lib.es2022.string.d.ts",
            "lib.es2023.array.d.ts",
            "lib.es2023.collection.d.ts",
            "lib.es2023.intl.d.ts",
            "lib.es2024.arraybuffer.d.ts",
            "lib.es2024.collection.d.ts",
            "lib.es2024.object.d.ts",
            "lib.es2024.promise.d.ts",
            "lib.es2024.regexp.d.ts",
            "lib.es2024.sharedmemory.d.ts",
            "lib.es2024.string.d.ts",
            "lib.esnext.array.d.ts",
            "lib.esnext.collection.d.ts",
            "lib.esnext.decorators.d.ts",
            "lib.esnext.disposable.d.ts",
            "lib.esnext.error.d.ts",
            "lib.esnext.intl.d.ts",
            "lib.esnext.iterator.d.ts",
            "lib.esnext.promise.d.ts",
            "lib.esnext.sharedmemory.d.ts",
            "lib.esnext.temporal.d.ts",
            "lib.esnext.typedarrays.d.ts",
        ])
    })
}

fn check_with_lib(source: &str, lib: &[Arc<LibFile>]) -> Vec<(u32, String)> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        lib,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn check(source: &str) -> Vec<(u32, String)> {
    check_with_lib(source, stripped_libs())
}

fn ts2322(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(code, _)| *code == 2322).collect()
}

/// tsc rule: T[] is assignable to a union containing `interface I extends Array<T>`.
/// No TS2322 should be emitted for the exact repro from issue #6528.
#[test]
fn json_value_interface_extends_array_no_false_ts2322() {
    let source = r#"
type JSONValue =
  | string
  | number
  | boolean
  | null
  | JSONArray
  | JSONObject;

interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const json: JSONValue = {
  name: "test",
  values: [1, 2, { nested: true }],
  config: {
    enabled: true,
    items: ["a", "b"]
  }
};
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for JSONValue pattern with interface extends Array. Got: {diags:#?}"
    );
}

/// Same test with full TypeScript libs (matching the binary's behavior).
#[test]
fn json_value_interface_extends_array_no_false_ts2322_full_libs() {
    let libs = full_libs();
    if libs.is_empty() {
        // Skip if full libs aren't available
        return;
    }
    let source = r#"
type JSONValue =
  | string
  | number
  | boolean
  | null
  | JSONArray
  | JSONObject;

interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const json: JSONValue = {
  name: "test",
  values: [1, 2, { nested: true }],
  config: {
    enabled: true,
    items: ["a", "b"]
  }
};
"#;
    let diags = check_with_lib(source, libs);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for JSONValue pattern with full libs. Got: {diags:#?}"
    );
}

/// Same rule with a different alias name — must not be hardcoded to `JSONValue`.
#[test]
fn json_value_different_alias_name_no_false_ts2322() {
    let source = r#"
type DataValue =
  | string
  | number
  | boolean
  | null
  | DataList
  | DataObject;

interface DataList extends Array<DataValue> {}
interface DataObject { [key: string]: DataValue }

const d: DataValue = {
  x: "hello",
  items: [1, 2, 3],
};
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for DataValue pattern with interface extends Array. Got: {diags:#?}"
    );
}

/// string[] is directly assignable to a union containing `JSONArray`.
#[test]
fn string_array_assignable_to_json_value_union() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = ["a", "b", "c"];
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected string[] to be assignable to JSONValue union. Got: {diags:#?}"
    );
}

/// number[] is assignable to `JSONValue` (number is a union member).
#[test]
fn number_array_assignable_to_json_value_union() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = [1, 2, 3];
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected number[] to be assignable to JSONValue union. Got: {diags:#?}"
    );
}

/// Nested arrays are also valid `JSONValue`.
#[test]
fn nested_array_assignable_to_json_value() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = [[1, 2], ["a", "b"], [true, false]];
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected nested arrays to be assignable to JSONValue. Got: {diags:#?}"
    );
}

/// Mixed array with objects is valid `JSONValue`.
#[test]
fn mixed_array_with_objects_assignable_to_json_value() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = [1, "two", true, null, { key: "value" }];
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected mixed array to be assignable to JSONValue. Got: {diags:#?}"
    );
}

/// Variable annotation case: variable typed as `JSONValue` holding a string array.
#[test]
fn variable_string_array_to_json_value_no_ts2322() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

function toJson(v: JSONValue): JSONValue { return v; }
const items: string[] = ["a", "b"];
const result: JSONValue = items;
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected string[] variable to be assignable to JSONValue. Got: {diags:#?}"
    );
}

/// Union type with fewer members still works (regression guard).
#[test]
fn two_member_json_value_union_works() {
    let source = r#"
type JSONValue = JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = [];
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected empty array to be assignable to two-member JSONValue union. Got: {diags:#?}"
    );
}

/// Truly incompatible types MUST still produce TS2322 (no over-acceptance).
#[test]
fn function_not_assignable_to_json_value() {
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const fn_val: JSONValue = () => {};
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert!(
        !errors.is_empty(),
        "Expected TS2322 for function assigned to JSONValue. Got: {diags:#?}"
    );
}

/// Same test with full libs to verify false positive doesn't occur with full Array definition.
#[test]
fn string_array_assignable_to_json_value_full_libs() {
    let libs = full_libs();
    if libs.is_empty() {
        return;
    }
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = ["a", "b", "c"];
const arr2: JSONValue = [1, 2, 3];
const arr3: JSONValue = [1, "two", true, null];
"#;
    let diags = check_with_lib(source, libs);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected arrays to be assignable to JSONValue with full libs. Got: {diags:#?}"
    );
}

/// Exact repro from issue #6528 with website (binary-equivalent) libs.
#[test]
fn json_value_nested_object_with_arrays_website_libs_no_false_ts2322() {
    let libs = website_libs();
    if libs.is_empty() {
        return;
    }
    let source = r#"
type JSONValue =
  | string
  | number
  | boolean
  | null
  | JSONArray
  | JSONObject;

interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const json: JSONValue = {
  name: "test",
  values: [1, 2, { nested: true }],
  config: {
    enabled: true,
    items: ["a", "b"]
  }
};
"#;
    let diags = check_with_lib(source, libs);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for JSONValue pattern with website libs. Got: {diags:#?}"
    );
}

/// Direct string[] assignment to `JSONValue` — must pass with website libs.
#[test]
fn string_array_website_libs_no_false_ts2322() {
    let libs = website_libs();
    if libs.is_empty() {
        return;
    }
    let source = r#"
type JSONValue = string | number | boolean | null | JSONArray | JSONObject;
interface JSONArray extends Array<JSONValue> {}
interface JSONObject { [key: string]: JSONValue }

const arr: JSONValue = ["a", "b", "c"];
const arr2: JSONValue = [1, 2, 3];
"#;
    let diags = check_with_lib(source, libs);
    let errors = ts2322(&diags);
    assert!(
        errors.is_empty(),
        "Expected array literals to be assignable to JSONValue with website libs. Got: {diags:#?}"
    );
}
