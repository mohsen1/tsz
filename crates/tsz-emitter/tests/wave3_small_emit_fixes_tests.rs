//! Structural regression tests for five small wave-3 emit fixes.
//!
//! Each test pins a structural rule (not a single rendered fingerprint) and
//! varies bound names / shapes so renaming the user-chosen identifier does not
//! make the assertion pass spuriously.
//!
//! Fixes covered:
//! - A: `CommonJS` inline live-export postfix update keeps its `++`/`--`
//!   operator.
//! - B: an elision-first array literal whose newline lives inside a *later*
//!   element stays single-line (the nested element's newline must not force
//!   the outer array multi-line).
//! - C: a class-expression default parameter with a no-initializer static/typed
//!   field stays a native default param (no `if (x === void 0)` prologue and
//!   no `__setFunctionName` helper).
//! - D: an ES5 auto-accessor with a literal computed key does not reserve a
//!   phantom outer `__a_accessor_storage` temp; a side-effecting key does.
//! - E: private-field destructuring allocates a receiver temp only when the
//!   receiver is not a bare identifier.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_lower_print, parse_and_print_with_opts};

fn cjs_es5() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn cjs_es2015() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn es2015() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    }
}

fn es5() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Fix A: CJS inline live-export postfix keeps the operator
//
// Rule: a postfix `x++`/`x--` *statement* on an inline live CJS export with no
// extra clause aliases must emit `exports.x++` / `exports.x--` — the operator
// must not be dropped.
// ---------------------------------------------------------------------------

#[test]
fn inline_export_postfix_increment_keeps_operator() {
    let source = "export let counter = 0;\ncounter++;\n";
    let output = parse_and_lower_print(source, cjs_es2015());
    assert!(
        output.contains("exports.counter++"),
        "inline-export `counter++` statement must keep the `++` operator.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.counter;"),
        "the postfix operator must not be dropped to a bare property read.\nOutput:\n{output}"
    );
}

