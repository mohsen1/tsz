//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/es5/helpers_object_literal.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 671f45296c5d41579fd073e49e8ce424b97cfe45d5f9dba50dde776de2b9347c 1063 computed_object_member_line_comment_uses_multiline_comma_layout
    #[test]
    fn computed_object_member_line_comment_uses_multiline_comma_layout() {
        let source = "class Base {\n\
    bar() { return 0; }\n\
}\n\
class C extends Base {\n\
    foo() {\n\
        () => {\n\
            var obj = {\n\
                [super.bar()]() { } // needs capture\n\
            };\n\
        }\n\
    }\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains(
                "var obj = (_a = {},\n                _a[_super.prototype.bar.call(_this)] = function () { } // needs capture\n            ,\n                _a);"
            ),
            "Computed-object comma lowering should place following comma items after the trailing line comment.\nOutput:\n{output}"
        );
        assert!(
            !output.contains(
                "_a = {}, _a[_super.prototype.bar.call(_this)] = function () { } // needs capture"
            ),
            "Trailing line comments must not be emitted in compact comma layout.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 671f45296c5d41579fd073e49e8ce424b97cfe45d5f9dba50dde776de2b9347c

// TSZ_INLINE_TEST_BEGIN a2b50e1b51ade3aac7337d95ede81cffde44ac5282b85d021032483cb8ccba2c 1094 computed_object_member_without_line_comment_stays_compact
    #[test]
    fn computed_object_member_without_line_comment_stays_compact() {
        let source = "class Base {\n\
    bar() { return 0; }\n\
}\n\
class C extends Base {\n\
    foo() {\n\
        () => {\n\
            var obj = {\n\
                [super.bar()]() { }\n\
            };\n\
        }\n\
    }\n\
}\n";

        let output = emit_es5(source);

        assert!(
            output.contains(
                "var obj = (_a = {}, _a[_super.prototype.bar.call(_this)] = function () { }, _a);"
            ),
            "Computed-object members without trailing line comments should keep the compact comma layout.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END a2b50e1b51ade3aac7337d95ede81cffde44ac5282b85d021032483cb8ccba2c
