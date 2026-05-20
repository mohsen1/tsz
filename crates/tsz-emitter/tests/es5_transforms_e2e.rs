//! End-to-end ES5 transform tests using the full `lower_and_print` pipeline.
//!
//! These tests verify that the complete chain (parse -> lower -> print) produces
//! correct ES5 output for destructuring, class, and async transforms.

use crate::context::emit::EmitContext;
use crate::emitter::ModuleKind;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use crate::output::printer::{PrintOptions, lower_and_print};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es5(source: &str) -> String {
    emit_with_target(source, ScriptTarget::ES5)
}

fn emit_es5_with_module(source: &str, module: ModuleKind) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target: ScriptTarget::ES5,
        module,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn accessor_return_type_asserted_object_spread_uses_assign() {
    let output = emit_es5_with_module(
        "declare const props: WizardStepProps;\n\
         export class Wizard {\n\
             get steps() {\n\
                 return { wizard: this, ...props } as WizardStepProps;\n\
             }\n\
         }\n\
         export interface WizardStepProps { wizard?: Wizard; }\n",
        ModuleKind::CommonJS,
    );

    assert!(
        output.contains("return __assign({ wizard: this }, props);"),
        "ES5 class accessor object spread should delegate to object-spread lowering.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return (_a = { wizard: this },"),
        "Object spread must not be treated as computed-property comma lowering.\nOutput:\n{output}"
    );
}

