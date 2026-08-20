//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir_expressions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2df363e763bd527a6041be3f22d0633cd1b5fba082addb6d2e09f71a0bb02ab8 1621 instance_private_method_call_uses_instances_brand_and_call
    #[test]
    fn instance_private_method_call_uses_instances_brand_and_call() {
        let output = emit_es5(
            "class Counter {\n    #step(n: number) { return n; }\n    bump() { return this.#step(2); }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Counter_instances, \"m\", _Counter_step).call(this, 2)"
            ),
            "Instance `this.#step(2)` must read through the `_instances` brand and invoke via `.call`.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("this.()") && !output.contains(".()"),
            "Lowered output must not contain the invalid bare private callee.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 2df363e763bd527a6041be3f22d0633cd1b5fba082addb6d2e09f71a0bb02ab8

// TSZ_INLINE_TEST_BEGIN 8281a3986b0f3dee9d8c82b4a075a9af0d39180ef5ba3cb3ed6986d8ef035478 1638 instance_private_method_reference_without_call_reads_function_value
    #[test]
    fn instance_private_method_reference_without_call_reads_function_value() {
        // A private method read in value position (not called) lowers to the
        // bare 4-arg get with no `.call`.
        let output = emit_es5(
            "class Registry {\n    #lookup() { return 1; }\n    handle() { const f = this.#lookup; return f; }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Registry_instances, \"m\", _Registry_lookup)"
            ),
            "A private-method reference must read the 4-arg function value.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("_Registry_lookup).call"),
            "A bare reference must not synthesize a `.call`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 8281a3986b0f3dee9d8c82b4a075a9af0d39180ef5ba3cb3ed6986d8ef035478

// TSZ_INLINE_TEST_BEGIN 3b6d08fec15b873814d05225e4ecc1623a5e01c6619ca6b35be6efc3b242c920 1657 instance_private_getter_in_call_position_preserves_receiver
    #[test]
    fn instance_private_getter_in_call_position_preserves_receiver() {
        // Distinct binder names (anti-hardcoding): the call lowering keys on the
        // read slot, not the member spelling.
        let output = emit_es5(
            "class Service {\n    get #handler() { return () => 1; }\n    run() { return this.#handler(); }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldGet(this, _Service_instances, \"a\", _Service_handler_get).call(this)"
            ),
            "A private getter invoked in call position must brand against `_instances` and `.call`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 3b6d08fec15b873814d05225e4ecc1623a5e01c6619ca6b35be6efc3b242c920

// TSZ_INLINE_TEST_BEGIN 52b5572ecb86490ea3dbe8d15cfac0a9982b8357d2a26e45e3a9fe424557ef06 1672 instance_private_method_call_captures_side_effecting_receiver_once
    #[test]
    fn instance_private_method_call_captures_side_effecting_receiver_once() {
        // The receiver is referenced twice (read + `.call` this), so a
        // side-effecting receiver must be captured into a single hoisted temp.
        let output = emit_es5(
            "class Node {\n    #weight() { return 1; }\n    total(make: () => Node) { return make().#weight(); }\n}\n",
        );
        assert!(
            output.contains("(_a = make())")
                && output.contains(
                    "__classPrivateFieldGet((_a = make()), _Node_instances, \"m\", _Node_weight).call(_a)"
                ),
            "A side-effecting receiver must be captured once and reused by the `.call`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 52b5572ecb86490ea3dbe8d15cfac0a9982b8357d2a26e45e3a9fe424557ef06

// TSZ_INLINE_TEST_BEGIN 93697cef1c5528d586cb61c2c16edf247a1cec8b93f325c809b73fe2bbd74823 1688 instance_private_accessor_read_uses_instances_brand_and_getter_ref
    #[test]
    fn instance_private_accessor_read_uses_instances_brand_and_getter_ref() {
        let output = emit_es5(
            "class Cell {\n    get #value() { return 7; }\n    peek() { return this.#value; }\n}\n",
        );
        assert!(
            output
                .contains("__classPrivateFieldGet(this, _Cell_instances, \"a\", _Cell_value_get)"),
            "An instance accessor read must brand against `_instances` and pass the getter ref.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 93697cef1c5528d586cb61c2c16edf247a1cec8b93f325c809b73fe2bbd74823

// TSZ_INLINE_TEST_BEGIN 00d02dec16b1f684090eee147abc765bcad3947d8db3d3e2f5d89ae3ad6943bc 1700 instance_private_accessor_write_uses_instances_brand_and_setter_ref
    #[test]
    fn instance_private_accessor_write_uses_instances_brand_and_setter_ref() {
        let output = emit_es5(
            "class Slot {\n    set #value(v: number) {}\n    fill() { this.#value = 9; }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Slot_instances, 9, \"a\", _Slot_value_set)"
            ),
            "An instance accessor write must brand against `_instances` and pass the setter ref.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 00d02dec16b1f684090eee147abc765bcad3947d8db3d3e2f5d89ae3ad6943bc

// TSZ_INLINE_TEST_BEGIN bc03417c37e4a22ea92d41900c1c4fc9477211a8e6cd70679e6049567e2e13c6 1713 private_field_function_call_routes_through_call_to_preserve_this
    #[test]
    fn private_field_function_call_routes_through_call_to_preserve_this() {
        // A private field holding a function is still read with kind "f", but a
        // call must use `.call(this)` like tsc (preserving the receiver).
        let output =
            emit_es5("class Box {\n    #run = () => 1;\n    go() { return this.#run(); }\n}\n");
        assert!(
            output.contains("__classPrivateFieldGet(this, _Box_run, \"f\").call(this)"),
            "A private field-function call must route through `.call(this)`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END bc03417c37e4a22ea92d41900c1c4fc9477211a8e6cd70679e6049567e2e13c6

// TSZ_INLINE_TEST_BEGIN 76586ce7b2bf27a0873ee4cc21e93c9de7d80e270efafec90230c65200dba01a 1731 static_property_initializer_this_optional_property_keeps_guard
    #[test]
    fn static_property_initializer_this_optional_property_keeps_guard() {
        // `this` inside a static initializer is substituted with the class
        // alias; the optional-property guard must survive that substitution.
        let output = emit_es5("class Widget {\n    static handle = this?.id;\n}\n");
        assert!(
            output.contains("=== null ||") && output.contains("=== void 0 ? void 0 :"),
            "Static `this?.id` must keep the optional-chain guard.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("Widget.handle = _a.id;"),
            "Optional access must not be dropped to a plain property access.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 76586ce7b2bf27a0873ee4cc21e93c9de7d80e270efafec90230c65200dba01a

// TSZ_INLINE_TEST_BEGIN 635daeedc485b64d85a91b400a2dfc1fabc459f09128c9f013c0bfb9bb725197 1746 accessor_return_preserves_jsdoc_type_cast_comment
    #[test]
    fn accessor_return_preserves_jsdoc_type_cast_comment() {
        let output =
            emit_es5("class Casts {\n    get value() { return /** @type {*} */(null); }\n}\n");
        assert!(
            output.contains("return /** @type {*} */ (null);"),
            "ES5 class IR must preserve erased JSDoc type-cast comments.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 635daeedc485b64d85a91b400a2dfc1fabc459f09128c9f013c0bfb9bb725197

// TSZ_INLINE_TEST_BEGIN e93c1b9dbe46f38e2ad89b1c8d61ea7d6a8583efeee70eaefeab39351d3d9fb4 1756 static_property_initializer_this_optional_method_call_keeps_guard
    #[test]
    fn static_property_initializer_this_optional_method_call_keeps_guard() {
        // Different class/member names; optional method call `this?.compute()`.
        let output = emit_es5("class Engine {\n    static result = this?.compute();\n}\n");
        assert!(
            output.contains("=== null ||")
                && output.contains("=== void 0 ? void 0 :")
                && output.contains(".compute()"),
            "Static `this?.compute()` must guard the call.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END e93c1b9dbe46f38e2ad89b1c8d61ea7d6a8583efeee70eaefeab39351d3d9fb4

// TSZ_INLINE_TEST_BEGIN efdfbe84678a7327d24255473c674359532bad025466fefb1921a5d2bbc9d8b6 1768 static_property_initializer_this_optional_element_call_keeps_guard
    #[test]
    fn static_property_initializer_this_optional_element_call_keeps_guard() {
        // Element-access optional method call inside a static initializer.
        let output = emit_es5("class Store {\n    static v = this?.[\"load\"]();\n}\n");
        assert!(
            output.contains("=== null ||")
                && output.contains("=== void 0 ? void 0 :")
                && output.contains("[\"load\"]()"),
            "Static `this?.[\"load\"]()` must guard the element call.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END efdfbe84678a7327d24255473c674359532bad025466fefb1921a5d2bbc9d8b6

// TSZ_INLINE_TEST_BEGIN d154035a7de04eb7f588ccd6536a2bf44ed4583f9a52a46b8620e1dbb0a18307 1780 static_method_body_this_optional_access_keeps_guard
    #[test]
    fn static_method_body_this_optional_access_keeps_guard() {
        // Static *method* body (not just initializer), different name again.
        let output =
            emit_es5("class Service {\n    static run() {\n        return this?.go();\n    }\n}\n");
        assert!(
            output.contains("=== null ||") && output.contains("=== void 0 ? void 0 :"),
            "Static method `this?.go()` must keep the guard.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END d154035a7de04eb7f588ccd6536a2bf44ed4583f9a52a46b8620e1dbb0a18307

// TSZ_INLINE_TEST_BEGIN 0f361d8469088b6bda2963ab9b64bd807afada67f4681139de3f00a4ae1d5948 1791 instance_method_body_this_optional_access_keeps_guard
    #[test]
    fn instance_method_body_this_optional_access_keeps_guard() {
        // Instance method body — proves the fix is not static-specific.
        let output = emit_es5("class Cache {\n    m() {\n        return this?.entry;\n    }\n}\n");
        assert!(
            output.contains("this === null || this === void 0 ? void 0 : this.entry"),
            "Instance `this?.entry` must keep the guard.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 0f361d8469088b6bda2963ab9b64bd807afada67f4681139de3f00a4ae1d5948

// TSZ_INLINE_TEST_BEGIN 595f335ec610ecf3002822c2a6d83456a3888fa86b4d5d1b5ff5cec4a65991de 1801 class_member_identifier_receiver_optional_access_keeps_guard
    #[test]
    fn class_member_identifier_receiver_optional_access_keeps_guard() {
        // Receiver is a plain identifier, not `this` — proves the rule keys on
        // the `?.` token, not on the `this` keyword.
        let output =
            emit_es5("declare const dep: any;\nclass Host {\n    static value = dep?.field;\n}\n");
        assert!(
            output.contains("dep === null || dep === void 0 ? void 0 : dep.field"),
            "Identifier-receiver `dep?.field` must keep the guard.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 595f335ec610ecf3002822c2a6d83456a3888fa86b4d5d1b5ff5cec4a65991de

// TSZ_INLINE_TEST_BEGIN 7f887232b60e2ac14fc0305cfa1cde13cd6aacbb88b9a19499914c08971c15dc 1813 class_member_non_optional_access_is_unchanged
    #[test]
    fn class_member_non_optional_access_is_unchanged() {
        // Negative case: a non-optional access must NOT gain a guard.
        let output = emit_es5("class Plain {\n    static value = this.field;\n}\n");
        assert!(
            !output.contains("=== void 0 ? void 0 :"),
            "Non-optional `this.field` must not be lowered to a guard.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 7f887232b60e2ac14fc0305cfa1cde13cd6aacbb88b9a19499914c08971c15dc

// TSZ_INLINE_TEST_BEGIN 760f16db9114f3e478af4dc2be8f28b8fd2162555c62c351d64a13867b622265 1831 private_field_compound_add_lowers_to_get_op_set
    #[test]
    fn private_field_compound_add_lowers_to_get_op_set() {
        let output = emit_es5(
            "class Acc {\n    #count = 0;\n    bump() {\n        this.#count += 2;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Acc_count, __classPrivateFieldGet(this, _Acc_count, \"f\") + 2, \"f\")"
            ),
            "Private `#count += 2` must lower to get-op-set.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("\"f\") += "),
            "Must not emit an un-assignable `get(...) += v`.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 760f16db9114f3e478af4dc2be8f28b8fd2162555c62c351d64a13867b622265

// TSZ_INLINE_TEST_BEGIN 4dfeeaf7c65aa109aa9279a25fed839817b3186500fe070858bf9f919e3ca7af 1848 private_field_compound_bitor_uses_base_operator
    #[test]
    fn private_field_compound_bitor_uses_base_operator() {
        // Different class/member/operator: `|=` lowers with base `|`.
        let output = emit_es5(
            "class Flags {\n    #mask = 0;\n    set(b: number) {\n        this.#mask |= b;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Flags_mask, __classPrivateFieldGet(this, _Flags_mask, \"f\") | b, \"f\")"
            ),
            "Private `#mask |= b` must lower to get-`|`-set.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4dfeeaf7c65aa109aa9279a25fed839817b3186500fe070858bf9f919e3ca7af

// TSZ_INLINE_TEST_BEGIN 4f133a82f6494b2f4e2c89bcd378d33637bed3fc5b16f3e61c3b6c9d9461c215 1862 private_field_prefix_increment_uses_single_form
    #[test]
    fn private_field_prefix_increment_uses_single_form() {
        let output =
            emit_es5("class Pre {\n    #n = 0;\n    up() {\n        ++this.#n;\n    }\n}\n");
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Pre_n, (_a = __classPrivateFieldGet(this, _Pre_n, \"f\"), ++_a), \"f\")"
            ),
            "Prefix `++this.#n` must use the new-value comma form.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _a;"),
            "Prefix mutation must hoist its temp.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4f133a82f6494b2f4e2c89bcd378d33637bed3fc5b16f3e61c3b6c9d9461c215

// TSZ_INLINE_TEST_BEGIN 73904d609f6d060bcc608534c8bc90a8273afdeed01a11dcee5c99b05a522607 1878 private_field_postfix_decrement_statement_uses_lean_form
    #[test]
    fn private_field_postfix_decrement_statement_uses_lean_form() {
        // Statement position discards the result → no old-value temp.
        let output =
            emit_es5("class Pst {\n    #v = 5;\n    step() {\n        this.#v--;\n    }\n}\n");
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Pst_v, (_a = __classPrivateFieldGet(this, _Pst_v, \"f\"), _a--, _a), \"f\")"
            ),
            "Statement `this.#v--` must use the lean single-temp form.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("var _a, _b"),
            "Statement postfix must not allocate an old-value temp.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 73904d609f6d060bcc608534c8bc90a8273afdeed01a11dcee5c99b05a522607

// TSZ_INLINE_TEST_BEGIN 9c2c5511d0421e799ae27e9bfb6aade00e973cfcf4b9b51346983987138f71d3 1895 private_field_postfix_increment_value_keeps_old_value
    #[test]
    fn private_field_postfix_increment_value_keeps_old_value() {
        // Value position (`return ...`) must yield the pre-mutation value.
        let output = emit_es5(
            "class Val {\n    #w = 0;\n    take() {\n        return this.#w++;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "return (__classPrivateFieldSet(this, _Val_w, (_b = __classPrivateFieldGet(this, _Val_w, \"f\"), _a = _b++, _b), \"f\"), _a)"
            ),
            "Value `return this.#w++` must return the old value via the two-temp form.\nOutput:\n{output}"
        );
        assert!(
            output.contains("var _a, _b;"),
            "Value postfix must hoist both temps in tsc order (`_a`, `_b`).\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 9c2c5511d0421e799ae27e9bfb6aade00e973cfcf4b9b51346983987138f71d3

// TSZ_INLINE_TEST_BEGIN 16d5baeb6c3f5537643ffe687478902bc97f5cfc08f04cdbfebbcf60cbf996f3 1913 private_accessor_compound_uses_instances_brand_and_get_set_refs
    #[test]
    fn private_accessor_compound_uses_instances_brand_and_get_set_refs() {
        // An instance accessor brands against `_Box_instances` and threads the
        // getter as the trailing read argument and the setter as the trailing
        // write argument (tsc's 4-arg get / 5-arg set forms).
        let output = emit_es5(
            "class Box {\n    get #val() { return 1; }\n    set #val(v: number) {}\n    add() {\n        this.#val += 3;\n    }\n}\n",
        );
        assert!(
            output.contains(
                "__classPrivateFieldSet(this, _Box_instances, __classPrivateFieldGet(this, _Box_instances, \"a\", _Box_val_get) + 3, \"a\", _Box_val_set)"
            ),
            "Accessor `#val += 3` must brand against `_Box_instances` and pass the getter/setter refs.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 16d5baeb6c3f5537643ffe687478902bc97f5cfc08f04cdbfebbcf60cbf996f3

// TSZ_INLINE_TEST_BEGIN 71fae7864e0ad729cbd66167ec9c9dcf71f41438258f251cc9814626bb48895c 1929 private_field_plain_assignment_still_lowers
    #[test]
    fn private_field_plain_assignment_still_lowers() {
        // Regression guard: the plain `=` write path (from #12180) is unchanged.
        let output =
            emit_es5("class Plain {\n    #p = 0;\n    reset() {\n        this.#p = 9;\n    }\n}\n");
        assert!(
            output.contains("__classPrivateFieldSet(this, _Plain_p, 9, \"f\")"),
            "Plain `this.#p = 9` must still lower to a single set.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 71fae7864e0ad729cbd66167ec9c9dcf71f41438258f251cc9814626bb48895c
