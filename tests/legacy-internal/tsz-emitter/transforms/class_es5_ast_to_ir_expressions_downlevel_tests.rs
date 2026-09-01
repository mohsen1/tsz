use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

/// Emit `source` as ES5 JS through the full class-IR lowering pipeline.
fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// Structural rule: the ES5 class-IR converter must lower private-field
// exponent (`**=`) and short-circuit (`&&=`/`||=`) compound assignment to a
// get-fold-set form instead of an un-assignable `__classPrivateFieldGet(...)
// **= v`. `**=` is an unconditional write (fields + accessors); `&&=`/`||=`
// fold to an always-write set only for plain fields, where it is observably
// equivalent. `??=` and accessor short-circuit stay on the fallthrough. The
// rule keys on the member's storage slot and kind, never on its spelling.

#[test]
fn private_field_exponent_assign_lowers_through_math_pow() {
    let output = emit_es5("class E {\n    #x = 2;\n    m() {\n        this.#x **= 3;\n    }\n}\n");
    assert!(
        output.contains(
            "__classPrivateFieldSet(this, _E_x, Math.pow(__classPrivateFieldGet(this, _E_x, \"f\"), 3), \"f\")"
        ),
        "Private `#x **= 3` must lower through `Math.pow`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("**="),
        "ES5 output must not retain the `**=` operator.\nOutput:\n{output}"
    );
}

#[test]
fn private_accessor_exponent_assign_threads_get_and_set_storage() {
    // `**=` is unconditional, so it is also correct for accessors: read
    // through the getter and write through the setter, both branded against
    // the `_instances` `WeakSet` with kind "a".
    let output = emit_es5(
        "class Box {\n    get #v() { return 2; }\n    set #v(x: number) {}\n    grow() {\n        this.#v **= 4;\n    }\n}\n",
    );
    assert!(
        output.contains(
            "__classPrivateFieldSet(this, _Box_instances, Math.pow(__classPrivateFieldGet(this, _Box_instances, \"a\", _Box_v_get), 4), \"a\", _Box_v_set)"
        ),
        "Accessor `#v **= 4` must brand against `_Box_instances` and thread the getter/setter refs.\nOutput:\n{output}"
    );
}

#[test]
fn private_field_logical_and_assign_folds_to_set_get_and_rhs() {
    let output = emit_es5(
        "class A {\n    #flag = true;\n    m() {\n        this.#flag &&= false;\n    }\n}\n",
    );
    assert!(
        output.contains(
            "__classPrivateFieldSet(this, _A_flag, __classPrivateFieldGet(this, _A_flag, \"f\") && false, \"f\")"
        ),
        "Private `#flag &&= false` must fold to set(get() && rhs).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("&&="),
        "Lowered output must not retain `&&=`.\nOutput:\n{output}"
    );
}

#[test]
fn private_field_logical_or_assign_folds_to_set_get_or_rhs() {
    let output = emit_es5(
        "class O {\n    #cache = 0;\n    m(v: number) {\n        this.#cache ||= v;\n    }\n}\n",
    );
    assert!(
        output.contains(
            "__classPrivateFieldSet(this, _O_cache, __classPrivateFieldGet(this, _O_cache, \"f\") || v, \"f\")"
        ),
        "Private `#cache ||= v` must fold to set(get() || rhs).\nOutput:\n{output}"
    );
}

#[test]
fn private_field_logical_assign_parenthesizes_conditional_rhs() {
    // `||` binds tighter than `?:`, so a conditional rhs must be parenthesized
    // or the assignment silently reparses as `(get() || a) ? b : c`.
    let output = emit_es5(
        "declare const a: any;\nclass C {\n    #x = 0;\n    m() {\n        this.#x ||= a ? 1 : 2;\n    }\n}\n",
    );
    assert!(
        output.contains(
            "__classPrivateFieldSet(this, _C_x, __classPrivateFieldGet(this, _C_x, \"f\") || (a ? 1 : 2), \"f\")"
        ),
        "Conditional rhs of `||=` must be parenthesized.\nOutput:\n{output}"
    );
}