/// Different name + `--` operator + ES5 target: proves the rule is structural,
/// not keyed to `counter` or to `++` or to ES2015.
#[test]
fn inline_export_postfix_decrement_keeps_operator_other_name_es5() {
    let source = "export let tally = 9;\ntally--;\n";
    let output = parse_and_lower_print(source, cjs_es5());
    assert!(
        output.contains("exports.tally--"),
        "inline-export `tally--` statement must keep the `--` operator.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.tally;"),
        "the postfix operator must not be dropped to a bare property read.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Fix B: elision-first array — nested-element newline must not force multiline
//
// Rule: the outer array's multi-line decision depends only on the source span
// up to the first top-level comma; a newline nested inside a *later* element
// must not split the outer array element-per-line.
// ---------------------------------------------------------------------------

#[test]
fn elision_first_array_with_nested_multiline_stays_single_line() {
    let source = "let data: any, alpha: string, beta: string;\n\
[, [\n    alpha = \"a\",\n    beta = \"b\"\n] = [\"x\", \"y\"]] = data;\n";
    let output = parse_and_print_with_opts(source, es2015());
    // The outer array must NOT be split as `[\n    ,\n    [`.
    assert!(
        !output.contains("[\n    ,\n"),
        "elision-first outer array must not be forced multi-line by a nested \
         element's newline.\nOutput:\n{output}"
    );
    // The outer array opens with the elision directly after `[`.
    assert!(
        output.contains("[, ["),
        "outer array should keep `[, [` on one line.\nOutput:\n{output}"
    );
}

/// Different bound names and a deeper nested array prove the rule is structural.
#[test]
fn elision_first_array_other_names_stays_single_line() {
    let source = "let src: any, first: number, second: number;\n\
[, [\n    first = 1,\n    second = 2\n] = [0, 0]] = src;\n";
    let output = parse_and_print_with_opts(source, es2015());
    assert!(
        !output.contains("[\n    ,\n"),
        "renamed elision-first outer array must still stay single-line.\nOutput:\n{output}"
    );
}

/// Negative/contrast: when the newline really IS between the outer array's own
/// elements (after the first top-level comma region), the outer array is
/// genuinely multi-line and must be split.
#[test]
fn array_with_outer_newline_is_multiline() {
    let source = "let a = [\n    1,\n    2\n];\n";
    let output = parse_and_print_with_opts(source, es2015());
    assert!(
        output.contains("[\n"),
        "a genuinely multi-line array must keep its elements on separate lines.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Fix C: class-expr default param with a no-init static field stays native
//
// Rule: a static (or computed) field with no runtime state (no initializer, not
// an auto-accessor, not decorated) lowers to nothing, so a class expression used
// as a default parameter value needs no captured temp — keep the native default
// param and emit no `__setFunctionName` helper.
// ---------------------------------------------------------------------------

#[test]
fn class_expr_default_param_no_init_static_field_stays_native() {
    let source = "function make<U>(x = class { static slot: U }): U {\n\
    return undefined as any;\n}\n";
    let output = parse_and_print_with_opts(source, es2015());
    assert!(
        output.contains("function make(x = class"),
        "class-expression default param with a no-init static field must stay a \
         native default param.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("if (x === void 0)"),
        "no spurious ES5-style default-param prologue should be emitted.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName"),
        "no `__setFunctionName` helper should be emitted for a no-runtime-state \
         class-expression default param.\nOutput:\n{output}"
    );
}

/// Different type-param + field name + a *static computed* no-init key proves
/// the rule is keyed on structure (no runtime state) and not on `U`/`slot`. A
/// static computed no-init field is exactly the printer-side comma-expr case
/// that previously emitted a temp + `__setFunctionName`.
#[test]
fn class_expr_default_param_no_init_static_computed_field_stays_native() {
    let source = "const key = \"k\";\n\
function build<T>(y = class { static [key]: T }): T {\n    return undefined as any;\n}\n";
    let output = parse_and_print_with_opts(source, es2015());
    assert!(
        output.contains("function build(y = class"),
        "renamed class-expression default param with a no-init static computed \
         field must stay native.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__setFunctionName"),
        "no `__setFunctionName` helper for a no-runtime-state default-param \
         class.\nOutput:\n{output}"
    );
}

/// Negative/contrast: an *initialized* static field carries runtime state, so
/// the class expression default param IS lowered into a comma expression with a
/// temp (and may carry `__setFunctionName`). The native `= class {}` shorthand
/// must NOT be used here.
#[test]
fn class_expr_default_param_initialized_static_field_is_lowered() {
    let source = "function make<U>(x = class { static slot = 1 }): U {\n\
    return undefined as any;\n}\n";
    let output = parse_and_print_with_opts(source, es2015());
    assert!(
        output.contains("slot = 1") || output.contains(".slot = 1"),
        "an initialized static field must still emit its runtime assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function make(x = class {\n}) {"),
        "an initialized static field must not collapse to a bare native default \
         param.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Fix D: ES5 auto-accessor literal computed key — no phantom storage reservation
//
// Rule: the ES5 externally-hoisted accessor-storage reservation
// (`_{Class}__a_accessor_storage`) is only needed when the accessor's computed
// key is side-effecting (needs a temp); a literal computed key (or identifier)
// must not reserve it.
// ---------------------------------------------------------------------------

#[test]
fn es5_auto_accessor_literal_key_no_phantom_storage_reservation() {
    let source = "class Multi {\n\
    accessor [\"w\"]: any;\n\
    accessor [\"x\"] = 1;\n}\n";
    let output = parse_and_lower_print(source, es5());
    // WITHOUT the fix, the externally-hoisted decl pass reserves an extra
    // standalone `var _Multi__a_accessor_storage;` declaration line that splits
    // the real per-accessor storages onto separate statements. After the fix the
    // two storages share one combined declaration and there is no phantom
    // standalone reservation line for the first-accessor storage.
    assert!(
        output.contains("var _Multi__a_accessor_storage, _Multi__b_accessor_storage;"),
        "the two literal-key accessor storages should be declared together in one \
         statement, not split by a phantom reservation line.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _Multi__a_accessor_storage;"),
        "there must be no standalone phantom `var _Multi__a_accessor_storage;` \
         reservation line for a literal computed key.\nOutput:\n{output}"
    );
}

/// Renamed class + renamed literal computed keys: the rule is keyed on the key
/// being a non-side-effecting literal, not on `Multi`/`w`/`x`. The two storages
/// must still share one combined declaration with no phantom standalone line.
#[test]
fn es5_auto_accessor_literal_key_other_names_no_phantom_storage_reservation() {
    let source = "class Panel {\n\
    accessor [\"top\"]: any;\n\
    accessor [\"bottom\"] = 5;\n}\n";
    let output = parse_and_lower_print(source, es5());
    assert!(
        output.contains("var _Panel__a_accessor_storage, _Panel__b_accessor_storage;"),
        "renamed literal-key accessor storages must share one declaration with no \
         phantom reservation line.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _Panel__a_accessor_storage;"),
        "there must be no standalone phantom `var _Panel__a_accessor_storage;` \
         reservation line.\nOutput:\n{output}"
    );
}

