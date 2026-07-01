//! ES5 lowering of **instance private auto-accessors** (`accessor #name`).
//!
//! Structural rule: a private auto-accessor lowers, at every sub-ES2022 target,
//! to a backing-storage `WeakMap` (`_C_y_accessor_storage`) plus a branded
//! get/set helper pair (`_C_y_get` / `_C_y_set`) that read and write that
//! storage; access to `this.#y` routes through the instance brand exactly like
//! an ordinary private accessor
//! (`__classPrivateFieldGet(this, _C_instances, "a", _C_y_get)`). The prototype
//! never gains an `Object.defineProperty` for a private auto-accessor. Before
//! this fix the ES5 class transformer dropped the member entirely and emitted
//! syntactically invalid `this.` for the read.
//!
//! The trailing helper-assignment chain and the hoisted `var` list follow
//! **source member order** — a private accessor declared before a private method
//! assigns its helper first — matching tsc, which walks the class body once.
//!
//! Ground truth captured from `tsc` 6.0.2 `--target es5 --module esnext`. Binder
//! names vary across tests so the behaviour keys on the structural
//! private-auto-accessor shape, not on any identifier spelling.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print;

fn emit_es5(source: &str) -> String {
    let opts = PrintOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    parse_and_lower_print(source, opts)
}

/// A basic instance private auto-accessor lowers to storage + branded get/set,
/// and reads/writes route through the instance brand.
#[test]
fn basic_private_auto_accessor_read_and_write() {
    let source = r#"export class Cell {
  accessor #v = 2;
  read() { return this.#v; }
  write(next: number) { this.#v = next; }
}"#;
    let out = emit_es5(source);

    // The dropped-member regression: no invalid `this.` and no raw `#v`.
    assert!(
        !out.contains("this.;") && !out.contains("this. ="),
        "private auto-accessor must not emit an invalid empty property access.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("#v"),
        "the private name must be fully lowered.\nOutput:\n{out}"
    );

    assert!(
        out.contains("var _Cell_instances, _Cell_v_get, _Cell_v_set, _Cell_v_accessor_storage;"),
        "hoisted var list: brand, get, set, then storage.\nOutput:\n{out}"
    );
    assert!(
        out.contains("_Cell_instances.add(this);")
            && out.contains("_Cell_v_accessor_storage.set(this, 2);"),
        "constructor brands the instance and seeds the storage.\nOutput:\n{out}"
    );
    assert!(
        out.contains("return __classPrivateFieldGet(this, _Cell_instances, \"a\", _Cell_v_get);"),
        "read routes through the accessor brand.\nOutput:\n{out}"
    );
    assert!(
        out.contains("__classPrivateFieldSet(this, _Cell_instances, next, \"a\", _Cell_v_set);"),
        "write routes through the accessor brand.\nOutput:\n{out}"
    );
    assert!(
        out.contains(
            "_Cell_instances = new WeakSet(), _Cell_v_accessor_storage = new WeakMap(), _Cell_v_get = function _Cell_v_get() { return __classPrivateFieldGet(this, _Cell_v_accessor_storage, \"f\"); }, _Cell_v_set = function _Cell_v_set(value) { __classPrivateFieldSet(this, _Cell_v_accessor_storage, value, \"f\"); };"
        ),
        "trailing chain: brand, storage, then get/set helpers.\nOutput:\n{out}"
    );
}

/// A private auto-accessor without an initializer seeds the storage with
/// `void 0`.
#[test]
fn private_auto_accessor_without_initializer() {
    let source = r#"export class Slot {
  accessor #payload;
  peek() { return this.#payload; }
}"#;
    let out = emit_es5(source);
    assert!(
        out.contains("_Slot_payload_accessor_storage.set(this, void 0);"),
        "uninitialized storage is seeded with void 0.\nOutput:\n{out}"
    );
    assert!(
        out.contains(
            "return __classPrivateFieldGet(this, _Slot_instances, \"a\", _Slot_payload_get);"
        ),
        "read routes through the accessor brand.\nOutput:\n{out}"
    );
}

/// Two private auto-accessors keep their storages and helpers in source order.
#[test]
fn multiple_private_auto_accessors_keep_source_order() {
    let source = r#"export class Pair {
  accessor #left = 1;
  accessor #right = 2;
}"#;
    let out = emit_es5(source);
    assert!(
        out.contains(
            "var _Pair_instances, _Pair_left_get, _Pair_left_set, _Pair_right_get, _Pair_right_set, _Pair_left_accessor_storage, _Pair_right_accessor_storage;"
        ),
        "helpers in source order, storages grouped after them.\nOutput:\n{out}"
    );
}