fn emit_es5_with_comments(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = EmitterPrinter::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn emit_es5_downlevel_iteration(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        downlevel_iteration: true,
        remove_comments: true,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn new_target_es5_captures_ordinary_function_and_lexical_arrow() {
    let output = emit_es5(
        "function F() { return new.target; }\n\
         function Outer() { var f = () => new.target; return f; }\n",
    );

    assert!(
        output.contains(
            "function F() {\n    var _newTarget = this && this instanceof F ? this.constructor : void 0;\n    return _newTarget;\n}"
        ),
        "Ordinary functions should capture `new.target` with the tsc ES5 initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "function Outer() {\n    var _newTarget = this && this instanceof Outer ? this.constructor : void 0;\n    var f = function () { return _newTarget; };\n    return f;\n}"
        ),
        "Arrows should reuse the containing function's `new.target` capture.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_names_anonymous_function_expressions_for_capture() {
    let output = emit_es5(
        "var A = function() { return new.target; };\n\
         var obj = { p: function() { return new.target; } };\n",
    );

    assert!(
        output.contains("var A = function A()"),
        "Anonymous variable-assigned functions should receive the inferred name used by the capture initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this && this instanceof A ? this.constructor : void 0"),
        "Variable-assigned capture should test the inferred function name.\nOutput:\n{output}"
    );
    assert!(
        output.contains("p: function p()"),
        "Object property functions should receive the property name used by the capture initializer.\nOutput:\n{output}"
    );
    assert!(
        output.contains("this && this instanceof p ? this.constructor : void 0"),
        "Property-assigned capture should test the inferred property function name.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_class_constructor_and_invalid_method_use_tsc_recovery() {
    let output = emit_es5(
        "class C {\n\
             constructor(x = new.target) { this.x = new.target; }\n\
             m(x = new.target) { return new.target; }\n\
         }\n",
    );

    assert!(
        output.contains(
            "function C(x) {\n        if (x === void 0) { x = _newTarget; }\n        var _newTarget = this.constructor;"
        ),
        "Class constructors should use `this.constructor`, with parameter defaults before the constructor capture.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "C.prototype.m = function (x) {\n        var _newTarget = void 0;\n        if (x === void 0) { x = _newTarget; }\n        return _newTarget;\n    };"
        ),
        "Invalid method `new.target` should recover as `void 0`, before method parameter defaults.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "Class ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_object_literal_methods_and_accessors_get_invalid_capture() {
    let output = emit_es5(
        "const O = {\n\
             [new.target]: undefined,\n\
             k() { return new.target; },\n\
             get l() { return new.target; },\n\
             set m(_) { _ = new.target; },\n\
             n: new.target,\n\
         };\n",
    );

    assert!(
        output.contains(
            "_a.k = function () {\n        var _newTarget = void 0;\n        return _newTarget;\n    }"
        ),
        "Object-literal methods own invalid `new.target` and should not close over the outer computed-name binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "get: function () {\n        var _newTarget = void 0;\n        return _newTarget;\n    }"
        ),
        "Object-literal getters own invalid `new.target` through the accessor descriptor function.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "set: function (_) {\n        var _newTarget = void 0;\n        _ = _newTarget;\n    }"
        ),
        "Object-literal setters own invalid `new.target` through the accessor descriptor function.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_derived_constructor_body_capture_precedes_super_capture() {
    let output = emit_es5(
        "class A {}\n\
         class B extends A {\n\
             constructor() {\n\
                 super();\n\
                 const e = new.target;\n\
                 const f = () => new.target;\n\
             }\n\
         }\n",
    );

    let new_target_capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!("Derived constructor body should capture `new.target`.\nOutput:\n{output}")
        });
    let super_capture = output.find("var _this = _super.call(this) || this;").unwrap_or_else(|| {
        panic!("Derived constructor should still capture the super return value.\nOutput:\n{output}")
    });

    assert!(
        new_target_capture < super_capture,
        "Constructor-body `new.target` capture should precede the super return capture, matching tsc.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_derived_constructor_keeps_body_and_field_captures() {
    let output = emit_es5(
        "class B {}\n\
         class D extends B {\n\
             x = new.target;\n\
             constructor(value = new.target) {\n\
                 super();\n\
                 const body = new.target;\n\
             }\n\
         }\n",
    );

    let captures: Vec<_> = output
        .match_indices("var _newTarget = this.constructor;")
        .map(|(idx, _)| idx)
        .collect();
    assert_eq!(
        captures.len(),
        2,
        "Derived constructor should emit separate `new.target` captures for constructor scope and moved field initializers.\nOutput:\n{output}"
    );

    let super_capture = output.find("var _this = _super.call(this) || this;").unwrap_or_else(|| {
        panic!("Derived constructor should still capture the super return value.\nOutput:\n{output}")
    });
    let field_initializer = output.find("_this.x = _newTarget;").unwrap_or_else(|| {
        panic!("Moved field initializer should read the post-super capture.\nOutput:\n{output}")
    });
    let body_read = output.find("var body = _newTarget;").unwrap_or_else(|| {
        panic!("Constructor body should still read a `new.target` capture.\nOutput:\n{output}")
    });

    assert!(
        captures[0] < super_capture
            && super_capture < captures[1]
            && captures[1] < field_initializer
            && field_initializer < body_read,
        "Constructor-scope capture should precede super(), and moved field initializer capture should follow super() before field/body reads.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_class_field_initializers_capture_default_constructor() {
    let output = emit_es5(
        "class C {\n\
             x = new.target;\n\
             y = () => new.target;\n\
         }\n",
    );

    let capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!("Default constructor should capture `new.target` for field initializers.\nOutput:\n{output}")
        });
    let direct_initializer = output.find("this.x = _newTarget;").unwrap_or_else(|| {
        panic!("Direct field initializer should read the capture.\nOutput:\n{output}")
    });
    let arrow_initializer = output
        .find("this.y = function () { return _newTarget; };")
        .unwrap_or_else(|| {
            panic!("Arrow field initializer should close over the capture.\nOutput:\n{output}")
        });

    assert!(
        capture < direct_initializer && capture < arrow_initializer,
        "Class constructor capture should precede moved field initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_derived_field_initializers_capture_after_super() {
    let output = emit_es5(
        "class B {}\n\
         class D extends B {\n\
             x = new.target;\n\
             y = () => new.target;\n\
         }\n",
    );

    let super_capture = output.find("var _this =").unwrap_or_else(|| {
        panic!("Derived constructor should materialize the super return value.\nOutput:\n{output}")
    });
    let capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!(
                "Derived constructor should capture `new.target` after super().\nOutput:\n{output}"
            )
        });
    let direct_initializer = output.find("_this.x = _newTarget;").unwrap_or_else(|| {
        panic!("Derived field initializer should read the capture.\nOutput:\n{output}")
    });
    let arrow_initializer = output
        .find("_this.y = function () { return _newTarget; };")
        .unwrap_or_else(|| {
            panic!(
                "Derived arrow field initializer should close over the capture.\nOutput:\n{output}"
            )
        });

    assert!(
        super_capture < capture && capture < direct_initializer && capture < arrow_initializer,
        "Derived class capture should follow super() and precede moved field initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_explicit_constructor_checks_field_initializers() {
    let output = emit_es5(
        "class C {\n\
             x = new.target;\n\
             constructor() {}\n\
         }\n\
         class D extends C {\n\
             y = new.target;\n\
             constructor() { super(); }\n\
         }\n",
    );

    assert_eq!(
        output.matches("var _newTarget = this.constructor;").count(),
        2,
        "Both explicit constructors should capture `new.target` used only by field initializers.\nOutput:\n{output}"
    );

    let derived_super = output.rfind("var _this =").unwrap_or_else(|| {
        panic!("Derived explicit constructor should materialize the super return value.\nOutput:\n{output}")
    });
    let derived_capture = output
        .rfind("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!("Derived explicit constructor should capture `new.target`.\nOutput:\n{output}")
        });
    let derived_initializer = output.find("_this.y = _newTarget;").unwrap_or_else(|| {
        panic!("Derived explicit field initializer should read the capture.\nOutput:\n{output}")
    });

    assert!(
        derived_super < derived_capture && derived_capture < derived_initializer,
        "Derived explicit constructor capture should follow super() and precede field initializers.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_private_field_initializers_capture_default_constructor() {
    let output = emit_es5(
        "class C {\n\
             #x = new.target;\n\
             #y = () => new.target;\n\
             get x() { return this.#x; }\n\
         }\n",
    );

    let capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!(
                "Default constructor should capture `new.target` for private field initializers.\nOutput:\n{output}"
            )
        });
    let direct_initializer = output
        .find("__classPrivateFieldSet(this, _C_x, _newTarget")
        .unwrap_or_else(|| {
            panic!("Private field initializer should read the capture.\nOutput:\n{output}")
        });
    let arrow_initializer = output
        .find("__classPrivateFieldSet(this, _C_y, function () { return _newTarget; }")
        .unwrap_or_else(|| {
            panic!(
                "Private arrow field initializer should close over the capture.\nOutput:\n{output}"
            )
        });

    assert!(
        capture < direct_initializer && capture < arrow_initializer,
        "Class constructor capture should precede moved private field initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_derived_private_field_initializers_capture_after_super() {
    let output = emit_es5(
        "class B {}\n\
         class D extends B {\n\
             #x = new.target;\n\
             #y = () => new.target;\n\
             get x() { return this.#x; }\n\
         }\n",
    );

    let super_capture = output.find("var _this =").unwrap_or_else(|| {
        panic!("Derived constructor should materialize the super return value.\nOutput:\n{output}")
    });
    let capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!(
                "Derived constructor should capture `new.target` for private fields.\nOutput:\n{output}"
            )
        });
    let direct_initializer = output
        .find("__classPrivateFieldSet(_this, _D_x, _newTarget")
        .unwrap_or_else(|| {
            panic!("Derived private field initializer should read the capture.\nOutput:\n{output}")
        });
    let arrow_initializer = output
        .find("__classPrivateFieldSet(_this, _D_y, function () { return _newTarget; }")
        .unwrap_or_else(|| {
            panic!(
                "Derived private arrow field initializer should close over the capture.\nOutput:\n{output}"
            )
        });

    assert!(
        super_capture < capture && capture < direct_initializer && capture < arrow_initializer,
        "Derived class capture should follow super() and precede moved private field initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_auto_accessor_initializers_capture_default_constructor() {
    let output = emit_es5(
        "class C {\n\
             accessor x = new.target;\n\
             accessor y = () => new.target;\n\
         }\n",
    );

    let capture = output
        .find("var _newTarget = this.constructor;")
        .unwrap_or_else(|| {
            panic!(
                "Default constructor should capture `new.target` for auto-accessor initializers.\nOutput:\n{output}"
            )
        });
    let direct_initializer = output
        .find("_C_x_accessor_storage.set(this, _newTarget);")
        .unwrap_or_else(|| {
            panic!("Auto-accessor initializer should read the capture.\nOutput:\n{output}")
        });
    let arrow_initializer = output
        .find("_C_y_accessor_storage.set(this, function () { return _newTarget; });")
        .unwrap_or_else(|| {
            panic!(
                "Auto-accessor arrow initializer should close over the capture.\nOutput:\n{output}"
            )
        });

    assert!(
        capture < direct_initializer && capture < arrow_initializer,
        "Class constructor capture should precede moved auto-accessor initializers.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("new.target"),
        "ES5 output must not retain raw `new.target` syntax.\nOutput:\n{output}"
    );
}