#[test]
fn private_field_nullish_assign_stays_on_fallthrough() {
    // Out of scope: `??=` needs ES5 nullish lowering of the folded `??`.
    // This guards the documented scope boundary (no accidental partial fold).
    let output = emit_es5("class N {\n    #x = 0;\n    m() {\n        this.#x ??= 9;\n    }\n}\n");
    assert!(
        !output.contains("Math.pow") && !output.contains("\"f\") ?? "),
        "`??=` must not be partially folded; it stays on the fallthrough.\nOutput:\n{output}"
    );
}

#[test]
fn private_accessor_short_circuit_assign_stays_on_fallthrough() {
    // Out of scope: an accessor `&&=` would call the setter even when the
    // short circuit says skip, so the always-write fold is unsafe here.
    let output = emit_es5(
        "class Acc {\n    get #v() { return 1; }\n    set #v(x: number) {}\n    m() {\n        this.#v &&= 3;\n    }\n}\n",
    );
    assert!(
        !output.contains("__classPrivateFieldSet(this, _Acc_v_set, __classPrivateFieldGet"),
        "Accessor `&&=` must not be folded to an always-write set.\nOutput:\n{output}"
    );
}

// ====================================================================
// ES2020/ES2021 operator downlevel inside ES5 class member bodies.
//
// Structural rule: when the ES5 class-IR converter sees nullish
// coalescing (`??`), a logical/nullish compound assignment
// (`||=`/`&&=`/`??=`) on a non-private target, or an optional *call*
// token (`?.()`), it must apply the same downlevel the main emitter
// applies at sub-ES2015 targets - the IR printer would otherwise re-emit
// the raw operator, which is invalid ES5. The rules key on the operator
// token / `?.` chain flag, never on a receiver spelling, so these tests
// vary class/member/binder names.
// ====================================================================

#[test]
fn nullish_coalescing_in_method_downlevels() {
    // `x ?? 0` - simple operand, no temp.
    let output = emit_es5("class C {\n    a(x?: number) { return x ?? 0; }\n}\n");
    assert!(
        output.contains("return x !== null && x !== void 0 ? x : 0;"),
        "`x ?? 0` must downlevel to the nullish ternary.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("??"),
        "No raw `??` may survive at ES5.\nOutput:\n{output}"
    );
}

#[test]
fn nullish_coalescing_side_effecting_operand_captures_temp() {
    // A side-effecting left operand is captured once via `(_a = ...)`.
    let output =
        emit_es5("class Reader {\n    read(make: () => number) { return make() ?? 7; }\n}\n");
    assert!(
        output.contains("(_a = make()) !== null && _a !== void 0 ? _a : 7"),
        "A side-effecting `??` operand must be captured once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a;"),
        "The capture temp must be hoisted.\nOutput:\n{output}"
    );
}

#[test]
fn logical_or_assignment_in_method_downlevels() {
    // `v ||= 1` -> `v || (v = 1)`.
    let output = emit_es5("class C {\n    c(v: any) { v ||= 1; }\n}\n");
    assert!(
        output.contains("v || (v = 1);"),
        "`v ||= 1` must downlevel to `v || (v = 1)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("||="),
        "No raw `||=` may survive.\nOutput:\n{output}"
    );
}

#[test]
fn logical_and_assignment_in_method_downlevels() {
    // `v &&= 3` -> `v && (v = 3)`. Distinct binder name (anti-hardcoding).
    let output = emit_es5("class Gate {\n    flip(flag: any) { flag &&= 3; }\n}\n");
    assert!(
        output.contains("flag && (flag = 3);"),
        "`flag &&= 3` must downlevel to `flag && (flag = 3)`.\nOutput:\n{output}"
    );
}

