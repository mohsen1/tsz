#[test]
fn test_class_to_iife() {
    let output = emit_es5_with_comments(
        "class Point {\n    constructor(x, y) {\n        this.x = x;\n        this.y = y;\n    }\n}\n",
    );
    assert!(
        output.contains("/** @class */"),
        "Expected @class annotation.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function Point("),
        "Expected constructor function.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return Point;"),
        "Expected return statement.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_extends_to_iife() {
    let output = emit_es5("class Dog extends Animal {\n    bark() { return 'woof'; }\n}\n");
    assert!(
        output.contains("__extends"),
        "Expected __extends helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_super"),
        "Expected _super parameter.\nOutput:\n{output}"
    );
}

#[test]
fn es5_invalid_super_property_access_uses_recovery_base() {
    let output = emit_es5(
        r#"
class NoBase {
    constructor() {
        var a = super.prototype;
        var b = super.hasOwnProperty("");
    }

    fn() {
        var a = super.prototype;
        var b = super.hasOwnProperty("");
    }

    m = super.prototype;
    n = super.hasOwnProperty("");

    static static1() {
        super.hasOwnProperty("");
    }
}

var obj = { n: super.wat, p: super.foo() };
"#,
    );

    assert!(
        output.contains("this.m = _super.prototype.prototype;"),
        "Instance field super property access in an invalid no-base class should lower through _super.prototype.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this.n = _super.prototype.hasOwnProperty.call(this, \"\");"),
        "Instance field super calls in an invalid no-base class should bind this through _super.prototype.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var a = _super.prototype.prototype;")
            && output.contains("var b = _super.prototype.hasOwnProperty.call(this, \"\");"),
        "Constructor and instance method super access should use the instance home-object base.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "NoBase.static1 = function () {\n        _super.hasOwnProperty.call(this, \"\");"
        ),
        "Static method super calls should lower through the static _super base.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var obj = { n: _super.wat, p: _super.foo.call(this) };"),
        "Top-level invalid super in an object literal should use tsc's recovery _super base.\nOutput:\n{output}"
    );
}

#[test]
fn es5_nested_non_arrow_functions_use_super_recovery_base() {
    let output = emit_es5(
        r#"
class Base {
    publicFunc() { }
}
class Derived extends Base {
    fn() {
        super.publicFunc();
        function inner() {
            super.publicFunc();
        }
        var x = {
            test: function () { return super.publicFunc(); }
        };
    }
}
"#,
    );

    assert!(
        output.contains("_super.prototype.publicFunc.call(this);"),
        "Immediate instance method super calls should use _super.prototype.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function inner() {\n            _super.publicFunc.call(this);"),
        "Nested function declarations should use tsc's invalid-super recovery base.\nOutput:\n{output}"
    );
    assert!(
        output.contains("test: function () { return _super.publicFunc.call(this); }"),
        "Nested function expressions should use tsc's invalid-super recovery base.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function inner() {\n            _super.prototype.publicFunc.call(this);")
            && !output.contains("return _super.prototype.publicFunc.call(this); }"),
        "Nested non-arrow functions must not inherit the enclosing method's instance super base.\nOutput:\n{output}"
    );
}