#[test]
fn new_target_es5_top_level_arrow_reads_recovered_binding_without_capture() {
    let output = emit_es5("var B = () => new.target;\n");

    assert!(
        output.contains("var B = function () { return _newTarget; };"),
        "Top-level invalid arrow `new.target` should read the recovered `_newTarget` binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var _newTarget"),
        "No function owns top-level arrow `new.target`, so no capture should be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_for_loop_captured_let_with_await_uses_loop_generator() {
    let output = emit_es5_with_comments(
        "async function f() {\n\
             var ar = [];\n\
             for (let i = 0; i < 1; i++) {\n\
                 await 1;\n\
                 ar.push(() => i);\n\
             }\n\
         }\n",
    );

    assert!(
        output.contains("var ar, _loop_1, i;"),
        "Captured loop helper and loop variable should be hoisted.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_loop_1 = function (i)"),
        "Captured loop body should move into a loop helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [5 /*yield**/, _loop_1(i)];"),
        "Outer async loop should delegate to the loop helper generator.\nOutput:\n{output}"
    );
    assert!(
        output.contains("ar.push(function () { return i; });"),
        "Captured arrow should close over the loop helper parameter.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_uses_ambient_value_for_custom_promise_constructor() {
    let output = emit_es5(
        "type MyPromise<T> = Promise<T>;\n\
         declare var MyPromise: typeof Promise;\n\
         async function f(): MyPromise<void> { }\n\
         var g = async (): MyPromise<void> => { };\n\
         class C { async m(): MyPromise<void> { } }\n",
    );

    assert!(
        output.matches("MyPromise, function").count() >= 3,
        "Async functions, arrows, and methods should pass the ambient value constructor.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_arrow_computed_object_value_arrow_captures_generator_this() {
    let output = emit_es5(
        "class A {\n\
             b = async (...args: any[]) => {\n\
                 await Promise.resolve();\n\
                 const obj = { [\"a\"]: () => this };\n\
             };\n\
         }\n",
    );

    assert!(
        output.contains("return __awaiter(_this,"),
        "Class field async arrow should pass the captured instance to __awaiter.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var obj;\n                var _a;\n                var _this = this;"),
        "Nested arrow after await should get a generator-callback lexical this capture.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a[\"a\"] = function () { return _this; }"),
        "Computed object value arrow should return the generator callback capture.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_nested_regular_function_arrow_owns_this_capture() {
    let output = emit_es5(
        "class A {\n\
             b = async () => {\n\
                 await Promise.resolve();\n\
                 const f = function () { return () => this; };\n\
             };\n\
         }\n",
    );

    assert!(
        !output.contains("var f;\n                var _this = this;"),
        "Async generator callback should not capture `this` for arrows inside a nested regular function.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "f = function () {\n                            var _this = this;\n                            return function () { return _this; };\n                        };"
        ),
        "Nested regular function should own the `_this` capture for its lowered arrow.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_user_this_alias_identifier_does_not_trigger_capture() {
    let output = emit_es5(
        "async function f() {\n\
             const _this = \"sentinel\";\n\
             await Promise.resolve();\n\
             return _this;\n\
         }\n",
    );

    assert!(
        !output.contains("var _this = this;"),
        "User identifier `_this` should not be mistaken for generated lexical-this capture.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _this;"),
        "The user `_this` local should still be hoisted as an ordinary async local.\nOutput:\n{output}"
    );
}

#[test]
fn class_method_for_of_delegates_es5_statement_lowering() {
    let output = emit_es5_with_comments(
        r#"
class Operation {
    validate(parameterValues: any) {
        let result: any = null;
        for (const parameterLocation of Object.keys(parameterValues)) {
            const parameter = (this as any).getParameter();
            // keep loop comment
            result = parameterLocation;
        }
        return result;
    }
}
"#,
    );

    assert!(
        output
            .contains("for (var _i = 0, _a = Object.keys(parameterValues); _i < _a.length; _i++)"),
        "Class method for-of should use the normal ES5 array-index lowering.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var parameterLocation = _a[_i];"),
        "Loop variable should be bound from the generated array temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var parameter = this.getParameter();"),
        "Type-erased `(this as any)` should print through the normal AST expression path.\nOutput:\n{output}"
    );
    assert!(
        !output.contains(" of Object.keys(parameterValues)")
            && !output.contains("(this).getParameter()"),
        "Class method for-of must not leak raw for-of syntax or redundant type-erasure parens.\nOutput:\n{output}"
    );
}

#[test]
fn class_accessors_capture_arrow_this_with_collision_safe_alias() {
    let output = emit_es5(
        "class C {\n\
             get value() {\n\
                 var x = { run: cb => () => { var _this = 1; return cb(this); } };\n\
                 return 1;\n\
             }\n\
             set value(next) {\n\
                 var _this = 1;\n\
                 var x = { run: cb => () => cb(this) };\n\
             }\n\
         }\n",
    );

    assert!(
        output.matches("var _this_1 = this;").count() >= 2,
        "Accessor arrows should reserve a collision-safe lexical this alias.\nOutput:\n{output}"
    );
    assert!(
        output.matches("return cb(_this_1);").count() >= 2,
        "Nested arrows should reference the reserved accessor this alias.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return cb(_this);"),
        "User `_this` bindings must not shadow generated accessor this captures.\nOutput:\n{output}"
    );
}