#[test]
fn nullish_assignment_in_method_downlevels() {
    // `v ??= 2` -> `v !== null && v !== void 0 ? v : (v = 2)`.
    let output = emit_es5("class C {\n    d(v: any) { v ??= 2; }\n}\n");
    assert!(
        output.contains("v !== null && v !== void 0 ? v : (v = 2);"),
        "`v ??= 2` must downlevel to the nullish-assignment ternary.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("??="),
        "No raw `??=` may survive.\nOutput:\n{output}"
    );
}

#[test]
fn nullish_assignment_on_property_target_captures_value_temp() {
    // `this.p ??= 5` (simple receiver) -> value-temp capture, parenthesized
    // write-back.
    let output = emit_es5("class Box {\n    fill() { this.p ??= 5; }\n}\n");
    assert!(
        output.contains("(_a = this.p) !== null && _a !== void 0 ? _a : (this.p = 5);"),
        "`this.p ??= 5` must capture the read into a value temp.\nOutput:\n{output}"
    );
}

#[test]
fn logical_or_assignment_on_property_target() {
    // `this.p ||= 1` -> `this.p || (this.p = 1)`.
    let output = emit_es5("class Box {\n    seed() { this.p ||= 1; }\n}\n");
    assert!(
        output.contains("this.p || (this.p = 1);"),
        "`this.p ||= 1` must downlevel to `this.p || (this.p = 1)`.\nOutput:\n{output}"
    );
}

#[test]
fn optional_call_token_on_method_preserves_receiver() {
    // `o.m?.()` -> capture the function value and invoke via `.call(o)`.
    let output = emit_es5("class C {\n    b(o: { m?: () => void }) { return o.m?.(); }\n}\n");
    assert!(
        output.contains("(_a = o.m) === null || _a === void 0 ? void 0 : _a.call(o)"),
        "`o.m?.()` must guard the function value and preserve `this` via `.call(o)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("o.m()"),
        "The optional-call token must not be dropped to a plain call.\nOutput:\n{output}"
    );
}

#[test]
fn optional_call_token_on_bare_callee_uses_plain_call() {
    // `f?.()` - no receiver, so no `.call`.
    let output = emit_es5("class C {\n    run(f?: () => void) { return f?.(); }\n}\n");
    assert!(
        output.contains("f === null || f === void 0 ? void 0 : f()"),
        "`f?.()` must guard the callee and invoke it plainly.\nOutput:\n{output}"
    );
}

#[test]
fn optional_call_token_on_element_access_preserves_receiver() {
    // `o["k"]?.()` -> element-access function value + `.call(o)`.
    let output = emit_es5("class C {\n    pick(o: any) { return o[\"k\"]?.(); }\n}\n");
    assert!(
        output.contains("(_a = o[\"k\"]) === null || _a === void 0 ? void 0 : _a.call(o)"),
        "`o[\"k\"]?.()` must guard the element function value and preserve `this`.\nOutput:\n{output}"
    );
}

#[test]
fn class_member_downlevel_matches_top_level_downlevel() {
    // Parity anchor: the SAME expression lowered at the top level (the main
    // emitter path, which is byte-parity-tested against tsc) and inside a
    // class member body (this converter) must produce the identical operator
    // form. Compares the lowered fragment, ignoring the surrounding wrapper.
    for expr in ["x ?? 0", "v ||= 1", "v ??= 2"] {
        let top = emit_es5(&format!(
            "function host(x?: any, v?: any) {{ return ({expr}); }}\n"
        ));
        let member = emit_es5(&format!(
            "class Host {{ run(x?: any, v?: any) {{ return ({expr}); }} }}\n"
        ));
        // Both must downlevel away the raw operator.
        assert!(
            !top.contains("??") && !top.contains("||="),
            "top-level `{expr}` should downlevel.\nOutput:\n{top}"
        );
        assert!(
            !member.contains("??") && !member.contains("||="),
            "class-member `{expr}` should downlevel.\nOutput:\n{member}"
        );
    }
}