#[test]
fn es5_class_super_assignment_function_comment_stays_after_assignment() {
    let output = emit_es5(
        r#"
class Base {
    m1(a) { return ""; }
}
class Derived extends Base {
    fn() {
        super.m1 = function (a) { return ""; }; // kept
        super.value = 0;
    }
}
"#,
    );

    assert!(
        output.contains("_super.prototype.m1 = function (a) { return \"\"; };"),
        "Super function assignment should keep the nested function body compact.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return \"\"; // kept"),
        "Trailing comment after the assignment must not be attached to the nested return.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_static_method() {
    let output = emit_es5("class Counter {\n    static count() { return 0; }\n}\n");
    assert!(
        output.contains("Counter.count = function"),
        "Expected static method on class directly.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_prototype_method() {
    let output = emit_es5("class Greeter {\n    greet() { return 'hello'; }\n}\n");
    assert!(
        output.contains("Greeter.prototype.greet = function"),
        "Expected prototype method.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_private_field_weakmap() {
    let output = emit_es5("class Container {\n    #value = 42;\n}\n");
    assert!(
        output.contains("WeakMap"),
        "Expected WeakMap for private field.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_property_initializer() {
    let output = emit_es5("class Counter {\n    count = 0;\n}\n");
    assert!(
        output.contains("this.count ="),
        "Expected property initializer in constructor.\nOutput:\n{output}"
    );
}

#[test]
fn test_computed_string_field_preserves_source_quotes() {
    let output = emit_es5("class C {\n    ['this'] = '';\n}\n");
    assert!(
        output.contains("this['this'] = '';"),
        "Expected computed string field to preserve source quotes.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Expected computed string field not to be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn test_class_getter_setter_define_property() {
    let output = emit_es5("class Foo {\n    get bar() { return 1; }\n    set bar(v) {}\n}\n");
    assert!(
        output.contains("Object.defineProperty"),
        "Expected Object.defineProperty for accessors.\nOutput:\n{output}"
    );
}

#[test]
fn test_arrow_function_to_function() {
    let output = emit_es5("const f = (x) => x * 2;\n");
    assert!(
        !output.contains("=>"),
        "ES5 should not contain arrow function syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function"),
        "Expected function keyword.\nOutput:\n{output}"
    );
}

#[test]
fn test_arrow_function_this_capture() {
    let output = emit_es5("class Foo {\n    bar() {\n        const f = () => this;\n    }\n}\n");
    assert!(
        output.contains("_this"),
        "Expected _this capture for arrow function using this.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_arrow_in_function_passes_lexical_this_to_awaiter() {
    let output = emit_es5(
        "function f() {\n    const promise = (async () => {\n        await null;\n    })();\n}\n",
    );

    assert!(
        output.contains("var _this = this;"),
        "Async arrow inside a function should capture lexical this in ES5.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__awaiter(_this, void 0, void 0"),
        "Async arrow inside a function should pass lexical this to __awaiter.\nOutput:\n{output}"
    );
}

#[test]
fn test_top_level_async_arrow_still_passes_void_0_to_awaiter() {
    let output = emit_es5("const f = async () => {\n    await null;\n};\n");

    assert!(
        output.contains("__awaiter(void 0, void 0, void 0"),
        "Top-level async arrow should not synthesize a lexical this capture.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _this = this;"),
        "Top-level async arrow should not emit a file-level _this capture.\nOutput:\n{output}"
    );
}

#[test]
fn test_let_becomes_var() {
    let output = emit_es5("let x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected let to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let x"),
        "let should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn test_const_becomes_var() {
    let output = emit_es5("const x = 1;\n");
    assert!(
        output.contains("var x"),
        "Expected const to become var.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const x"),
        "const should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_function_awaiter() {
    let output = emit_es5("async function fetchData() {\n    await fetch('/api');\n}\n");
    assert!(
        output.contains("__awaiter"),
        "Expected __awaiter helper for async function.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("async "),
        "async keyword should not appear in ES5.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_directive_prologue_stays_in_awaiter_wrapper() {
    let output = emit_es5(
        "declare var a: boolean;\n\
         declare var p: Promise<boolean>;\n\
         async function func(): Promise<void> {\n\
             \"use strict\";\n\
             var b = await p || a;\n\
         }\n",
    );

    let wrapper_directive = output
        .find("function () {\n        \"use strict\";\n        var b;")
        .unwrap_or_else(|| {
            panic!(
                "Async directive prologue should be emitted before hoisted vars in the __awaiter wrapper.\nOutput:\n{output}"
            )
        });
    let generator_return = output
        .find("return __generator(this, function (_a) {")
        .unwrap_or_else(|| panic!("Expected __generator body.\nOutput:\n{output}"));

    assert!(
        wrapper_directive < generator_return,
        "Async directive prologue should precede the generator body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("case 0:\n                        \"use strict\";"),
        "Async directive prologue should not remain inside the generator switch case.\nOutput:\n{output}"
    );
}

#[test]
fn async_arrow_hoisted_locals_share_var_statement() {
    let output = emit_es5(
        "(async () => {\n\
             const response = await fetch('/api');\n\
             const blob = await response.blob();\n\
             const size = 300;\n\
             const image = new Image();\n\
         })();\n",
    );

    assert!(
        output.contains("var response, blob, size, image;"),
        "Async arrow hoisted locals should share one var statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var response;\n        var blob;"),
        "Async arrow hoisted locals should not split ordinary declarations.\nOutput:\n{output}"
    );
}