#[test]
fn class_accessors_capture_arrow_this_with_default_alias_when_available() {
    let output = emit_es5(
        "class C {\n\
             get value() {\n\
                 var x = { run: cb => () => cb(this) };\n\
                 return 1;\n\
             }\n\
         }\n",
    );

    assert!(
        output.contains("var _this = this;"),
        "Accessor arrows should use the standard lexical this alias when it is free.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return cb(_this);"),
        "Nested arrows should read the standard accessor this alias.\nOutput:\n{output}"
    );
}

#[test]
fn static_members_capture_arrow_this_with_member_local_alias() {
    let output = emit_es5(
        "class C {\n\
             static field = this;\n\
             static get value() { () => this.field; return null; }\n\
             static set value(next) { () => { this.field = next; }; }\n\
             static method() { () => this.value; }\n\
         }\n",
    );

    assert!(
        output.contains("var _this = this;"),
        "Static accessor/method arrows should capture the member-local this value.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _this.field;"),
        "Static getter arrow should read through the member-local this alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_this.field = next;"),
        "Static setter arrow should assign through the member-local this alias.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return _this.value;"),
        "Static method arrow should read through the member-local this alias.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_labeled_block_break_after_await_lowers_to_generator_jump() {
    let output = emit_es5(
        "async function f() {\n\
             block: {\n\
                 await 1;\n\
                 break block;\n\
             }\n\
         }\n",
    );

    assert!(
        !output.contains("block:") && !output.contains("await 1"),
        "Raw labeled block and await syntax should not remain in ES5 output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 0: return [4 /*yield*/, 1];"),
        "Await should lower to the initial yield case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 2];"),
        "Labeled break should jump to the post-block case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("case 2: return [2 /*return*/];"),
        "Post-block case should contain the final generator return.\nOutput:\n{output}"
    );
}

// =============================================================================
// Array Destructuring
// =============================================================================

#[test]
fn test_array_destructuring_basic() {
    let output = emit_es5("const [a, b] = arr;\n");
    assert!(
        !output.contains("[a, b]"),
        "ES5 output should not contain array destructuring syntax.\nOutput:\n{output}"
    );
    // Should have temp variable
    assert!(
        output.contains("var") || output.contains("_a"),
        "Expected ES5 variable declaration.\nOutput:\n{output}"
    );
}

#[test]
fn test_object_destructuring_basic() {
    let output = emit_es5("const {x, y} = obj;\n");
    assert!(
        !output.contains("{x, y}"),
        "ES5 output should not contain object destructuring syntax.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_with_default_value() {
    let output = emit_es5("const {x = 5} = obj;\n");
    assert!(
        output.contains("void 0") || output.contains("undefined") || output.contains("5"),
        "Expected default value handling.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rest_array() {
    let output = emit_es5("const [first, ...rest] = arr;\n");
    assert!(
        output.contains("slice"),
        "Expected Array.prototype.slice for rest elements.\nOutput:\n{output}"
    );
}

#[test]
fn for_in_missing_destructuring_initializer_uses_void_temp() {
    let output = emit_es5("for (var [a, b] in []) { }\n");

    assert!(
        output.contains("for (var _a = void 0, a = _a[0], b = _a[1] in [])"),
        "ES5 for-in destructuring without an initializer should read from one void temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(void 0)[0]") && !output.contains("(void 0)[1]"),
        "ES5 for-in destructuring must not repeat void reads for each binding.\nOutput:\n{output}"
    );
}

#[test]
fn test_assignment_object_rest_uses_es5_lowering() {
    // Regression: when targeting ES5 the new ES2018 object-rest assignment
    // handler must NOT intercept dispatch. The ES5 destructuring lowering
    // already emits fully ES5-compatible output (no `{ a } = src` syntax).
    let output = emit_es5("var a, rest;\n({a, ...rest} = obj);\n");
    // The buggy ES2018 path emits `{ a: a } = obj, rest = __rest(obj, ["a"])`
    // — the `{ a: a } = obj` portion is ES2015+ destructuring, invalid for ES5.
    // The correct ES5 lowering emits `a = obj.a, rest = __rest(obj, ["a"])`.
    assert!(
        !output.contains("} = obj"),
        "ES5 output must not contain object destructuring assignment syntax.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ a") && !output.contains("{a"),
        "ES5 output must not retain an object literal pattern on the LHS.\nOutput:\n{output}"
    );
    assert!(
        output.contains("a = obj.a") || output.contains("a = obj[\"a\"]"),
        "Expected ES5-compatible property assignment for non-rest binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__rest"),
        "Expected __rest helper for object rest.\nOutput:\n{output}"
    );
}