/// A public and a private auto-accessor coexist: the public one keeps its
/// prototype `Object.defineProperty`; the private one is branded, with the two
/// storages in source order.
#[test]
fn mixed_public_and_private_auto_accessors() {
    let source = r#"export class Mixed {
  accessor open = 1;
  accessor #closed = 2;
  m() { return this.open + this.#closed; }
}"#;
    let out = emit_es5(source);
    assert!(
        out.contains("Object.defineProperty(Mixed.prototype, \"open\""),
        "the public auto-accessor keeps its prototype descriptor.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Mixed.prototype, \"closed\"")
            && !out.contains("Mixed.prototype, \"#closed\""),
        "the private auto-accessor must NOT define a prototype property.\nOutput:\n{out}"
    );
    assert!(
        out.contains(
            "this.open + __classPrivateFieldGet(this, _Mixed_instances, \"a\", _Mixed_closed_get)"
        ),
        "public access stays a plain member; private access is branded.\nOutput:\n{out}"
    );
    assert!(
        out.contains(
            "_Mixed_instances = new WeakSet(), _Mixed_open_accessor_storage = new WeakMap(), _Mixed_closed_accessor_storage = new WeakMap(), _Mixed_closed_get = function"
        ),
        "both accessor storages are allocated in source order.\nOutput:\n{out}"
    );
}

/// A private field, a private auto-accessor, and a private method interleave in
/// source order in the trailing helper chain — the private auto-accessor's
/// get/set land at its own position, before the later method.
#[test]
fn private_auto_accessor_interleaves_with_field_and_method_in_source_order() {
    let source = r#"export class Svc {
  #flag = 0;
  accessor #count = 1;
  #tick() { return this.#flag; }
  run() { return this.#tick() + this.#count; }
}"#;
    let out = emit_es5(source);
    assert!(
        out.contains(
            "_Svc_flag = new WeakMap(), _Svc_instances = new WeakSet(), _Svc_count_accessor_storage = new WeakMap(), _Svc_count_get = function _Svc_count_get() { return __classPrivateFieldGet(this, _Svc_count_accessor_storage, \"f\"); }, _Svc_count_set = function _Svc_count_set(value) { __classPrivateFieldSet(this, _Svc_count_accessor_storage, value, \"f\"); }, _Svc_tick = function _Svc_tick() { return __classPrivateFieldGet(this, _Svc_flag, \"f\"); };"
        ),
        "source order: field storage, brand, accessor storage, accessor get/set, then the later method.\nOutput:\n{out}"
    );
}

/// A compound assignment to a private auto-accessor expands to a branded
/// get + set, both keyed on the instance brand.
#[test]
fn compound_assignment_to_private_auto_accessor() {
    let source = r#"export class Counter {
  accessor #n = 0;
  bump() { this.#n += 1; return this.#n; }
}"#;
    let out = emit_es5(source);
    assert!(
        out.contains(
            "__classPrivateFieldSet(this, _Counter_instances, __classPrivateFieldGet(this, _Counter_instances, \"a\", _Counter_n_get) + 1, \"a\", _Counter_n_set);"
        ),
        "compound assignment reads then writes through the accessor brand.\nOutput:\n{out}"
    );
}

/// A private auto-accessor in a derived class still lowers correctly.
#[test]
fn private_auto_accessor_in_derived_class() {
    let source = r#"declare class Base {}
export class Derived extends Base {
  accessor #tag = 9;
  val() { return this.#tag; }
}"#;
    let out = emit_es5(source);
    // A derived-class constructor captures `this` into `_this` after the super
    // chain, so the brand/storage stores key on `_this`.
    assert!(
        out.contains("_Derived_instances.add(_this);")
            && out.contains("_Derived_tag_accessor_storage.set(_this, 9);"),
        "derived-class constructor brands and seeds after the super chain.\nOutput:\n{out}"
    );
    assert!(
        out.contains(
            "return __classPrivateFieldGet(this, _Derived_instances, \"a\", _Derived_tag_get);"
        ),
        "read routes through the accessor brand.\nOutput:\n{out}"
    );
}

/// A private get/set accessor declared *before* a private method assigns its
/// helper first — the trailing chain follows source order, not a
/// methods-before-accessors grouping.
#[test]
fn private_accessor_before_method_keeps_source_order() {
    let source = r#"export class Ordered {
  #store = 1;
  get #view() { return this.#store; }
  set #view(x: number) { this.#store = x; }
  #compute() { return this.#store; }
  use() { return this.#view + this.#compute(); }
}"#;
    let out = emit_es5(source);
    let view_get = out
        .find("_Ordered_view_get = function")
        .expect("view get helper");
    let compute = out
        .find("_Ordered_compute = function")
        .expect("compute helper");
    assert!(
        view_get < compute,
        "the accessor helper (declared first) must be assigned before the method helper.\nOutput:\n{out}"
    );
}