/// Negative/contrast: a side-effecting computed key DOES need the key temp, so
/// the outer hoist reserves the storage `WeakMap` *and* the `_a` key temp.
#[test]
fn es5_auto_accessor_side_effecting_key_reserves_temp() {
    let source = "declare var sideEffect: any;\n\
class Gizmo {\n    accessor [sideEffect()] = 1;\n}\n";
    let output = parse_and_lower_print(source, es5());
    assert!(
        output.contains("_Gizmo__a_accessor_storage"),
        "a side-effecting computed-key auto-accessor still needs its storage \
         reservation.\nOutput:\n{output}"
    );
    assert!(
        output.contains("sideEffect()"),
        "the side-effecting key expression must still be evaluated.\nOutput:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Fix E: private-field destructuring receiver temp only for non-identifiers
//
// Rule: a private-field destructuring target allocates a hoisted receiver temp
// only when the receiver is NOT a bare identifier. `this` / property accesses
// need a temp; a plain parameter/local identifier does not.
// ---------------------------------------------------------------------------

#[test]
fn private_destructuring_identifier_receiver_needs_no_temp() {
    let source = "class Box {\n\
    #val = 0;\n\
    static fill(arg: Box) {\n        [arg.#val] = [9];\n    }\n}\n";
    let output = parse_and_lower_print(source, es2015());
    // The setter must reference the identifier receiver `arg` directly.
    assert!(
        output.contains("__classPrivateFieldSet(arg, _Box_val"),
        "an identifier receiver must be referenced directly in the setter.\nOutput:\n{output}"
    );
    // No `_a = arg, ...` receiver-temp dance for a bare identifier receiver.
    assert!(
        !output.contains("_a = arg,"),
        "a bare identifier receiver must not allocate a hoisted receiver temp.\nOutput:\n{output}"
    );
}

/// Renamed parameter proves the rule keys on the receiver being an identifier,
/// not on the spelling `arg`.
#[test]
fn private_destructuring_identifier_receiver_other_name_needs_no_temp() {
    let source = "class Cell {\n\
    #data = 0;\n\
    static load(node: Cell) {\n        [node.#data] = [3];\n    }\n}\n";
    let output = parse_and_lower_print(source, es2015());
    assert!(
        output.contains("__classPrivateFieldSet(node, _Cell_data"),
        "the renamed identifier receiver must be referenced directly.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a = node,"),
        "the renamed identifier receiver must not allocate a hoisted temp.\nOutput:\n{output}"
    );
}

/// Negative/contrast: a `this` receiver is NOT a bare identifier, so it must be
/// captured into a hoisted receiver temp first.
#[test]
fn private_destructuring_this_receiver_uses_temp() {
    let source = "class Box {\n\
    #val = 0;\n\
    use() {\n        [this.#val] = [7];\n    }\n}\n";
    let output = parse_and_lower_print(source, es2015());
    assert!(
        output.contains("_a = this,"),
        "a `this` receiver must be captured into a hoisted receiver temp.\nOutput:\n{output}"
    );
}