#[test]
fn assignment_object_rest_strips_redundant_paren_around_object_rhs() {
    // Regression for `destructuringObjectBindingPatternAndAssignment5`: when the
    // RHS of an assignment-form object-rest destructuring is a parenthesized
    // type-erased object literal (`({ } as any)`), the lowering threads the
    // RHS into a temp inside a comma expression — never at statement-leading
    // position, so the outer paren is redundant. tsc emits `_a = {}` here, not
    // `_a = ({})`.
    let output = emit_with_target(
        r#"
function a() {
    let x: number;
    let y: any;
    ({ x, ...y } = ({ } as any));
}
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("_a = {}, "),
        "RHS object literal must be unwrapped from its redundant paren.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a = ({})"),
        "Redundant paren must not be preserved at non-statement-leading position.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_computed_exclusion_reuses_key_temp() {
    let output = emit_es5(
        r#"
declare function getKey(): string;
const source: any = {};
const { [getKey()]: value = 1, ...rest } = source;
console.log(value);
"#,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "Computed rest key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" = source[_"),
        "Computed property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "Computed property default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "__rest exclusion should use TypeScript's symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__rest(source, [getKey()])"),
        "__rest exclusion must not re-evaluate the computed key expression.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_computed_exclusion_reuses_key_temp() {
    let output = emit_es5(
        r#"
declare function getKey(): string;
let value: any;
let rest: any;
const source: any = {};
({ [getKey()]: value = 1, ...rest } = source);
console.log(value);
"#,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "Computed rest assignment key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("source[_"),
        "Computed assignment property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "Computed assignment default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "Assignment __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("source[\"\"]") && !output.contains("__rest(source, [])"),
        "Assignment lowering must not drop the computed key.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_es2017_computed_default_reuses_key_temp() {
    let output = emit_with_target(
        r#"
declare function getKey(): string;
const source: any = {};
const { [getKey()]: value = 1, ...rest } = source;
console.log(value);
"#,
        ScriptTarget::ES2017,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "ES2017 computed rest key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[_"),
        "ES2017 computed property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "ES2017 computed property default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "ES2017 __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_es2017_computed_key_with_nested_pattern_lowers_nested_binding() {
    let output = emit_with_target(
        r#"
declare function order(n: number): any;
let { [order(0)]: { [order(2)]: z } = order(1), ...w } = {} as any;
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        !output.contains(",  ="),
        "Nested binding pattern under computed object-rest key must not emit an empty binding name.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("order(0)").count(),
        1,
        "Outer computed key should be evaluated once.\nOutput:\n{output}"
    );
    assert_eq!(
        output.matches("order(2)").count(),
        1,
        "Nested computed key should be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("z = _"),
        "Nested computed binding should decompose from the defaulted value temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("z = _d[_e], w = __rest("),
        "Nested binding output must remain comma-separated before the outer rest lowering.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__rest("),
        "Outer object rest should still lower to __rest.\nOutput:\n{output}"
    );
}

#[test]
fn object_rest_assignment_es2017_computed_exclusion_reuses_key_temp() {
    let output = emit_with_target(
        r#"
declare function getKey(): string;
let value: any;
let rest: any;
const source: any = {};
({ [getKey()]: value = 1, ...rest } = source);
console.log(value);
"#,
        ScriptTarget::ES2017,
    );

    assert_eq!(
        output.matches("getKey()").count(),
        1,
        "ES2017 computed rest assignment key expression must be evaluated once.\nOutput:\n{output}"
    );
    assert!(
        output.contains("source[_"),
        "ES2017 computed assignment property read should use a temp key.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === void 0 ? 1 : "),
        "ES2017 computed assignment default should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains(" === \"symbol\" ? ") && output.contains(" + \"\""),
        "ES2017 assignment __rest exclusion should use symbol-safe computed-key coercion.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__rest(source, [])") && !output.contains("__rest(source, [getKey()])"),
        "ES2017 assignment lowering must not drop or re-evaluate the computed key.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_object_rest_parameter_keeps_later_default_in_body() {
    let output = emit_with_target(
        "function f({ a, ...x }: any, b = a) { return b; }\n",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("function f(_a, b) {"),
        "Object-rest parameter lowering should replace only the binding pattern with a temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var { a } = _a, x = __rest(_a, [\"a\"]);"),
        "Object-rest parameter should lower to a body prologue before later defaults.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (b === void 0) { b = a; }"),
        "A later default that references the lowered binding must run after the prologue.\nOutput:\n{output}"
    );
}

#[test]
fn es2015_object_rest_parameter_prologue_follows_directives() {
    let output = emit_with_target(
        "function f({ a = {}, ...rest }: any = {}) {\n\
             \"use strict\";\n\
             \"another directive\";\n\
             rest.value(a);\n\
         }\n\
         class C {\n\
             constructor({ a = {}, ...rest }: any = {}) {\n\
                 \"use strict\";\n\
                 \"another directive\";\n\
                 rest.value(a);\n\
             }\n\
         }\n",
        ScriptTarget::ES2015,
    );

    let function_directive = output
        .find("function f(_a = {}) {\n    \"use strict\";\n    \"another directive\";")
        .unwrap_or_else(|| panic!("function directives should stay first.\nOutput:\n{output}"));
    let function_prologue = output
        .find("var { a = {} } = _a, rest = __rest(_a, [\"a\"]);")
        .unwrap_or_else(|| {
            panic!("function object-rest parameter prologue should be emitted.\nOutput:\n{output}")
        });
    let function_body = output
        .find("rest.value(a);")
        .unwrap_or_else(|| panic!("function body should be emitted.\nOutput:\n{output}"));

    assert!(
        function_directive < function_prologue && function_prologue < function_body,
        "Function object-rest parameter prologue should follow directives and precede body statements.\nOutput:\n{output}"
    );

    let constructor_directive = output
        .find("constructor(_a = {}) {\n        \"use strict\";\n        \"another directive\";")
        .unwrap_or_else(|| panic!("constructor directives should stay first.\nOutput:\n{output}"));
    let constructor_prologue = output
        .rfind("var { a = {} } = _a, rest = __rest(_a, [\"a\"]);")
        .unwrap_or_else(|| {
            panic!(
                "constructor object-rest parameter prologue should be emitted.\nOutput:\n{output}"
            )
        });
    let constructor_body = output
        .rfind("rest.value(a);")
        .unwrap_or_else(|| panic!("constructor body should be emitted.\nOutput:\n{output}"));

    assert!(
        constructor_directive < constructor_prologue && constructor_prologue < constructor_body,
        "Constructor object-rest parameter prologue should follow directives and precede body statements.\nOutput:\n{output}"
    );
}

#[test]
fn es5_defaulted_object_rest_parameter_uses_parameter_guard() {
    let output = emit_es5(
        "function f({ x: { z = 12, ...nested }, ...rest } = { x: { z: 1, ka: 1 }, y: 'noo' }) {\n\
             return rest.y + nested.ka;\n\
         }\n",
    );

    assert!(
        output.contains("if (_a === void 0) { _a = { x: { z: 1, ka: 1 }, y: 'noo' }; }"),
        "Defaulted object-rest params should default the parameter temp before destructuring.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "var _b = _a.x, _c = _b.z, z = _c === void 0 ? 12 : _c, nested = __rest(_b, [\"z\"]), rest = __rest(_a, [\"x\"]);"
        ),
        "Nested and outer object-rest bindings should read from the defaulted parameter temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_downlevel_iteration_array_parameter_uses_read_helper() {
    let output = emit_es5_downlevel_iteration(
        "function one([], {}) {}\n\
         function two([], [a, b, c]: number[]) {}\n",
    );

    assert!(
        output.contains("function one(_a, _b)"),
        "Empty binding parameters should still be replaced with parameter temps.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("__read(_a, 0)"),
        "Empty array binding parameters should not create an unused read helper call.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _c = __read(_b, 3), a = _c[0], b = _c[1], c = _c[2];"),
        "Non-empty array binding parameters under downlevelIteration must read the iterable once before indexing.\nOutput:\n{output}"
    );
}

#[test]
fn es5_downlevel_iteration_nested_array_parameter_reads_each_array_pattern() {
    let output = emit_es5_downlevel_iteration(
        "function nested([[a], , ...rest]: Iterable<any>) { return a; }\n",
    );

    assert!(
        output.contains("__read(_a)"),
        "Array binding parameters with rest should read the parameter without a fixed limit.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__read(_b[0], 1)"),
        "Nested array binding patterns should also read their iterable source before indexing.\nOutput:\n{output}"
    );
    assert!(
        output.contains("rest = _b.slice(2)"),
        "Array rest parameters should slice from the read array temp.\nOutput:\n{output}"
    );
}

#[test]
fn es5_downlevel_iteration_method_like_array_parameters_mark_read_helper() {
    let output = emit_es5_downlevel_iteration(
        "class C {\n\
             constructor([ctorValue]: Iterable<any>) { }\n\
             m([methodValue]: Iterable<any>) { }\n\
             set p([setterValue]: Iterable<any>) { }\n\
         }\n",
    );

    assert!(
        output.contains("var __read ="),
        "Method-like array binding parameters should schedule the __read helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("function C(_a) {\n        var _b = __read(_a, 1), ctorValue = _b[0];"),
        "Constructor array binding parameters should read the iterable source.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "C.prototype.m = function (_a) {\n        var _b = __read(_a, 1), methodValue = _b[0];"
        ),
        "Method array binding parameters should read the iterable source.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "set: function (_a) {\n            var _b = __read(_a, 1), setterValue = _b[0];"
        ),
        "Accessor array binding parameters should read the iterable source.\nOutput:\n{output}"
    );
}

#[test]
fn es5_class_method_object_rest_parameter_uses_rest_helper() {
    let output = emit_es5(
        "class C {\n\
             m({ a, ...clone }: any) { }\n\
             set p({ a, ...clone }: any) { }\n\
         }\n",
    );

    assert!(
        output.contains(
            "C.prototype.m = function (_a) {\n        var a = _a.a, clone = __rest(_a, [\"a\"]);"
        ),
        "ES5 class methods should lower object-rest parameters through the class IR prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "set: function (_a) {\n            var a = _a.a, clone = __rest(_a, [\"a\"]);"
        ),
        "ES5 class accessors should lower object-rest parameters through the class IR prologue.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_nested_array_patterns_inline_simple_sources() {
    let output = emit_es5(
        "function f0(a: any, [a, [b]]: any, { b }: any) { }\n\
         function f3([c, [c], [[c]]]: any) { }\n",
    );

    assert!(
        output.contains("var a = _a[0], b = _a[1][0];"),
        "Nested array parameter bindings without defaults should read through the source chain.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var c = _a[0], c = _a[1][0], c = _a[2][0][0];"),
        "Deep nested array parameter bindings should not allocate intermediary value temps.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_c = _a[1]") && !output.contains("_d = _c[0]"),
        "Simple nested array parameter bindings should not create temp-only source aliases.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_nested_object_patterns_inline_simple_sources() {
    let output = emit_es5(
        "function f4({ d, d: { d } }: any) { }\n\
         function f5({ e, e: { e } }: any, { e }: any, [d, e, [[e]]]: any, ...e: any[]) { }\n",
    );

    assert!(
        output.contains("var d = _a.d, d = _a.d.d;"),
        "Nested object parameter bindings without defaults should read through the source chain.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var e = _a.e, e = _a.e.e;"),
        "Repeated object parameter bindings should keep tsc's direct chained source reads.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var d = _c[0], e = _c[1], e = _c[2][0][0];"),
        "Object and array parameter prologues should share the same inline nested-source policy.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_b = _a.d") && !output.contains("_d = _a.e"),
        "Simple nested object parameter bindings should not create temp-only source aliases.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_string_literal_nested_object_patterns_keep_temp_path() {
    let output = emit_es5(
        "function f({ \"not-ident\": { value } }: any) { }\n\
         function g({ \"not-ident\": [first] }: any) { }\n",
    );

    assert!(
        output.contains("var _b = _a[\"not-ident\"], value = _b.value;"),
        "String-literal nested object parameter sources should keep the temp path.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _b = _a[\"not-ident\"], first = _b[0];"),
        "String-literal nested array parameter sources should keep the temp path.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("_a[\"not-ident\"].value") && !output.contains("_a[\"not-ident\"][0]"),
        "String-literal nested parameter sources should not use direct chained reads.\nOutput:\n{output}"
    );
}

#[test]
fn es5_param_empty_nested_patterns_keep_source_reads() {
    let output = emit_es5(
        "function f0([[]]: any) { }\n\
         function f1({ a: {} }: any) { }\n",
    );

    assert!(
        output.contains("var _b = _a[0];"),
        "Empty nested array parameter patterns should still read the nested source.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _b = _a.a;"),
        "Empty nested object parameter patterns should still read the nested source.\nOutput:\n{output}"
    );
}

#[test]
fn invalid_nonlast_object_rest_is_recovery_only() {
    let output = emit_with_target(
        "var {...a, x } = { x: 1 };\n\
         ({...a, x } = { x: 1 });\n\
         var {...a, x, ...b } = { x: 1 };\n\
         ({...a, x, ...b } = { x: 1 });\n",
        ScriptTarget::ES2015,
    );

    assert!(
        output.contains("var _c = { x: 1 }, { x } = _c;"),
        "A nonlast binding rest should be skipped while preserving tsc's temp-based recovery.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_a = { x: 1 }, { x } = _a);"),
        "A nonlast assignment rest should be skipped while preserving later property assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _d = { x: 1 }, { x } = _d, b = __rest(_d, [\"a\", \"x\"]);"),
        "A later valid binding rest should keep the invalid rest identifier in its exclude list.\nOutput:\n{output}"
    );
    assert!(
        output.contains("(_b = { x: 1 }, { x } = _b, b = __rest(_b, [\"x\"]));"),
        "Assignment recovery should not add the invalid rest expression to the later exclude list.\nOutput:\n{output}"
    );
}

#[test]
fn array_object_rest_es2017_defers_later_default_until_after_rest_binding() {
    let output = emit_with_target(
        "let [{ ...a }, b = a]: any[] = [{ x: 1 }];",
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("__rest("),
        "Nested object rest inside an array binding should lower to __rest.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("{ ...a }") && !output.contains("b = a] ="),
        "Lowering must remove object rest and defer the later default out of the array head.\nOutput:\n{output}"
    );
    let rest_pos = output
        .find("a = __rest")
        .expect("expected a deferred object-rest binding");
    let default_pos = output
        .find("b = _")
        .expect("expected a deferred default binding");
    assert!(
        rest_pos < default_pos,
        "Default initializer that references the rest binding must run after the rest binding.\nOutput:\n{output}"
    );
}

#[test]
fn nested_object_rest_lowers_when_outer_pattern_has_no_rest() {
    // Regression for `narrowingDestructuring`: when the outer object pattern has
    // no rest element but a nested element does (e.g. `{ f: { a, ...spread } }`),
    // the lowering pass must still introduce a nested temp and a `__rest` call.
    // The bug was a partial `needs_temp` predicate that only fired when the outer
    // pattern itself carried a rest element, leaving the source un-lowered.
    let output = emit_with_target(
        r#"
declare const value: { f: { a: number; b: string; c: number } };
const { f: { a, ...spread } } = value;
"#,
        ScriptTarget::ES2017,
    );

    assert!(
        output.contains("__rest("),
        "Nested object rest must be lowered to a __rest() call.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("...spread"),
        "Lowered output must not retain the rest spread syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("value.f"),
        "Lowering should thread `value.f` into a temp before the __rest call.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_nested_object() {
    let output = emit_es5("const {a: {b}} = obj;\n");
    assert!(
        !output.contains("{a:"),
        "ES5 should not contain nested destructuring syntax.\nOutput:\n{output}"
    );
    // Should reference .a and .b through temp variables
    assert!(
        output.contains(".a") || output.contains("[\"a\"]"),
        "Expected property access for nested destructuring.\nOutput:\n{output}"
    );
}

#[test]
fn test_destructuring_rename() {
    let output = emit_es5("const {x: renamed} = obj;\n");
    assert!(
        output.contains("renamed"),
        "Expected renamed binding.\nOutput:\n{output}"
    );
}

// =============================================================================
// Class ES5 Transform
// =============================================================================

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

// =============================================================================
// Arrow Function ES5 Transform
// =============================================================================

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

// =============================================================================
// Let/Const -> Var
// =============================================================================

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

// =============================================================================
// Async Function Transform
// =============================================================================

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

#[test]
fn async_arrow_import_meta_hoisted_locals_share_var_statement() {
    let output = emit_es5_with_module(
        "(async () => {\n\
             const response = await fetch(new URL(\"../hamsters.jpg\", import.meta.url).toString());\n\
             const blob = await response.blob();\n\
             \n\
             const size = import.meta.scriptElement.dataset.size || 300;\n\
             \n\
             const image = new Image();\n\
             image.src = URL.createObjectURL(blob);\n\
             image.width = image.height = size;\n\
             \n\
             document.body.appendChild(image);\n\
         })();\n",
        ModuleKind::CommonJS,
    );

    assert!(
        output.contains("var response, blob, size, image;"),
        "Async arrow import.meta hoisted locals should share one var statement.\nOutput:\n{output}"
    );
}

#[test]
fn system_import_meta_file_is_wrapped_as_module() {
    let output = emit_es5_with_module(
        "(async () => {\n\
             const response = await fetch(new URL(\"../hamsters.jpg\", import.meta.url).toString());\n\
             const blob = await response.blob();\n\
             \n\
             const size = import.meta.scriptElement.dataset.size || 300;\n\
             \n\
             const image = new Image();\n\
             image.src = URL.createObjectURL(blob);\n\
             image.width = image.height = size;\n\
             \n\
             document.body.appendChild(image);\n\
         })();\n",
        ModuleKind::System,
    );

    assert!(
        output.starts_with("System.register([], function (exports_1, context_1) {"),
        "System import.meta files should be module-wrapped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("context_1.meta.url"),
        "System import.meta should lower to context_1.meta.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"use strict\";\n    var __awaiter"),
        "System async helpers should be emitted inside the wrapper after the strict prologue.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var response, blob, size, image;"),
        "System async arrow hoisted locals should share one var statement.\nOutput:\n{output}"
    );
}

#[test]
fn system_import_meta_preserves_import_property_lookalikes() {
    let output = emit_es5_with_module(
        "export let x = import.meta;\n\
         export let y = import.metal;\n\
         export let z = import.import.import.malkovich;\n",
        ModuleKind::System,
    );

    assert!(
        output.contains("exports_1(\"x\", x = context_1.meta);"),
        "System should lower only real import.meta.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"y\", y = import.metal);"),
        "System should preserve import.metal lookalikes.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports_1(\"z\", z = import.import.import.malkovich);"),
        "System should preserve nested import property lookalikes.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("context_1.metal") && !output.contains("context_1.import"),
        "System import.meta lowering must not rewrite non-meta import properties.\nOutput:\n{output}"
    );
}

#[test]
fn test_async_while_with_await_lowers_loop_body() {
    let output = emit_es5(
        "async function f(xs) {\n    while (xs.length) {\n        await g(xs.pop());\n    }\n}\n",
    );

    assert!(
        !output.contains("while (xs.length)"),
        "Source while loop should be lowered into generator cases.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("await "),
        "await keyword should not appear in ES5.\nOutput:\n{output}"
    );
    assert!(
        output.contains("if (!xs.length) return [3 /*break*/, 2];"),
        "Loop condition should branch to the exit case.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [4 /*yield*/, g(xs.pop())];"),
        "Await in the loop body should become a generator yield.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [3 /*break*/, 0];"),
        "Loop body should jump back to the condition case.\nOutput:\n{output}"
    );
}

// =============================================================================
// Template Literals
// =============================================================================

#[test]
fn test_template_literal_to_concatenation() {
    let output = emit_es5("const msg = `Hello ${name}!`;\n");
    // ES5 should convert template literals to string concatenation
    assert!(
        !output.contains('`'),
        "ES5 should not contain template literal syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("+") || output.contains("concat"),
        "Expected string concatenation.\nOutput:\n{output}"
    );
}

// =============================================================================
// Spread Transform
// =============================================================================

#[test]
fn test_spread_in_call() {
    let output = emit_es5("foo(...args);\n");
    assert!(
        !output.contains("...args"),
        "ES5 should not contain spread syntax.\nOutput:\n{output}"
    );
    assert!(
        output.contains("apply") || output.contains("__spreadArray"),
        "Expected apply or __spreadArray for spread.\nOutput:\n{output}"
    );
}

// =============================================================================
// Exponentiation Transform (ES2016)
// =============================================================================

#[test]
fn test_exponentiation_to_math_pow() {
    let output = emit_es5("const x = 2 ** 3;\n");
    assert!(
        output.contains("Math.pow"),
        "Expected Math.pow for exponentiation.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("**"),
        "ES5 should not contain ** operator.\nOutput:\n{output}"
    );
}

// =============================================================================
// Enum ES5 Transform
// =============================================================================

#[test]
fn test_enum_to_iife() {
    let output = emit_es5_with_comments("enum Color {\n    Red,\n    Green,\n    Blue\n}\n");
    // Enums become IIFEs in ES5
    assert!(
        output.contains("Color[Color[") || output.contains("Color[\"Red\"]"),
        "Expected enum IIFE pattern.\nOutput:\n{output}"
    );
}

// =============================================================================
// Type Stripping
// =============================================================================

#[test]
fn test_type_annotations_stripped() {
    let output = emit_es5("const x: number = 42;\n");
    assert!(
        !output.contains(": number"),
        "Type annotations should be stripped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("42"),
        "Value should be preserved.\nOutput:\n{output}"
    );
}

#[test]
fn test_interface_stripped() {
    let output = emit_es5("interface Point { x: number; y: number; }\n");
    assert!(
        !output.contains("interface"),
        "Interface should be stripped from JS output.\nOutput:\n{output}"
    );
}

#[test]
fn test_type_alias_stripped() {
    let output = emit_es5("type ID = string | number;\n");
    assert!(
        !output.contains("type ID"),
        "Type alias should be stripped from JS output.\nOutput:\n{output}"
    );
}

// Structural rule: when an async function targeting ES5 contains a dynamic
// import call and the module system is CommonJS, the IR transformer must lower
// `import("mod")` to `Promise.resolve().then(function () { return
// __importStar(require("mod")); })` — the same form the regular printer emits.
// The name of the specifier and the presence/absence of other awaits are
// irrelevant to the lowering rule; any string-literal specifier must produce
// the same pattern.

#[test]
fn async_es5_cjs_dynamic_import_lowered_to_promise_resolve_require() {
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./mod\"); }",
        ModuleKind::CommonJS,
    );
    assert!(
        output.contains("Promise.resolve().then("),
        "CJS async ES5: import() must become Promise.resolve().then(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require(\"./mod\")"),
        "CJS async ES5: require() with the original specifier must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "CJS async ES5: raw import() call must not remain in output.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_cjs_dynamic_import_different_specifier_also_lowered() {
    // Prove the rule operates on the specifier value, not just "mod".
    let output = emit_es5_with_module(
        "async function load() { return await import(\"@scope/package\"); }",
        ModuleKind::CommonJS,
    );
    assert!(
        output.contains("require(\"@scope/package\")"),
        "CJS async ES5: specifier must be preserved verbatim in require().\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "CJS async ES5: raw import() call must not remain.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_amd_dynamic_import_lowered_to_new_promise_require() {
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./amd-mod\"); }",
        ModuleKind::AMD,
    );
    assert!(
        output.contains("new Promise("),
        "AMD async ES5: import() must be wrapped in new Promise(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require([\"./amd-mod\"]"),
        "AMD async ES5: AMD-style require([specifier]) must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "AMD async ES5: raw import() must not remain in output.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_umd_dynamic_import_lowered_same_as_amd() {
    // UMD and AMD share the same promise-based require wrapper.
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./umd-lib\"); }",
        ModuleKind::UMD,
    );
    assert!(
        output.contains("new Promise("),
        "UMD async ES5: import() must be wrapped in new Promise(...).\nOutput:\n{output}"
    );
    assert!(
        output.contains("require([\"./umd-lib\"]"),
        "UMD async ES5: AMD-style require() must appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("import("),
        "UMD async ES5: raw import() must not remain.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_no_dynamic_import_lowering_for_esnext() {
    // ESNext does not lower dynamic imports — import() must pass through.
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./esm\"); }",
        ModuleKind::ESNext,
    );
    assert!(
        output.contains("import("),
        "ESNext async ES5: import() must pass through unchanged.\nOutput:\n{output}"
    );
}

#[test]
fn async_es5_system_dynamic_import_lowered_to_context_import() {
    // System module: import() → context_1.import(specifier)
    let output = emit_es5_with_module(
        "async function load() { return await import(\"./sys-mod\"); }",
        ModuleKind::System,
    );
    assert!(
        output.contains("context_1.import(\"./sys-mod\")"),
        "System async ES5: import() must become context_1.import(...).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("require("),
        "System async ES5: require() must not appear.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Promise.resolve()"),
        "System async ES5: Promise.resolve() CJS form must not appear.\nOutput:\n{output}"
    );
}
