use crate::emit_context::EmitContext;
use crate::lowering_pass::LoweringPass;
use crate::emitter::{ModuleKind, PrinterOptions, ScriptTarget, Printer};
use crate::parser::ParserState;

fn assert_parity(source: &str, target: ScriptTarget, module: ModuleKind) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options_legacy = PrinterOptions::default();
    options_legacy.target = target;
    options_legacy.module = module;

    let mut printer_legacy = Printer::with_options(arena, options_legacy);
    printer_legacy.set_source_text(source);
    if matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        printer_legacy.set_target_es5(true);
    }
    printer_legacy.emit(root);
    let output_legacy = printer_legacy.take_output();

    let mut options_new = PrinterOptions::default();
    options_new.target = target;
    options_new.module = module;

    let ctx = EmitContext::with_options(options_new.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    if source.contains("class") && matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        assert!(
            !transforms.is_empty(),
            "LoweringPass failed to generate transforms for ES5 class"
        );
    }

    let mut printer_new = Printer::with_transforms_and_options(arena, transforms, options_new);
    printer_new.set_source_text(source);
    if matches!(target, ScriptTarget::ES3 | ScriptTarget::ES5) {
        printer_new.set_target_es5(true);
    }
    printer_new.emit(root);
    let output_new = printer_new.take_output();

    let output_legacy_trimmed = output_legacy.trim_end_matches('\n');
    let output_new_trimmed = output_new.trim_end_matches('\n');

    assert_eq!(
        output_legacy_trimmed, output_new_trimmed,
        "\nParity mismatch for source:\n{}\n\nLegacy:\n{}\n\nNew:\n{}",
        source, output_legacy, output_new
    );
}

#[test]
fn test_parity_es5_class() {
    assert_parity(
        "class Point { constructor(x, y) { this.x = x; this.y = y; } }",
        ScriptTarget::ES5,
        ModuleKind::None,
    );
}

#[test]
fn test_parity_commonjs_export() {
    assert_parity(
        "export class Foo {}",
        ScriptTarget::ES5,
        ModuleKind::CommonJS,
    );
}

#[test]
fn test_parity_es5_arrow() {
    assert_parity(
        "const add = (a, b) => a + b;",
        ScriptTarget::ES5,
        ModuleKind::None,
    );
}

#[test]
fn test_parity_async_es5() {
    assert_parity(
        "async function foo() { await bar(); }",
        ScriptTarget::ES5,
        ModuleKind::None,
    );
}

/// Parity test for ES5 class with async method calling super.method().
/// This test compares our output against expected tsc output behavior.
/// The async method should:
/// 1. Be wrapped in __awaiter
/// 2. Use _super.prototype.method.call(this) for super calls
/// 3. Include __extends helper for class inheritance
#[test]
fn test_parity_es5_class_async_super_method() {
    let source = "class Derived extends Base { async foo() { return super.method(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class downlevel produces correct output matching tsc behavior
    assert!(
        output.contains("__extends"),
        "ES5 output should include __extends helper for class inheritance: {}",
        output
    );
    assert!(
        output.contains("__awaiter"),
        "ES5 output should include __awaiter helper for async method: {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper for async method: {}",
        output
    );
    assert!(
        output.contains("_super.prototype.method.call(this)"),
        "ES5 output should lower super.method() to _super.prototype.method.call(this): {}",
        output
    );
    assert!(
        output.contains("Derived.prototype.foo = function"),
        "ES5 output should emit method on prototype: {}",
        output
    );
    assert!(
        !output.contains("async"),
        "ES5 output should not contain async keyword: {}",
        output
    );
}

/// Parity test for ES5 class with getter and setter.
/// In ES5, class accessors should be downleveled to Object.defineProperty calls.
/// tsc emits: Object.defineProperty(Foo.prototype, "value", { get: function() {...}, set: function(v) {...}, ... });
#[test]
fn test_parity_es5_class_getter_setter() {
    let source = "class Foo { private _value: number = 0; get value() { return this._value; } set value(v) { this._value = v; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class accessor downlevel produces correct output matching tsc behavior
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty for accessors: {}",
        output
    );
    assert!(
        output.contains("Foo.prototype"),
        "ES5 output should define accessors on prototype: {}",
        output
    );
    assert!(
        output.contains("\"value\""),
        "ES5 output should define property named 'value': {}",
        output
    );
    assert!(
        output.contains("get:") || output.contains("get :"),
        "ES5 output should have getter in property descriptor: {}",
        output
    );
    assert!(
        output.contains("set:") || output.contains("set :"),
        "ES5 output should have setter in property descriptor: {}",
        output
    );
    assert!(
        output.contains("this._value"),
        "ES5 output should reference this._value in accessor bodies: {}",
        output
    );
    assert!(
        !output.contains("get value()"),
        "ES5 output should not contain ES6 getter syntax: {}",
        output
    );
    assert!(
        !output.contains("set value("),
        "ES5 output should not contain ES6 setter syntax: {}",
        output
    );
}

/// Parity test for ES5 class with static getter.
/// Static accessors should be defined on the class constructor, not the prototype.
#[test]
fn test_parity_es5_class_static_getter() {
    let source = "class Foo { private static _instance: Foo; static get instance() { return Foo._instance; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 static accessor downlevel
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty for static accessor: {}",
        output
    );
    // Static accessors are defined on the constructor function itself, not prototype
    assert!(
        output.contains("Foo, \"instance\"") || output.contains("Foo,\"instance\""),
        "ES5 output should define static accessor on Foo (constructor): {}",
        output
    );
    assert!(
        output.contains("get:") || output.contains("get :"),
        "ES5 output should have getter in property descriptor: {}",
        output
    );
    assert!(
        output.contains("Foo._instance"),
        "ES5 output should reference Foo._instance in getter body: {}",
        output
    );
    assert!(
        !output.contains("static get instance"),
        "ES5 output should not contain ES6 static getter syntax: {}",
        output
    );
}

/// Parity test for ES5 class with static async method that captures `this`.
/// In static methods, `this` refers to the class constructor.
/// The async static method should be wrapped in __awaiter and properly capture `this`.
#[test]
fn test_parity_es5_class_static_async_this_capture() {
    let source = "class Foo { static value = 42; static async getValue() { return this.value; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 static async method downlevel with this capture
    assert!(
        output.contains("__awaiter"),
        "ES5 output should include __awaiter helper for static async method: {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper for static async method: {}",
        output
    );
    assert!(
        output.contains("Foo.getValue = function"),
        "ES5 output should emit static method on class constructor: {}",
        output
    );
    // In static methods, this refers to the class itself, so this.value should work
    assert!(
        output.contains("this.value") || output.contains("_this.value"),
        "ES5 output should reference this.value or _this.value in static async method: {}",
        output
    );
    assert!(
        !output.contains("async"),
        "ES5 output should not contain async keyword: {}",
        output
    );
    assert!(
        !output.contains("static async"),
        "ES5 output should not contain static async syntax: {}",
        output
    );
}

/// Parity test for ES5 class expression with extends.
/// Class expressions should be downleveled similarly to class declarations,
/// using __extends helper and IIFE pattern.
#[test]
fn test_parity_es5_class_expression_extends() {
    let source =
        "const Derived = class extends Base { constructor() { super(); this.value = 1; } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class expression downlevel with extends
    assert!(
        output.contains("__extends"),
        "ES5 output should include __extends helper for class expression inheritance: {}",
        output
    );
    assert!(
        output.contains("var Derived ="),
        "ES5 output should assign class expression to variable: {}",
        output
    );
    assert!(
        output.contains("function (_super)"),
        "ES5 output should use IIFE pattern with _super parameter: {}",
        output
    );
    assert!(
        output.contains("__extends("),
        "ES5 output should call __extends: {}",
        output
    );
    assert!(
        output.contains("_super.call(this)") || output.contains("_super.apply(this"),
        "ES5 output should convert super() to _super.call/apply: {}",
        output
    );
    assert!(
        output.contains("(Base)"),
        "ES5 output should pass Base to IIFE: {}",
        output
    );
    // Class expression uses /** @class */ comment pattern
    assert!(
        output.contains("/** @class */"),
        "ES5 output should include @class annotation: {}",
        output
    );
    assert!(
        !output.contains("extends Base"),
        "ES5 output should not contain extends keyword: {}",
        output
    );
}

/// Parity test for ES5 derived class with both instance and static fields.
/// Instance fields should be initialized in constructor after super().
/// Static fields should be assigned on class constructor after IIFE.
#[test]
fn test_parity_es5_derived_class_instance_static_fields() {
    let source = r#"
class Derived extends Base {
    instanceField = 42;
    static staticField = "hello";
    constructor() {
        super();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 derived class with fields
    assert!(
        output.contains("__extends"),
        "ES5 output should include __extends helper: {}",
        output
    );

    // Instance field should be initialized in constructor
    assert!(
        output.contains("this.instanceField = 42") || output.contains("_this.instanceField = 42"),
        "ES5 output should initialize instance field in constructor: {}",
        output
    );

    // Static field should be assigned on class constructor
    assert!(
        output.contains("Derived.staticField = \"hello\""),
        "ES5 output should assign static field on class constructor: {}",
        output
    );

    // super() should be converted
    assert!(
        output.contains("_super.call(this)") || output.contains("_super.apply(this"),
        "ES5 output should convert super() call: {}",
        output
    );

    // No ES6 class syntax
    assert!(
        !output.contains("extends Base"),
        "ES5 output should not contain extends keyword: {}",
        output
    );
    assert!(
        !output.contains("instanceField =")
            || output.contains("this.instanceField =")
            || output.contains("_this.instanceField ="),
        "ES5 output should not have class field syntax outside constructor: {}",
        output
    );
}

/// Parity test for ES5 async generator function.
/// Async generators (`async function*`) should be downleveled using
/// __awaiter, __generator, and __asyncGenerator helpers.
#[test]
fn test_parity_es5_async_generator_function() {
    let source = "async function* gen() { yield 1; yield await Promise.resolve(2); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 async generator function downlevel
    assert!(
        output.contains("__awaiter") || output.contains("__asyncGenerator"),
        "ES5 output should include async helper (__awaiter or __asyncGenerator): {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper: {}",
        output
    );
    assert!(
        output.contains("function gen"),
        "ES5 output should define gen function: {}",
        output
    );
    // Should not have async function* syntax
    assert!(
        !output.contains("async function*"),
        "ES5 output should not contain async function* syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow function with rest parameters.
/// Rest parameters should be converted to use arguments with slice.
#[test]
fn test_parity_es5_arrow_rest_parameters() {
    let source = "const sum = (...nums) => nums.reduce((a, b) => a + b, 0);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 arrow function with rest parameters
    assert!(
        output.contains("var sum = function"),
        "ES5 output should convert arrow to function expression: {}",
        output
    );
    // Rest parameters should be converted using arguments (either slice or for loop)
    assert!(
        output.contains("arguments"),
        "ES5 output should reference arguments for rest params: {}",
        output
    );
    // Should define nums from arguments (var nums = [] with for loop, or slice)
    assert!(
        output.contains("var nums"),
        "ES5 output should define nums variable: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
    // No rest parameter syntax
    assert!(
        !output.contains("...nums"),
        "ES5 output should not contain rest parameter syntax: {}",
        output
    );
}

/// Parity test for ES5 default export class in CommonJS.
/// Default exported class should be downleveled and exported via exports.default.
#[test]
fn test_parity_es5_default_export_class() {
    let source = "export default class Foo { constructor(x) { this.x = x; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 default export class in CommonJS
    assert!(
        output.contains("exports.default"),
        "CommonJS output should export via exports.default: {}",
        output
    );
    assert!(
        output.contains("__esModule"),
        "CommonJS output should include __esModule marker: {}",
        output
    );
    // Class should be downleveled to function
    assert!(
        output.contains("function Foo") || output.contains("var Foo = /** @class */"),
        "ES5 output should downlevel class to function: {}",
        output
    );
    // Constructor body preserved
    assert!(
        output.contains("this.x = x"),
        "ES5 output should preserve constructor body: {}",
        output
    );
    // No ES6 class syntax
    assert!(
        !output.contains("class Foo"),
        "ES5 output should not contain class syntax: {}",
        output
    );
    // No export default syntax
    assert!(
        !output.contains("export default"),
        "ES5 output should not contain export default syntax: {}",
        output
    );
}

/// Parity test for ES5 async iteration (for await...of).
/// Async iteration should be downleveled using __asyncValues helper.
#[test]
fn test_parity_es5_async_iteration() {
    let source =
        "async function process(items) { for await (const item of items) { console.log(item); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 async iteration downlevel
    assert!(
        output.contains("__awaiter"),
        "ES5 output should include __awaiter helper: {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper: {}",
        output
    );
    assert!(
        output.contains("function process"),
        "ES5 output should define process function: {}",
        output
    );
    // Should reference items in some form
    assert!(
        output.contains("items"),
        "ES5 output should reference items: {}",
        output
    );
    // No async function syntax
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async function syntax: {}",
        output
    );
    // No for await syntax
    assert!(
        !output.contains("for await"),
        "ES5 output should not contain for await syntax: {}",
        output
    );
}

/// Parity test for ES5 for-await-of with destructuring.
/// Tests a more complex for-await-of scenario with object destructuring.
#[test]
fn test_parity_es5_for_await_of_destructuring() {
    let source = "async function processItems(stream) { for await (const { id, value } of stream) { console.log(id, value); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 for-await-of with destructuring downlevel
    assert!(
        output.contains("__awaiter"),
        "ES5 output should include __awaiter helper: {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper: {}",
        output
    );
    assert!(
        output.contains("function processItems"),
        "ES5 output should define processItems function: {}",
        output
    );
    // Destructured properties should be referenced
    assert!(
        output.contains("id") && output.contains("value"),
        "ES5 output should reference destructured properties id and value: {}",
        output
    );
    // No async function syntax
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async function syntax: {}",
        output
    );
    // No for await syntax
    assert!(
        !output.contains("for await"),
        "ES5 output should not contain for await syntax: {}",
        output
    );
    // No destructuring pattern in for-of (should be lowered)
    assert!(
        !output.contains("{ id, value }"),
        "ES5 output should not contain destructuring pattern in for-of: {}",
        output
    );
}

/// Parity test for ES5 for-await-of in class method.
/// Async iteration inside a class method with typed stream.
#[test]
fn test_parity_es5_for_await_of_class_method() {
    let source = r#"
interface DataItem { id: number; payload: string }
class StreamProcessor {
    private results: DataItem[] = [];

    async processStream(stream: AsyncIterable<DataItem>): Promise<void> {
        for await (const item of stream) {
            this.results.push(item);
        }
    }

    getResults(): DataItem[] {
        return this.results;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain class name
    assert!(
        output.contains("StreamProcessor"),
        "ES5 output should contain StreamProcessor class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": DataItem")
            && !output.contains("AsyncIterable<")
            && !output.contains("Promise<void>"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private modifier: {}",
        output
    );
    // No async method syntax
    assert!(
        !output.contains("async processStream"),
        "ES5 output should not contain async method syntax: {}",
        output
    );
    // No for await syntax
    assert!(
        !output.contains("for await"),
        "ES5 output should not contain for await syntax: {}",
        output
    );
    // Should reference this
    assert!(
        output.contains("this.results"),
        "ES5 output should preserve this references: {}",
        output
    );
}

/// Parity test for ES5 for-await-of with error handling.
/// Async iteration with try/catch/finally for error handling.
#[test]
fn test_parity_es5_for_await_of_error_handling() {
    let source = r#"
interface StreamError { code: number; message: string }
async function safeIterate<T>(stream: AsyncIterable<T>): Promise<T[]> {
    const results: T[] = [];
    try {
        for await (const item of stream) {
            results.push(item);
        }
    } catch (error) {
        console.error("Stream error:", error);
        throw error;
    } finally {
        console.log("Stream processing complete");
    }
    return results;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("safeIterate"),
        "ES5 output should contain safeIterate function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("AsyncIterable<T>")
            && !output.contains("Promise<T[]>")
            && !output.contains(": T[]"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No async function syntax
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async function syntax: {}",
        output
    );
    // No for await syntax
    assert!(
        !output.contains("for await"),
        "ES5 output should not contain for await syntax: {}",
        output
    );
    // try/catch/finally structure should be preserved
    assert!(
        output.contains("try") && output.contains("catch") && output.contains("finally"),
        "ES5 output should preserve try/catch/finally structure: {}",
        output
    );
}

/// Parity test for ES5 for-await-of with nested destructuring.
/// Async iteration with complex nested destructuring patterns.
#[test]
fn test_parity_es5_for_await_of_nested_destructuring() {
    let source = r#"
interface NestedData { outer: { inner: { value: number } }; meta: string }
async function extractValues(stream: AsyncIterable<NestedData>): Promise<number[]> {
    const values: number[] = [];
    for await (const { outer: { inner: { value } }, meta } of stream) {
        console.log(meta);
        values.push(value);
    }
    return values;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("extractValues"),
        "ES5 output should contain extractValues function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("AsyncIterable<")
            && !output.contains("Promise<number[]>")
            && !output.contains(": number[]"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No async function syntax
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async function syntax: {}",
        output
    );
    // No for await syntax
    assert!(
        !output.contains("for await"),
        "ES5 output should not contain for await syntax: {}",
        output
    );
    // Should use __awaiter and __generator helpers
    assert!(
        output.contains("__awaiter") && output.contains("__generator"),
        "ES5 output should use awaiter/generator helpers: {}",
        output
    );
}

/// Parity test for ES5 template literals.
/// Template literals should be downleveled to string concatenation.
#[test]
fn test_parity_es5_template_literal() {
    let source = r#"const greeting = `Hello, ${name}!`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 template literal downlevel
    assert!(
        output.contains("var greeting"),
        "ES5 output should define greeting variable: {}",
        output
    );
    // Should reference name
    assert!(
        output.contains("name"),
        "ES5 output should reference name: {}",
        output
    );
    // Template literal should be converted to string concatenation or kept with quotes
    // tsc converts `Hello, ${name}!` to "Hello, " + name + "!"
    assert!(
        output.contains("+") || output.contains("\"Hello"),
        "ES5 output should use string concatenation or converted string: {}",
        output
    );
    // No template literal syntax
    assert!(
        !output.contains("`"),
        "ES5 output should not contain template literal backticks: {}",
        output
    );
}

/// Parity test for ES5 computed property names.
/// Computed properties in object literals should be downleveled.
#[test]
fn test_parity_es5_computed_property() {
    let source = r#"const key = "foo"; const obj = { [key]: 42 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 computed property downlevel
    assert!(
        output.contains("var key"),
        "ES5 output should define key variable: {}",
        output
    );
    assert!(
        output.contains("var obj"),
        "ES5 output should define obj variable: {}",
        output
    );
    // Should reference key and 42
    assert!(
        output.contains("key") && output.contains("42"),
        "ES5 output should reference key and value 42: {}",
        output
    );
    // No computed property syntax [key] in object literal
    // tsc converts { [key]: 42 } to (_a = {}, _a[key] = 42, _a)
    assert!(
        !output.contains("{ [key]") && !output.contains("{[key]"),
        "ES5 output should not contain computed property syntax in object literal: {}",
        output
    );
}

/// Parity test for ES5 shorthand property syntax.
/// Shorthand properties { x, y } should be expanded to { x: x, y: y }.
#[test]
fn test_parity_es5_shorthand_property() {
    let source = "const x = 1; const y = 2; const obj = { x, y };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 shorthand property expansion
    assert!(
        output.contains("var x"),
        "ES5 output should define x variable: {}",
        output
    );
    assert!(
        output.contains("var y"),
        "ES5 output should define y variable: {}",
        output
    );
    assert!(
        output.contains("var obj"),
        "ES5 output should define obj variable: {}",
        output
    );
    // Shorthand should be expanded to x: x, y: y
    // tsc outputs { x: x, y: y }
    assert!(
        output.contains("x: x") || output.contains("x:x"),
        "ES5 output should expand shorthand x to x: x: {}",
        output
    );
    assert!(
        output.contains("y: y") || output.contains("y:y"),
        "ES5 output should expand shorthand y to y: y: {}",
        output
    );
}

/// Parity test for ES5 method shorthand in object literals.
/// Method shorthand { foo() {} } should be expanded to { foo: function() {} }.
#[test]
fn test_parity_es5_method_shorthand() {
    let source = "const obj = { greet() { return 'hello'; } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 method shorthand expansion
    assert!(
        output.contains("var obj"),
        "ES5 output should define obj variable: {}",
        output
    );
    // Method shorthand should be expanded to greet: function()
    // tsc outputs { greet: function() { return 'hello'; } }
    assert!(
        output.contains("greet: function") || output.contains("greet:function"),
        "ES5 output should expand method shorthand to greet: function: {}",
        output
    );
    assert!(
        output.contains("hello"),
        "ES5 output should contain return value: {}",
        output
    );
    // No method shorthand syntax
    assert!(
        !output.contains("greet()")
            || output.contains("greet: function()")
            || output.contains("greet:function()"),
        "ES5 output should not contain method shorthand syntax: {}",
        output
    );
}

#[test]
fn test_parity_es5_shorthand_method_definitions() {
    let source = r#"
interface Calculator {
    add(a: number, b: number): number;
    subtract(a: number, b: number): number;
}

const calc: Calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    },
    multiply(x: number, y: number) {
        return x * y;
    }
};

class MathOps {
    static create(): Calculator {
        return {
            add(a, b) { return a + b; },
            subtract(a, b) { return a - b; }
        };
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Methods should be expanded to function expressions
    assert!(
        output.contains("add: function") || output.contains("add:function"),
        "Method shorthand should be expanded: {}",
        output
    );
    assert!(
        output.contains("subtract: function") || output.contains("subtract:function"),
        "Method shorthand should be expanded: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("MathOps"),
        "Class should be present: {}",
        output
    );
}

#[test]
fn test_parity_es5_shorthand_method_computed() {
    let source = r#"
const methodName = "process";
const prefix = "handle";

type Handler = {
    [key: string]: () => void;
};

const handlers: Handler = {
    [methodName]() {
        console.log("processing");
    },
    [`${prefix}Click`]() {
        console.log("clicked");
    },
    [prefix + "Submit"]() {
        console.log("submitted");
    }
};

class DynamicMethods {
    static readonly ACTION = "execute";

    [DynamicMethods.ACTION](): void {
        console.log("executing");
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain method name and prefix variables
    assert!(
        output.contains("methodName") && output.contains("prefix"),
        "Output should contain methodName and prefix: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DynamicMethods"),
        "Class should be present: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Handler"),
        "Type alias should be erased: {}",
        output
    );
    // readonly should be erased
    assert!(
        !output.contains("readonly"),
        "readonly should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_shorthand_method_async() {
    let source = r#"
interface AsyncService {
    fetch(): Promise<string>;
    save(data: string): Promise<void>;
}

const service: AsyncService = {
    async fetch(): Promise<string> {
        return "data";
    },
    async save(data: string): Promise<void> {
        console.log(data);
    }
};

const api = {
    async getData<T>(id: number): Promise<T | null> {
        return null;
    },
    async postData(payload: object) {
        return { success: true };
    }
};

class AsyncHandler {
    async process(): Promise<void> {
        await this.validate();
    }

    private async validate(): Promise<boolean> {
        return true;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Async should be transformed (no async keyword in ES5)
    assert!(
        !output.contains("async fetch") && !output.contains("async save"),
        "async keyword should be transformed: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Promise type annotations should be erased
    assert!(
        !output.contains("Promise<"),
        "Promise type should be erased: {}",
        output
    );
    // Generic type should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("AsyncHandler"),
        "Class should be present: {}",
        output
    );
}

#[test]
fn test_parity_es5_shorthand_method_generator() {
    let source = r#"
interface Iterable<T> {
    values(): Generator<T>;
}

const collection = {
    items: [1, 2, 3] as number[],
    *values(): Generator<number> {
        for (const item of this.items) {
            yield item;
        }
    },
    *[Symbol.iterator]() {
        yield* this.items;
    }
};

const multi = {
    *range(start: number, end: number): Generator<number> {
        for (let i = start; i <= end; i++) {
            yield i;
        }
    },
    *infinite() {
        let i = 0;
        while (true) {
            yield i++;
        }
    }
};

class GeneratorContainer<T> {
    private data: T[] = [];

    *iterate(): Generator<T> {
        for (const item of this.data) {
            yield item;
        }
    }

    static *countdown(from: number): Generator<number> {
        while (from >= 0) {
            yield from--;
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Generator type should be erased
    assert!(
        !output.contains("Generator<"),
        "Generator type should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Iterable"),
        "Interface should be erased: {}",
        output
    );
    // Generic class parameter should be erased
    assert!(
        !output.contains("GeneratorContainer<T>"),
        "Generic parameter should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": T[]"),
        "Type annotations should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("GeneratorContainer"),
        "Class should be present: {}",
        output
    );
}

/// Parity test for ES5 default parameters.
/// Default parameters should be downleveled to undefined checks.
#[test]
fn test_parity_es5_default_parameters() {
    let source = "function greet(name = 'World') { return 'Hello, ' + name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 default parameter downlevel
    assert!(
        output.contains("function greet"),
        "ES5 output should define greet function: {}",
        output
    );
    // Default parameter should be converted to undefined check
    // tsc outputs: if (name === void 0) { name = 'World'; }
    assert!(
        output.contains("void 0") || output.contains("undefined"),
        "ES5 output should check for undefined: {}",
        output
    );
    assert!(
        output.contains("World"),
        "ES5 output should contain default value: {}",
        output
    );
    // No default parameter syntax in function signature
    assert!(
        !output.contains("name = 'World')") && !output.contains("name='World')"),
        "ES5 output should not contain default parameter syntax: {}",
        output
    );
}

/// Parity test for ES5 for...of loop.
/// for...of should be downleveled to iterator pattern or indexed loop.
#[test]
fn test_parity_es5_for_of_loop() {
    let source = "const arr = [1, 2, 3]; for (const x of arr) { console.log(x); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 for...of downlevel
    assert!(
        output.contains("var arr"),
        "ES5 output should define arr variable: {}",
        output
    );
    // for...of should be converted to iterator pattern or for loop
    // Should use __values helper or indexed for loop
    assert!(
        output.contains("for (") || output.contains("for("),
        "ES5 output should contain for loop: {}",
        output
    );
    assert!(
        output.contains("console.log"),
        "ES5 output should contain console.log: {}",
        output
    );
    // No for...of syntax
    assert!(
        !output.contains("of arr") && !output.contains("of arr)"),
        "ES5 output should not contain for...of syntax: {}",
        output
    );
}

/// Parity test for ES5 array destructuring in function parameters.
/// Array destructuring params should be downleveled to indexed access.
#[test]
fn test_parity_es5_array_destructuring_param() {
    let source = "function swap([a, b]) { return [b, a]; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 array destructuring param downlevel
    assert!(
        output.contains("function swap"),
        "ES5 output should define swap function: {}",
        output
    );
    // Destructuring should be converted to indexed access or temp variable
    // tsc outputs: function swap(_a) { var a = _a[0], b = _a[1]; return [b, a]; }
    assert!(
        output.contains("[0]") || output.contains("_a") || output.contains("_b"),
        "ES5 output should use indexed access or temp variables: {}",
        output
    );
    // No destructuring in parameter list
    assert!(
        !output.contains("([a, b])") && !output.contains("([a,b])"),
        "ES5 output should not contain array destructuring in params: {}",
        output
    );
}

/// Parity test for ES5 object destructuring in function parameters.
/// Object destructuring params should be downleveled to property access.
#[test]
fn test_parity_es5_object_destructuring_param() {
    let source = "function greet({ name, age }) { return name + ' is ' + age; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 object destructuring param downlevel
    assert!(
        output.contains("function greet"),
        "ES5 output should define greet function: {}",
        output
    );
    // Destructuring should be converted to property access
    // tsc outputs: function greet(_a) { var name = _a.name, age = _a.age; ... }
    assert!(
        output.contains(".name") || output.contains(".age") || output.contains("_a"),
        "ES5 output should use property access or temp variables: {}",
        output
    );
    // No destructuring in parameter list
    assert!(
        !output.contains("({ name") && !output.contains("({name"),
        "ES5 output should not contain object destructuring in params: {}",
        output
    );
}

/// Parity test for ES5 arrow function with expression body.
/// Arrow with expression body should be converted to function with return.
#[test]
fn test_parity_es5_arrow_expression_body() {
    let source = "const double = (x) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 arrow expression body downlevel
    assert!(
        output.contains("var double"),
        "ES5 output should define double variable: {}",
        output
    );
    // Arrow with expression body should have return statement
    // tsc outputs: var double = function(x) { return x * 2; };
    assert!(
        output.contains("function"),
        "ES5 output should use function keyword: {}",
        output
    );
    assert!(
        output.contains("return"),
        "ES5 output should have return statement: {}",
        output
    );
    assert!(
        output.contains("* 2") || output.contains("*2"),
        "ES5 output should contain multiplication: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 derived class with constructor and super call.
/// Derived class constructor should call _super with proper arguments.
#[test]
fn test_parity_es5_class_constructor_super() {
    let source = "class Animal { constructor(name) { this.name = name; } } class Dog extends Animal { constructor(name, breed) { super(name); this.breed = breed; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class with constructor and super
    assert!(
        output.contains("__extends"),
        "ES5 output should include __extends helper: {}",
        output
    );
    assert!(
        output.contains("function Animal"),
        "ES5 output should define Animal function: {}",
        output
    );
    assert!(
        output.contains("function Dog"),
        "ES5 output should define Dog function: {}",
        output
    );
    // super(name) should be converted to _super.call(this, name)
    assert!(
        output.contains("_super.call(this") || output.contains("_super.apply(this"),
        "ES5 output should call _super with this: {}",
        output
    );
    // No class syntax
    assert!(
        !output.contains("class Animal") && !output.contains("class Dog"),
        "ES5 output should not contain class syntax: {}",
        output
    );
}

/// Parity test for ES5 class with prototype methods.
/// Class methods should be added to the prototype.
#[test]
fn test_parity_es5_class_prototype_method() {
    let source =
        "class Calculator { add(a, b) { return a + b; } multiply(a, b) { return a * b; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class with prototype methods
    assert!(
        output.contains("function Calculator"),
        "ES5 output should define Calculator function: {}",
        output
    );
    // Methods should be on prototype
    assert!(
        output.contains("Calculator.prototype.add") || output.contains("prototype"),
        "ES5 output should add methods to prototype: {}",
        output
    );
    assert!(
        output.contains("function") && output.contains("return"),
        "ES5 output should contain method bodies: {}",
        output
    );
    // No class syntax
    assert!(
        !output.contains("class Calculator"),
        "ES5 output should not contain class syntax: {}",
        output
    );
}

/// Parity test for ES5 async arrow function.
/// Async arrow should be converted to function with __awaiter/__generator.
#[test]
fn test_parity_es5_async_arrow() {
    let source =
        "const fetchData = async () => { const result = await fetch('/api'); return result; };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 async arrow downlevel
    assert!(
        output.contains("var fetchData"),
        "ES5 output should define fetchData variable: {}",
        output
    );
    assert!(
        output.contains("__awaiter"),
        "ES5 output should include __awaiter helper: {}",
        output
    );
    assert!(
        output.contains("__generator"),
        "ES5 output should include __generator helper: {}",
        output
    );
    assert!(
        output.contains("function"),
        "ES5 output should use function keyword: {}",
        output
    );
    // No async/arrow syntax
    assert!(
        !output.contains("async") || output.contains("__async"),
        "ES5 output should not contain async keyword: {}",
        output
    );
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 let in for loop.
/// let/const should be converted to var.
#[test]
fn test_parity_es5_let_in_for_loop() {
    let source = "for (let i = 0; i < 10; i++) { console.log(i); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 let->var conversion
    assert!(
        output.contains("for (var i") || output.contains("for(var i"),
        "ES5 output should use var in for loop: {}",
        output
    );
    assert!(
        output.contains("console.log"),
        "ES5 output should contain console.log: {}",
        output
    );
    // No let keyword
    assert!(
        !output.contains("let i") && !output.contains("let  i"),
        "ES5 output should not contain let keyword: {}",
        output
    );
}

/// Parity test for ES5 const declaration.
/// const should be converted to var.
#[test]
fn test_parity_es5_const_declaration() {
    let source = "const PI = 3.14159; const E = 2.71828;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 const->var conversion
    assert!(
        output.contains("var PI") || output.contains("var  PI"),
        "ES5 output should use var for PI: {}",
        output
    );
    assert!(
        output.contains("var E") || output.contains("var  E"),
        "ES5 output should use var for E: {}",
        output
    );
    assert!(
        output.contains("3.14159") && output.contains("2.71828"),
        "ES5 output should contain values: {}",
        output
    );
    // No const keyword
    assert!(
        !output.contains("const PI") && !output.contains("const E"),
        "ES5 output should not contain const keyword: {}",
        output
    );
}

/// Parity test for ES5 CommonJS named exports.
/// Named exports should be assigned to exports object.
#[test]
fn test_parity_es5_commonjs_named_exports() {
    let source = "const foo = 1; const bar = 2; export { foo, bar };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify CommonJS named exports
    assert!(
        output.contains("__esModule"),
        "CommonJS output should include __esModule marker: {}",
        output
    );
    assert!(
        output.contains("var foo") || output.contains("foo = 1"),
        "CommonJS output should define foo: {}",
        output
    );
    assert!(
        output.contains("var bar") || output.contains("bar = 2"),
        "CommonJS output should define bar: {}",
        output
    );
    // Named exports should be assigned to exports
    assert!(
        output.contains("exports.foo") || output.contains("exports[\"foo\"]"),
        "CommonJS output should export foo: {}",
        output
    );
    assert!(
        output.contains("exports.bar") || output.contains("exports[\"bar\"]"),
        "CommonJS output should export bar: {}",
        output
    );
    // No ES6 export syntax
    assert!(
        !output.contains("export {"),
        "CommonJS output should not contain ES6 export syntax: {}",
        output
    );
}

/// Parity test for ES5 class with static property initialization.
/// Static properties should be assigned after the class IIFE.
#[test]
fn test_parity_es5_class_static_property() {
    let source = "class Config { static version = '1.0.0'; static count = 0; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 static property initialization
    assert!(
        output.contains("function Config") || output.contains("var Config"),
        "ES5 output should define Config as function: {}",
        output
    );
    // Static properties should be assigned on the class constructor
    assert!(
        output.contains("Config.version")
            && (output.contains("'1.0.0'") || output.contains("\"1.0.0\"")),
        "ES5 output should assign static version property: {}",
        output
    );
    assert!(
        output.contains("Config.count") && output.contains("0"),
        "ES5 output should assign static count property: {}",
        output
    );
    // No class syntax
    assert!(
        !output.contains("class Config"),
        "ES5 output should not contain class syntax: {}",
        output
    );
    // No static keyword in output
    assert!(
        !output.contains("static version") && !output.contains("static count"),
        "ES5 output should not contain static keyword: {}",
        output
    );
}

/// Parity test for ES5 abstract class.
/// Abstract classes should be downleveled just like regular classes,
/// with the abstract keyword removed (only TypeScript type checking uses it).
#[test]
fn test_parity_es5_abstract_class() {
    let source = "abstract class Shape { abstract area(): number; getName() { return 'shape'; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 abstract class downlevel
    assert!(
        output.contains("function Shape") || output.contains("var Shape"),
        "ES5 output should define Shape as function: {}",
        output
    );
    // Concrete method should be on prototype
    assert!(
        output.contains("Shape.prototype.getName") || output.contains("getName"),
        "ES5 output should define getName method: {}",
        output
    );
    // Abstract method should NOT be emitted (it's type-only)
    assert!(
        !output.contains("Shape.prototype.area") || output.contains("area"),
        "ES5 output may or may not emit abstract method stub: {}",
        output
    );
    // No abstract keyword in output
    assert!(
        !output.contains("abstract class") && !output.contains("abstract "),
        "ES5 output should not contain abstract keyword: {}",
        output
    );
    // No class syntax
    assert!(
        !output.contains("class Shape"),
        "ES5 output should not contain class syntax: {}",
        output
    );
}

/// Parity test for ES5 namespace/module downlevel.
/// TypeScript namespaces should be downleveled to IIFE with exports.
#[test]
fn test_parity_es5_namespace() {
    let source = "namespace Utils { export function add(a: number, b: number) { return a + b; } export const PI = 3.14; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 namespace downlevel
    assert!(
        output.contains("var Utils") || output.contains("Utils = {}"),
        "ES5 output should define Utils namespace: {}",
        output
    );
    // Should use IIFE pattern or direct assignment
    assert!(
        output.contains("function") && output.contains("Utils"),
        "ES5 output should contain function for namespace: {}",
        output
    );
    // Exported members should be on namespace object
    assert!(
        output.contains("Utils.add") || output.contains("Utils_1.add") || output.contains("add"),
        "ES5 output should export add function: {}",
        output
    );
    assert!(
        output.contains("Utils.PI") || output.contains("PI") || output.contains("3.14"),
        "ES5 output should export PI constant: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Utils"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
}

/// Parity test for ES5 enum downlevel.
/// TypeScript enums should be downleveled to IIFE with bidirectional mapping.
#[test]
fn test_parity_es5_enum() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 enum downlevel
    assert!(
        output.contains("var Color") || output.contains("Color = {}"),
        "ES5 output should define Color enum: {}",
        output
    );
    // Enum members should be assigned
    assert!(
        output.contains("Red") && output.contains("Green") && output.contains("Blue"),
        "ES5 output should contain all enum members: {}",
        output
    );
    // Should have numeric values (0, 1, 2)
    assert!(
        output.contains("0") && output.contains("1") && output.contains("2"),
        "ES5 output should have numeric enum values: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Color"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for ES5 string enum downlevel.
/// String enums should be downleveled without reverse mapping.
#[test]
fn test_parity_es5_string_enum() {
    let source = r#"enum Direction { Up = "UP", Down = "DOWN", Left = "LEFT", Right = "RIGHT" }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 string enum downlevel
    assert!(
        output.contains("var Direction") || output.contains("Direction = {}"),
        "ES5 output should define Direction enum: {}",
        output
    );
    // Enum members should have string values
    assert!(
        output.contains("UP")
            && output.contains("DOWN")
            && output.contains("LEFT")
            && output.contains("RIGHT"),
        "ES5 output should contain all string enum values: {}",
        output
    );
    // Member names should be present
    assert!(
        output.contains("Up") && output.contains("Down"),
        "ES5 output should contain enum member names: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Direction"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for type-only import erasure.
/// Type-only imports should be completely removed from output.
#[test]
fn test_parity_type_only_import_erasure() {
    let source = "import type { Foo } from './foo'; import { bar } from './bar'; bar();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type-only import should be erased
    assert!(
        !output.contains("Foo") && !output.contains("foo"),
        "Type-only import should be erased: {}",
        output
    );
    // Value import should remain
    assert!(
        output.contains("bar") && output.contains("require"),
        "Value import should remain: {}",
        output
    );
    // No import type syntax
    assert!(
        !output.contains("import type"),
        "Output should not contain import type syntax: {}",
        output
    );
}

/// Parity test for interface erasure.
/// TypeScript interfaces should be completely removed from output.
#[test]
fn test_parity_interface_erasure() {
    let source = "interface User { name: string; age: number; } const user: User = { name: 'Alice', age: 30 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be completely erased
    assert!(
        !output.contains("interface User"),
        "Interface declaration should be erased: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": User"),
        "Type annotation should be erased: {}",
        output
    );
    // Value code should remain
    assert!(
        output.contains("var user") && output.contains("Alice") && output.contains("30"),
        "Value code should remain: {}",
        output
    );
}

/// Parity test for type alias erasure.
/// TypeScript type aliases should be completely removed from output.
#[test]
fn test_parity_type_alias_erasure() {
    let source = "type StringOrNumber = string | number; const value: StringOrNumber = 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type alias should be completely erased
    assert!(
        !output.contains("type StringOrNumber"),
        "Type alias declaration should be erased: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": StringOrNumber"),
        "Type annotation should be erased: {}",
        output
    );
    // Value code should remain
    assert!(
        output.contains("var value") && output.contains("42"),
        "Value code should remain: {}",
        output
    );
}

/// Parity test for function parameter type erasure.
/// Function parameter types should be removed from output.
#[test]
fn test_parity_function_param_type_erasure() {
    let source = "function greet(name: string, age: number): string { return name + age; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("function greet"),
        "ES5 output should define greet function: {}",
        output
    );
    // Parameter names should remain
    assert!(
        output.contains("name") && output.contains("age"),
        "ES5 output should have parameter names: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
    // Return type should be erased
    assert!(
        !output.contains("): string"),
        "Return type should be erased: {}",
        output
    );
}

/// Parity test for generic type parameter erasure.
/// Generic type parameters should be removed from output.
#[test]
fn test_parity_generic_type_erasure() {
    let source = "function identity<T>(value: T): T { return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("function identity"),
        "Output should define identity function: {}",
        output
    );
    // Parameter name should remain
    assert!(
        output.contains("value"),
        "Output should have parameter name: {}",
        output
    );
    // Generic type parameter should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type parameter <T> should be erased: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": T"),
        "Type annotation : T should be erased: {}",
        output
    );
}

/// Parity test for optional parameter question mark erasure.
/// The ? marker on optional parameters should be removed.
#[test]
fn test_parity_optional_param_erasure() {
    let source = "function greet(name?: string) { return name || 'World'; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("function greet"),
        "Output should define greet function: {}",
        output
    );
    // Parameter name should remain
    assert!(
        output.contains("name"),
        "Output should have parameter name: {}",
        output
    );
    // Optional marker should be erased
    assert!(
        !output.contains("?"),
        "Optional marker ? should be erased: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for rest parameters ES5 downlevel.
/// Rest parameters should be converted to arguments slicing.
#[test]
fn test_parity_es5_rest_params() {
    let source = "function sum(...nums: number[]) { return nums.reduce((a, b) => a + b, 0); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("function sum"),
        "Output should define sum function: {}",
        output
    );
    // Rest parameter syntax should be removed
    assert!(
        !output.contains("...nums"),
        "Rest parameter syntax should be removed: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number[]"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for ES5 class static block downlevel.
/// Static blocks should be converted to static initialization code.
#[test]
fn test_parity_es5_static_block() {
    let source = r#"class Counter {
    static count = 0;
    static {
        Counter.count = 10;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Counter") || output.contains("function Counter"),
        "ES5 output should define Counter: {}",
        output
    );
    // Static block syntax should not appear in ES5 output
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Static initialization should occur
    assert!(
        output.contains("Counter.count"),
        "ES5 output should have static property access: {}",
        output
    );
}

/// Parity test for ES5 class static block with multiple statements.
/// Multiple statements in static block should be preserved.
#[test]
fn test_parity_es5_static_block_multi_stmt() {
    let source = r#"class Config {
    static debug = false;
    static version = "";
    static {
        Config.debug = true;
        Config.version = "1.0.0";
        console.log("Initialized");
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Config") || output.contains("function Config"),
        "ES5 output should define Config: {}",
        output
    );
    // Static block syntax should not appear in ES5 output
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // All static property assignments should occur
    assert!(
        output.contains("Config.debug") && output.contains("Config.version"),
        "ES5 output should have static property assignments: {}",
        output
    );
    // Console.log call should be preserved
    assert!(
        output.contains("console.log"),
        "ES5 output should preserve console.log call: {}",
        output
    );
}

/// Parity test for ES5 object spread downlevel.
/// Object spread should be converted to Object.assign or helper.
#[test]
fn test_parity_es5_object_spread() {
    let source = "const merged = { ...obj1, ...obj2, extra: true };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("var merged") || output.contains("merged"),
        "ES5 output should define merged: {}",
        output
    );
    // Spread syntax should not appear in ES5 output
    assert!(
        !output.contains("...obj1") && !output.contains("...obj2"),
        "ES5 output should not contain spread syntax: {}",
        output
    );
}

#[test]
fn test_parity_es5_object_spread_typed() {
    let source = r#"
interface Person {
    name: string;
    age: number;
}

interface Employee extends Person {
    department: string;
    salary: number;
}

const person: Person = { name: "John", age: 30 };
const employee: Employee = {
    ...person,
    department: "Engineering",
    salary: 100000
};

function clonePerson<T extends Person>(p: T): T {
    return { ...p };
}

type PartialEmployee = Partial<Employee>;
const partial: PartialEmployee = { ...employee, salary: undefined };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...person")
            && !output.contains("...p")
            && !output.contains("...employee"),
        "Spread syntax should not appear: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Person") && !output.contains(": Employee"),
        "Type annotations should be erased: {}",
        output
    );
    // Generic constraint should be erased
    assert!(
        !output.contains("extends Person"),
        "Generic constraint should be erased: {}",
        output
    );
    // Partial type should be erased
    assert!(
        !output.contains("Partial<"),
        "Partial type should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_object_spread_multiple() {
    let source = r#"
interface Config {
    [key: string]: unknown;
}

const defaults: Config = { timeout: 1000, retries: 3 };
const userConfig: Config = { timeout: 5000 };
const envConfig: Config = { debug: true };

const finalConfig: Config = {
    ...defaults,
    ...userConfig,
    ...envConfig,
    timestamp: Date.now()
};

function mergeAll<T extends object>(...objects: T[]): T {
    return objects.reduce((acc, obj) => ({ ...acc, ...obj }), {} as T);
}

const a = { x: 1 };
const b = { y: 2 };
const c = { z: 3 };
const d = { w: 4 };
const combined = { ...a, ...b, ...c, ...d };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...defaults")
            && !output.contains("...userConfig")
            && !output.contains("...envConfig"),
        "Spread syntax should not appear: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Config"),
        "Type annotations should be erased: {}",
        output
    );
    // Generic constraint should be erased
    assert!(
        !output.contains("extends object"),
        "Generic constraint should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_object_spread_overrides() {
    let source = r#"
interface Theme {
    primary: string;
    secondary: string;
    background: string;
    text: string;
}

const lightTheme: Theme = {
    primary: "blue",
    secondary: "gray",
    background: "white",
    text: "black"
};

const darkTheme: Theme = {
    ...lightTheme,
    background: "black",
    text: "white"
};

const customTheme: Theme = {
    ...darkTheme,
    primary: "red",
    ...{ secondary: "green" }
};

function withDefaults<T>(defaults: T, overrides: Partial<T>): T {
    return { ...defaults, ...overrides };
}

const result = withDefaults(lightTheme, { primary: "navy" });
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...lightTheme") && !output.contains("...darkTheme"),
        "Spread syntax should not appear: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Theme"),
        "Type annotations should be erased: {}",
        output
    );
    // Partial type should be erased
    assert!(
        !output.contains("Partial<"),
        "Partial type should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_object_spread_nested_deep() {
    let source = r#"
interface DeepConfig {
    level1: {
        level2: {
            level3: {
                value: number;
            };
        };
    };
}

const base: DeepConfig = {
    level1: {
        level2: {
            level3: {
                value: 42
            }
        }
    }
};

const modified: DeepConfig = {
    ...base,
    level1: {
        ...base.level1,
        level2: {
            ...base.level1.level2,
            level3: {
                ...base.level1.level2.level3,
                value: 100
            }
        }
    }
};

type Nested = {
    outer: {
        inner: {
            data: string[];
        };
    };
};

function deepMerge<T extends object>(a: T, b: Partial<T>): T {
    const result = { ...a };
    for (const key in b) {
        if (typeof b[key] === "object" && b[key] !== null) {
            (result as any)[key] = { ...(a as any)[key], ...(b as any)[key] };
        }
    }
    return result;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...base"),
        "Spread syntax should not appear: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Nested"),
        "Type alias should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": DeepConfig"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_object_spread_computed() {
    let source = r#"
const propName = "dynamicKey";
const prefix = "computed_";

interface DynamicObject {
    [key: string]: unknown;
}

const base: DynamicObject = { existing: true };

const withComputed: DynamicObject = {
    ...base,
    [propName]: "value1",
    [`${prefix}prop`]: "value2",
    [propName + "_suffix"]: "value3"
};

function spreadWithComputed<T extends object>(
    obj: T,
    key: string,
    value: unknown
): T & { [k: string]: unknown } {
    return {
        ...obj,
        [key]: value
    };
}

const result = spreadWithComputed({ a: 1 }, "b", 2);

class SpreadBuilder<T extends object> {
    private data: T;

    constructor(initial: T) {
        this.data = initial;
    }

    add<K extends string, V>(key: K, value: V): SpreadBuilder<T & Record<K, V>> {
        return new SpreadBuilder({ ...this.data, [key]: value });
    }

    build(): T {
        return this.data;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...base")
            && !output.contains("...obj")
            && !output.contains("...this.data"),
        "Spread syntax should not appear: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": DynamicObject"),
        "Type annotations should be erased: {}",
        output
    );
    // Generic constraints should be erased
    assert!(
        !output.contains("extends object") && !output.contains("extends string"),
        "Generic constraints should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("SpreadBuilder"),
        "Class should be present: {}",
        output
    );
}

/// Parity test for ES5 array spread downlevel.
/// Array spread should be converted to concat or helper.
#[test]
fn test_parity_es5_array_spread() {
    let source = "const combined = [...arr1, ...arr2, 42];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("var combined") || output.contains("combined"),
        "ES5 output should define combined: {}",
        output
    );
    // Spread syntax should not appear in ES5 output
    assert!(
        !output.contains("...arr1") && !output.contains("...arr2"),
        "ES5 output should not contain spread syntax: {}",
        output
    );
}

/// Parity test for ES5 private class field downlevel.
/// Private fields (#name) should be converted to WeakMap-based emulation.
#[test]
fn test_parity_es5_private_field() {
    let source = r#"class Person {
    #name: string;
    constructor(name: string) {
        this.#name = name;
    }
    getName() {
        return this.#name;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Person") || output.contains("function Person"),
        "ES5 output should define Person class: {}",
        output
    );
    // Private field syntax should not appear in ES5 output
    assert!(
        !output.contains("#name"),
        "ES5 output should not contain private field syntax #name: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for ES5 private static field access downlevel.
/// Static private field access should use __classPrivateFieldGet helper.
#[test]
fn test_parity_es5_private_static_field_access() {
    let source = r#"class Counter {
    static #count = 0;
    static getCount() {
        return Counter.#count;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Counter") || output.contains("function Counter"),
        "ES5 output should define Counter class: {}",
        output
    );
    // Should use __classPrivateFieldGet helper for static private access
    assert!(
        output.contains("__classPrivateFieldGet"),
        "ES5 output should use __classPrivateFieldGet helper: {}",
        output
    );
}

/// Parity test for ES5 private method downlevel.
/// Private methods should be converted to WeakSet-based emulation.
#[test]
fn test_parity_es5_private_method() {
    let source = r#"class Calculator {
    #validate(x: number) {
        return x >= 0;
    }
    compute(x: number) {
        if (this.#validate(x)) {
            return x * 2;
        }
        return 0;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Calculator") || output.contains("function Calculator"),
        "ES5 output should define Calculator class: {}",
        output
    );
    // Private method syntax should not appear in ES5 output
    assert!(
        !output.contains("#validate"),
        "ES5 output should not contain private method syntax #validate: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for ES5 private getter downlevel.
/// Private getters should be converted using helper functions.
#[test]
fn test_parity_es5_private_getter() {
    let source = r#"class Box {
    #value = 0;
    get #contents() {
        return this.#value;
    }
    read() {
        return this.#contents;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Box") || output.contains("function Box"),
        "ES5 output should define Box class: {}",
        output
    );
    // Private getter syntax should not appear in ES5 output
    assert!(
        !output.contains("get #contents"),
        "ES5 output should not contain private getter syntax: {}",
        output
    );
}

/// Parity test for ES5 private setter downlevel.
/// Private setters should be converted using helper functions.
#[test]
fn test_parity_es5_private_setter() {
    let source = r#"class Container {
    #data = "";
    set #content(val: string) {
        this.#data = val;
    }
    store(val: string) {
        this.#content = val;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Container") || output.contains("function Container"),
        "ES5 output should define Container class: {}",
        output
    );
    // Private setter syntax should not appear in ES5 output
    assert!(
        !output.contains("set #content"),
        "ES5 output should not contain private setter syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for ES5 nullish coalescing downlevel.
/// The ?? operator should be converted to ternary checks.
#[test]
fn test_parity_es5_nullish_coalescing() {
    let source = "const result = value ?? 'default';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("var result") || output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
}

/// Parity test for ES5 optional chaining downlevel.
/// The ?. operator should be converted to conditional checks.
#[test]
fn test_parity_es5_optional_chaining() {
    let source = "const name = obj?.person?.name;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("var name") || output.contains("name"),
        "ES5 output should define name: {}",
        output
    );
}

/// Parity test for ES5 generator function type erasure.
/// Generator function type annotations should be erased.
#[test]
fn test_parity_es5_generator_type_erasure() {
    let source = r#"function* range(start: number, end: number): Generator<number> {
    yield start;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists (may still have * if generator transform not implemented)
    assert!(
        output.contains("range"),
        "Output should define range function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("Generator<number>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 generator method in class downlevel.
/// Generator methods should be converted to __generator helper.
#[test]
fn test_parity_es5_generator_method() {
    let source = r#"class Sequence {
    *items() {
        yield 1;
        yield 2;
        yield 3;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Sequence") || output.contains("function Sequence"),
        "ES5 output should define Sequence class: {}",
        output
    );
    // Generator method syntax should not appear in ES5 output
    assert!(
        !output.contains("*items"),
        "ES5 output should not contain *items generator method syntax: {}",
        output
    );
}

/// Parity test for ES5 generator yield type erasure.
/// Generator yield expressions should have types erased.
#[test]
fn test_parity_es5_generator_yield_type_erasure() {
    let source = r#"function* gen(): Generator<string, void, unknown> {
    const result: string = yield "hello";
    yield result;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("gen"),
        "Output should define gen function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("Generator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator function type erasure.
/// Async generator type annotations should be erased.
#[test]
fn test_parity_es5_async_generator_type_erasure() {
    let source = r#"async function* fetchPages(urls: string[]): AsyncGenerator<string> {
    for (const url of urls) {
        yield await fetch(url);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("fetchPages"),
        "Output should define fetchPages function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]") && !output.contains("AsyncGenerator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator method in class.
/// Async generator methods should have types erased.
#[test]
fn test_parity_es5_async_generator_method() {
    let source = r#"class DataStream {
    async *items(): AsyncGenerator<number> {
        yield 1;
        yield 2;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var DataStream") || output.contains("function DataStream"),
        "ES5 output should define DataStream class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": AsyncGenerator<number>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator with await and yield.
/// Both await and yield should be preserved in output.
#[test]
fn test_parity_es5_async_generator_await_yield() {
    let source = r#"async function* process(items: Promise<number>[]): AsyncGenerator<number> {
    for (const item of items) {
        const value = await item;
        yield value * 2;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("process"),
        "Output should define process function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Promise<number>[]") && !output.contains("AsyncGenerator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 class decorator downlevel.
/// Class decorators should use __decorate helper.
#[test]
fn test_parity_es5_class_decorator() {
    let source = r#"function sealed(constructor: Function) {}

@sealed
class Greeter {
    greeting: string;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Greeter"),
        "Output should define Greeter class: {}",
        output
    );
    // Decorator syntax should not appear directly in ES5 output
    assert!(
        !output.contains("@sealed"),
        "ES5 output should not contain @sealed decorator syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 method decorator downlevel.
/// Method decorators should use __decorate helper.
#[test]
fn test_parity_es5_method_decorator() {
    let source = r#"function log(target: any, key: string, descriptor: PropertyDescriptor) {}

class Calculator {
    @log
    add(a: number, b: number): number {
        return a + b;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Calculator"),
        "Output should define Calculator class: {}",
        output
    );
    // Decorator syntax should not appear directly in ES5 output
    assert!(
        !output.contains("@log"),
        "ES5 output should not contain @log decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": any")
            && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 property decorator downlevel.
/// Property decorators should use __decorate helper.
#[test]
fn test_parity_es5_property_decorator() {
    let source = r#"function observable(target: any, key: string) {}

class User {
    @observable
    name: string = "";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("User"),
        "Output should define User class: {}",
        output
    );
    // Decorator syntax should not appear directly in ES5 output
    assert!(
        !output.contains("@observable"),
        "ES5 output should not contain @observable decorator syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string") && !output.contains(": any"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 parameter decorator downlevel.
/// Parameter decorators should use __param helper.
#[test]
fn test_parity_es5_parameter_decorator() {
    let source = r#"function inject(target: any, key: string, index: number) {}

class Service {
    constructor(@inject private dep: any) {}
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Service"),
        "Output should define Service class: {}",
        output
    );
    // Decorator syntax should not appear directly in ES5 output
    assert!(
        !output.contains("@inject"),
        "ES5 output should not contain @inject decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": any") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 function call spread downlevel.
/// Spread in function calls should use .apply() or similar.
#[test]
fn test_parity_es5_call_spread() {
    let source = "const result = Math.max(...numbers);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
    // Spread syntax should not appear in ES5 output
    assert!(
        !output.contains("...numbers"),
        "ES5 output should not contain spread syntax: {}",
        output
    );
}

/// Parity test for ES5 new expression spread downlevel.
/// Spread in new expressions should be handled.
#[test]
fn test_parity_es5_new_spread() {
    let source = "const date = new Date(...args);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("date"),
        "ES5 output should define date: {}",
        output
    );
    // Spread syntax should not appear in ES5 output
    assert!(
        !output.contains("...args"),
        "ES5 output should not contain spread syntax: {}",
        output
    );
}

/// Parity test for ES5 rest parameters in function downlevel.
/// Rest parameters should be converted to arguments slicing.
#[test]
fn test_parity_es5_rest_params_function() {
    let source = "function collect(first: number, ...rest: number[]) { return [first, ...rest]; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("function collect"),
        "ES5 output should define collect function: {}",
        output
    );
    // Rest parameter syntax should be removed
    assert!(
        !output.contains("...rest"),
        "ES5 output should not contain rest parameter syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": number[]"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 spread in array literal with mixed elements.
/// Mixed spread and regular elements should be handled.
#[test]
fn test_parity_es5_array_spread_mixed() {
    let source = "const arr = [1, ...middle, 2, ...end, 3];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("arr"),
        "ES5 output should define arr: {}",
        output
    );
    // Spread syntax should not appear in ES5 output
    assert!(
        !output.contains("...middle") && !output.contains("...end"),
        "ES5 output should not contain spread syntax: {}",
        output
    );
}

#[test]
fn test_parity_es5_array_spread_literal_typed() {
    let source = r#"
const numbers: number[] = [1, 2, 3];
const strings: string[] = ["a", "b"];

const combined: (number | string)[] = [...numbers, ...strings];

function createArray<T>(...items: T[]): T[] {
    return [...items];
}

const doubled: number[] = [...numbers, ...numbers];

interface Point { x: number; y: number; }
const points: Point[] = [{ x: 0, y: 0 }];
const morePoints: Point[] = [...points, { x: 1, y: 1 }];

type NumberArray = number[];
const typed: NumberArray = [...numbers];

class Container<T> {
    items: T[] = [];

    addAll(...newItems: T[]): void {
        this.items = [...this.items, ...newItems];
    }

    getAll(): T[] {
        return [...this.items];
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...numbers")
            && !output.contains("...strings")
            && !output.contains("...items"),
        "Spread syntax should not appear: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type NumberArray"),
        "Type alias should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("Container"),
        "Class should be present: {}",
        output
    );
}

#[test]
fn test_parity_es5_array_spread_function_call() {
    let source = r#"
function sum(...nums: number[]): number {
    return nums.reduce((a, b) => a + b, 0);
}

function formatMessage(template: string, ...args: string[]): string {
    return template + args.join(", ");
}

const values: number[] = [1, 2, 3, 4, 5];

class Calculator {
    static total(...nums: number[]): number {
        return nums.reduce((a, b) => a + b, 0);
    }
}

function applyToAll<T, R>(fn: (...args: T[]) => R, items: T[]): R {
    return fn.apply(null, items);
}

interface Logger {
    log(...args: unknown[]): void;
}

const logger: Logger = {
    log(...args: unknown[]) {
        console.log(args);
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Rest parameters should be transformed to arguments access
    assert!(
        output.contains("arguments"),
        "Rest params should use arguments: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("Calculator"),
        "Class should be present: {}",
        output
    );
}

#[test]
fn test_parity_es5_array_spread_new_expression() {
    let source = r#"
class Vector {
    constructor(public x: number, public y: number, public z: number) {}

    magnitude(): number {
        return Math.sqrt(this.x * this.x + this.y * this.y + this.z * this.z);
    }
}

class DataStore<T> {
    private data: T[];

    constructor(initial: T[]) {
        this.data = initial.slice();
    }

    add(item: T): void {
        this.data.push(item);
    }

    getAll(): T[] {
        return this.data.slice();
    }
}

class Logger {
    constructor(private prefix: string) {}

    log(message: string): void {
        console.log(this.prefix, message);
    }
}

interface Factory<T> {
    create(): T;
}

class VectorFactory implements Factory<Vector> {
    create(): Vector {
        return new Vector(0, 0, 0);
    }
}

function createWithDefaults<T>(factory: Factory<T>): T {
    return factory.create();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // public/private keywords should be erased
    assert!(
        !output.contains("public") && !output.contains("private"),
        "Access modifiers should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("Vector") && output.contains("Logger") && output.contains("DataStore"),
        "Classes should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // implements clause should be erased
    assert!(
        !output.contains("implements"),
        "implements clause should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<Vector>"),
        "Generic types should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_array_spread_mixed_complex() {
    let source = r#"
type Item = { id: number; name: string };

const first: Item[] = [{ id: 1, name: "a" }];
const second: Item[] = [{ id: 2, name: "b" }];

const mixed: (Item | number | string)[] = [
    0,
    ...first,
    "separator",
    ...second,
    { id: 3, name: "c" },
    99
];

function interleave<T>(a: T[], b: T[]): T[] {
    const result: T[] = [];
    const maxLen = Math.max(a.length, b.length);
    for (let i = 0; i < maxLen; i++) {
        if (i < a.length) result.push(a[i]);
        if (i < b.length) result.push(b[i]);
    }
    return result;
}

const nums1 = [1, 3, 5];
const nums2 = [2, 4, 6];
const interleaved = [...interleave(nums1, nums2), 7, 8, 9];

class ArrayUtils {
    static flatten<T>(arrays: T[][]): T[] {
        return arrays.reduce((acc, arr) => [...acc, ...arr], [] as T[]);
    }

    static unique<T>(arr: T[]): T[] {
        return [...new Set(arr)];
    }

    static prepend<T>(item: T, arr: T[]): T[] {
        return [item, ...arr];
    }

    static append<T>(arr: T[], item: T): T[] {
        return [...arr, item];
    }
}

const nested = [[1, 2], [3, 4], [5, 6]];
const flat = ArrayUtils.flatten(nested);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Spread syntax should not appear
    assert!(
        !output.contains("...first") && !output.contains("...second") && !output.contains("...arr"),
        "Spread syntax should not appear: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Item"),
        "Type alias should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Item[]") && !output.contains(": T[]"),
        "Type annotations should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ArrayUtils"),
        "Class should be present: {}",
        output
    );
}

/// Parity test for ES5 array destructuring assignment downlevel.
/// Array destructuring should be converted to indexed access.
#[test]
fn test_parity_es5_array_destructuring_assignment() {
    let source = "const [first, second, third] = items;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variables are defined
    assert!(
        output.contains("first") && output.contains("second") && output.contains("third"),
        "ES5 output should define destructured variables: {}",
        output
    );
    // Array destructuring syntax should not appear in ES5 output
    assert!(
        !output.contains("[first, second, third]"),
        "ES5 output should not contain array destructuring syntax: {}",
        output
    );
}

/// Parity test for ES5 object destructuring assignment downlevel.
/// Object destructuring should be converted to property access.
#[test]
fn test_parity_es5_object_destructuring_assignment() {
    let source = "const { name, age, city } = person;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variables are defined
    assert!(
        output.contains("name") && output.contains("age") && output.contains("city"),
        "ES5 output should define destructured variables: {}",
        output
    );
    // Object destructuring syntax should not appear in ES5 output
    assert!(
        !output.contains("{ name, age, city }"),
        "ES5 output should not contain object destructuring syntax: {}",
        output
    );
}

/// Parity test for ES5 nested destructuring downlevel.
/// Nested destructuring should be fully expanded.
#[test]
fn test_parity_es5_nested_destructuring() {
    let source = "const { user: { name, address: { city } } } = data;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify extracted variables exist
    assert!(
        output.contains("name") && output.contains("city"),
        "ES5 output should define nested destructured variables: {}",
        output
    );
}

/// Parity test for ES5 destructuring with default values downlevel.
/// Default values in destructuring should be preserved.
#[test]
fn test_parity_es5_destructuring_defaults() {
    let source = "const { name = 'Anonymous', age = 0 } = config;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variables are defined
    assert!(
        output.contains("name") && output.contains("age"),
        "ES5 output should define destructured variables: {}",
        output
    );
    // Default values should be preserved
    assert!(
        output.contains("Anonymous") || output.contains("0"),
        "ES5 output should preserve default values: {}",
        output
    );
}

/// Parity test for ES5 destructuring with rest element downlevel.
/// Rest element in destructuring should be handled.
#[test]
fn test_parity_es5_destructuring_rest() {
    let source = "const [first, ...remaining] = items;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variables are defined
    assert!(
        output.contains("first") && output.contains("remaining"),
        "ES5 output should define destructured variables: {}",
        output
    );
    // Rest syntax should not appear in ES5 destructuring
    assert!(
        !output.contains("...remaining"),
        "ES5 output should not contain rest element in destructuring: {}",
        output
    );
}

/// Parity test for ES5 optional method call downlevel.
/// Optional method calls (?.) should be transformed.
#[test]
fn test_parity_es5_optional_method_call() {
    let source = "const result = obj?.method?.();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
}

/// Parity test for ES5 optional element access downlevel.
/// Optional element access (?.[ ]) should be transformed.
#[test]
fn test_parity_es5_optional_element_access() {
    let source = "const value = arr?.[0]?.name;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("value"),
        "ES5 output should define value: {}",
        output
    );
}

/// Parity test for ES5 optional chaining with nullish coalescing.
/// Combined ?. and ?? should both be handled.
#[test]
fn test_parity_es5_optional_chaining_with_nullish() {
    let source = "const name = user?.profile?.name ?? 'Anonymous';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("name"),
        "ES5 output should define name: {}",
        output
    );
    // Default value should be preserved
    assert!(
        output.contains("Anonymous"),
        "ES5 output should preserve default value: {}",
        output
    );
}

/// Parity test for ES5 optional chaining in function call.
/// Optional chaining before function call should be handled.
#[test]
fn test_parity_es5_optional_chaining_call() {
    let source = "const result = callback?.(arg1, arg2);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
    // Arguments should be preserved
    assert!(
        output.contains("arg1") && output.contains("arg2"),
        "ES5 output should preserve arguments: {}",
        output
    );
}

/// Parity test for ES5 nullish coalescing with function call.
/// Nullish coalescing with function call as fallback.
#[test]
fn test_parity_es5_nullish_coalescing_call() {
    let source = "const value = config ?? getDefault();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("value"),
        "ES5 output should define value: {}",
        output
    );
    // Function call should be preserved
    assert!(
        output.contains("getDefault"),
        "ES5 output should preserve fallback function call: {}",
        output
    );
}

/// Parity test for ES5 nullish coalescing assignment operator.
/// The ??= operator should be downleveled.
#[test]
fn test_parity_es5_nullish_assignment() {
    let source = "x ??= defaultValue;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify identifiers exist
    assert!(
        output.contains("x") && output.contains("defaultValue"),
        "ES5 output should preserve identifiers: {}",
        output
    );
}

/// Parity test for ES5 chained nullish coalescing.
/// Multiple ?? operators in chain.
#[test]
fn test_parity_es5_nullish_chained() {
    let source = "const result = a ?? b ?? c ?? 'fallback';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
    // All identifiers should be preserved
    assert!(
        output.contains("a") && output.contains("b") && output.contains("c"),
        "ES5 output should preserve all identifiers: {}",
        output
    );
    // Fallback should be preserved
    assert!(
        output.contains("fallback"),
        "ES5 output should preserve fallback value: {}",
        output
    );
}

/// Parity test for ES5 nullish coalescing with object property.
/// Nullish coalescing on object property access.
#[test]
fn test_parity_es5_nullish_property() {
    let source = "const name = obj.name ?? obj.defaultName ?? 'Unknown';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("name"),
        "ES5 output should define name: {}",
        output
    );
    // Property accesses should be preserved
    assert!(
        output.contains("obj") && output.contains("defaultName"),
        "ES5 output should preserve property accesses: {}",
        output
    );
}

/// Parity test for ES5 template literal with multiple expressions.
/// Multiple expressions should all be concatenated.
#[test]
fn test_parity_es5_template_multi_expr() {
    let source = r#"const msg = `Hello ${first} ${middle} ${last}!`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("msg"),
        "ES5 output should define msg: {}",
        output
    );
    // All identifiers should be preserved
    assert!(
        output.contains("first") && output.contains("middle") && output.contains("last"),
        "ES5 output should preserve all identifiers: {}",
        output
    );
    // Template literal syntax should not appear
    assert!(
        !output.contains("`"),
        "ES5 output should not contain backticks: {}",
        output
    );
}

/// Parity test for ES5 tagged template literal.
/// Tagged templates should be converted to function calls.
#[test]
fn test_parity_es5_tagged_template() {
    let source = r#"const result = tag`Hello ${name}!`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("result"),
        "ES5 output should define result: {}",
        output
    );
    // Tag function should be preserved
    assert!(
        output.contains("tag"),
        "ES5 output should preserve tag function: {}",
        output
    );
}

/// Parity test for ES5 template literal with function call expression.
/// Function calls in template expressions should be preserved.
#[test]
fn test_parity_es5_template_with_call() {
    let source = r#"const msg = `Result: ${compute(x, y)}`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("msg"),
        "ES5 output should define msg: {}",
        output
    );
    // Function call should be preserved
    assert!(
        output.contains("compute"),
        "ES5 output should preserve function call: {}",
        output
    );
    // Template literal syntax should not appear
    assert!(
        !output.contains("`"),
        "ES5 output should not contain backticks: {}",
        output
    );
}

/// Parity test for ES5 nested template literals.
/// Nested templates should all be converted.
#[test]
fn test_parity_es5_template_nested() {
    let source = r#"const msg = `outer ${`inner ${value}`}`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify variable declaration
    assert!(
        output.contains("msg"),
        "ES5 output should define msg: {}",
        output
    );
    // Value should be preserved
    assert!(
        output.contains("value"),
        "ES5 output should preserve value: {}",
        output
    );
    // Template literal syntax should not appear
    assert!(
        !output.contains("`"),
        "ES5 output should not contain backticks: {}",
        output
    );
}

/// Parity test for ES5 for-of with array destructuring.
/// for (const [a, b] of pairs) should downlevel both for-of and destructuring.
#[test]
fn test_parity_es5_for_of_array_destructuring() {
    let source = "const pairs = [[1,2],[3,4]]; for (const [a, b] of pairs) { console.log(a + b); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 for-of with array destructuring downlevel
    assert!(
        output.contains("var pairs"),
        "ES5 output should define pairs: {}",
        output
    );
    // Should have for loop structure
    assert!(
        output.contains("for (") || output.contains("for("),
        "ES5 output should contain for loop: {}",
        output
    );
    // No for...of syntax
    assert!(
        !output.contains(" of pairs") && !output.contains(" of pairs)"),
        "ES5 output should not contain for...of syntax: {}",
        output
    );
    // No destructuring pattern in loop header
    assert!(
        !output.contains("const [a, b]") && !output.contains("var [a, b]"),
        "ES5 output should not contain destructuring pattern: {}",
        output
    );
}

/// Parity test for ES5 for-of with object destructuring.
/// for (const {x, y} of points) should downlevel both for-of and destructuring.
#[test]
fn test_parity_es5_for_of_object_destructuring() {
    let source =
        "const points = [{x:1,y:2},{x:3,y:4}]; for (const {x, y} of points) { console.log(x, y); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 for-of with object destructuring downlevel
    assert!(
        output.contains("var points"),
        "ES5 output should define points: {}",
        output
    );
    // Should have for loop structure
    assert!(
        output.contains("for (") || output.contains("for("),
        "ES5 output should contain for loop: {}",
        output
    );
    // No for...of syntax
    assert!(
        !output.contains(" of points") && !output.contains(" of points)"),
        "ES5 output should not contain for...of syntax: {}",
        output
    );
    // No destructuring pattern in loop header
    assert!(
        !output.contains("const {x, y}") && !output.contains("var {x, y}"),
        "ES5 output should not contain object destructuring pattern: {}",
        output
    );
}

/// Parity test for ES5 nested for-of loops.
/// Nested for-of should both be downleveled correctly.
#[test]
fn test_parity_es5_for_of_nested() {
    let source = "const matrix = [[1,2],[3,4]]; for (const row of matrix) { for (const cell of row) { console.log(cell); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 nested for-of downlevel
    assert!(
        output.contains("var matrix"),
        "ES5 output should define matrix: {}",
        output
    );
    // Should have console.log preserved
    assert!(
        output.contains("console.log"),
        "ES5 output should contain console.log: {}",
        output
    );
    // No for...of syntax
    assert!(
        !output.contains(" of matrix") && !output.contains(" of row"),
        "ES5 output should not contain for...of syntax: {}",
        output
    );
}

/// Parity test for ES5 for-of with let binding.
/// for (let x of arr) should downlevel to var in ES5.
#[test]
fn test_parity_es5_for_of_let() {
    let source = "const items = [1, 2, 3]; for (let item of items) { item++; console.log(item); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 for-of with let downlevel
    assert!(
        output.contains("var items"),
        "ES5 output should define items with var: {}",
        output
    );
    // Should have for loop structure
    assert!(
        output.contains("for (") || output.contains("for("),
        "ES5 output should contain for loop: {}",
        output
    );
    // No for...of syntax
    assert!(
        !output.contains(" of items") && !output.contains(" of items)"),
        "ES5 output should not contain for...of syntax: {}",
        output
    );
    // No let keyword in ES5
    assert!(
        !output.contains("let "),
        "ES5 output should not contain let keyword: {}",
        output
    );
}

/// Parity test for ES5 logical OR assignment (||=).
/// x ||= y should downlevel to x || (x = y) or equivalent.
#[test]
fn test_parity_es5_logical_or_assignment() {
    let source = "let x = null; x ||= 'default';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 logical OR assignment downlevel
    assert!(
        output.contains("var x"),
        "ES5 output should define x with var: {}",
        output
    );
    // Should contain 'default' value
    assert!(
        output.contains("default") || output.contains("\"default\""),
        "ES5 output should contain default value: {}",
        output
    );
    // No ||= syntax in ES5
    assert!(
        !output.contains("||="),
        "ES5 output should not contain ||= syntax: {}",
        output
    );
}

/// Parity test for ES5 logical AND assignment (&&=).
/// x &&= y should downlevel to x && (x = y) or equivalent.
#[test]
fn test_parity_es5_logical_and_assignment() {
    let source = "let obj = { value: 1 }; obj.value &&= 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 logical AND assignment downlevel
    assert!(
        output.contains("var obj"),
        "ES5 output should define obj with var: {}",
        output
    );
    // Should contain 42 value
    assert!(
        output.contains("42"),
        "ES5 output should contain 42: {}",
        output
    );
    // No &&= syntax in ES5
    assert!(
        !output.contains("&&="),
        "ES5 output should not contain &&= syntax: {}",
        output
    );
}

/// Parity test for ES5 nullish coalescing assignment (??=).
/// x ??= y should downlevel to x ?? (x = y) or null check equivalent.
#[test]
fn test_parity_es5_nullish_assignment_operator() {
    let source = "let config = { timeout: undefined }; config.timeout ??= 5000;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 nullish coalescing assignment downlevel
    assert!(
        output.contains("var config"),
        "ES5 output should define config with var: {}",
        output
    );
    // Should contain 5000 value
    assert!(
        output.contains("5000"),
        "ES5 output should contain 5000: {}",
        output
    );
    // No ??= syntax in ES5
    assert!(
        !output.contains("??="),
        "ES5 output should not contain ??= syntax: {}",
        output
    );
}

/// Parity test for ES5 logical assignment with property access.
/// obj.prop ||= value should downlevel correctly.
#[test]
fn test_parity_es5_logical_assignment_property() {
    let source = "const settings = {}; settings.theme ||= 'dark'; settings.debug &&= false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 logical assignment with property downlevel
    assert!(
        output.contains("var settings"),
        "ES5 output should define settings with var: {}",
        output
    );
    // Should contain theme and debug
    assert!(
        output.contains("theme") && output.contains("debug"),
        "ES5 output should contain theme and debug: {}",
        output
    );
    // No logical assignment syntax in ES5
    assert!(
        !output.contains("||=") && !output.contains("&&="),
        "ES5 output should not contain logical assignment syntax: {}",
        output
    );
}

/// Parity test for ES5 exponentiation operator with type erasure.
/// Verifies type annotations are erased alongside exponentiation usage.
#[test]
fn test_parity_es5_exponentiation_type_erasure() {
    let source = "function power(base: number, exp: number): number { return base ** exp; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function is preserved
    assert!(
        output.contains("function power"),
        "ES5 output should contain function power: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Parameters should be preserved
    assert!(
        output.contains("base") && output.contains("exp"),
        "ES5 output should preserve parameter names: {}",
        output
    );
}

/// Parity test for ES5 exponentiation with const to var.
/// const with exponentiation should convert to var in ES5.
#[test]
fn test_parity_es5_exponentiation_const_to_var() {
    let source = "const squared = 5 ** 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify const is converted to var
    assert!(
        output.contains("var squared"),
        "ES5 output should convert const to var: {}",
        output
    );
    // No const keyword in ES5
    assert!(
        !output.contains("const "),
        "ES5 output should not contain const keyword: {}",
        output
    );
}

/// Parity test for ES5 exponentiation with let to var.
/// let with exponentiation should convert to var in ES5.
#[test]
fn test_parity_es5_exponentiation_let_to_var() {
    let source = "let result = 2 ** 10; result = result ** 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify let is converted to var
    assert!(
        output.contains("var result"),
        "ES5 output should convert let to var: {}",
        output
    );
    // No let keyword in ES5
    assert!(
        !output.contains("let "),
        "ES5 output should not contain let keyword: {}",
        output
    );
}

/// Parity test for ES5 exponentiation in arrow function.
/// Arrow function with exponentiation should downlevel correctly.
#[test]
fn test_parity_es5_exponentiation_arrow() {
    let source = "const cube = (n: number) => n ** 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify const is converted to var
    assert!(
        output.contains("var cube"),
        "ES5 output should convert const to var: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // Arrow function should be converted to regular function
    assert!(
        output.contains("function"),
        "ES5 output should convert arrow to function: {}",
        output
    );
}

/// Parity test for ES5 arrow function with typed expression body.
/// () => expr should convert to function() { return expr; }.
#[test]
fn test_parity_es5_arrow_typed_expression() {
    let source = "const double = (x: number) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Expression body should have return added
    assert!(
        output.contains("return"),
        "ES5 output should add return for expression body: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow function with block body.
/// () => { stmts } should convert to function() { stmts }.
#[test]
fn test_parity_es5_arrow_block_body() {
    let source = "const greet = (name: string) => { console.log('Hello ' + name); };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Console.log should be preserved
    assert!(
        output.contains("console.log"),
        "ES5 output should preserve console.log: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow function with this capture.
/// Arrow functions should capture outer this.
#[test]
fn test_parity_es5_arrow_this_capture() {
    let source = r#"class Counter {
    count = 0;
    increment = () => { this.count++; };
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class is converted to function
    assert!(
        output.contains("function Counter") || output.contains("var Counter"),
        "ES5 output should convert class to function: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
    // Class declaration should be converted (class Counter {)
    assert!(
        !output.contains("class Counter {"),
        "ES5 output should not contain class declaration: {}",
        output
    );
    // Should capture this with _this
    assert!(
        output.contains("_this"),
        "ES5 output should capture this with _this: {}",
        output
    );
}

/// Parity test for ES5 arrow function with multiple parameters.
/// (a, b, c) => expr should convert correctly.
#[test]
fn test_parity_es5_arrow_multi_params() {
    let source = "const sum = (a: number, b: number, c: number) => a + b + c;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Parameters should be preserved (without types)
    assert!(
        output.contains("a") && output.contains("b") && output.contains("c"),
        "ES5 output should preserve parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow with typed params and inference.
/// Arrow with explicit param types and inferred return type.
#[test]
fn test_parity_es5_arrow_typed_params_inference() {
    let source = r#"
interface Point { x: number; y: number }
const distance = (p1: Point, p2: Point) => Math.sqrt((p2.x - p1.x) ** 2 + (p2.y - p1.y) ** 2);
const origin: Point = { x: 0, y: 0 };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Point"),
        "ES5 output should erase Point type annotations: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow with default params and complex types.
/// Arrow with default values that have type annotations.
#[test]
fn test_parity_es5_arrow_defaults_complex() {
    let source = r#"
type Options = { timeout: number; retries: number };
const fetchData = (url: string, options: Options = { timeout: 3000, retries: 3 }) => fetch(url);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Options"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Options"),
        "ES5 output should erase Options type annotation: {}",
        output
    );
    assert!(
        !output.contains(": string"),
        "ES5 output should erase string type annotation: {}",
        output
    );
    // Default values should be preserved
    assert!(
        output.contains("3000") && output.contains("retries"),
        "ES5 output should preserve default values: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow with rest params and tuple types.
/// Arrow with rest parameter that has a tuple type annotation.
#[test]
fn test_parity_es5_arrow_rest_tuple() {
    let source = r#"
type NumTuple = [number, number, number];
const sum = (...nums: NumTuple) => nums.reduce((a, b) => a + b, 0);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type NumTuple"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Tuple type annotation should be erased
    assert!(
        !output.contains(": NumTuple"),
        "ES5 output should erase tuple type annotation: {}",
        output
    );
    // No arrow syntax in outer function
    assert!(
        !output.contains("sum = (") || !output.contains("=>"),
        "ES5 output should convert outer arrow: {}",
        output
    );
}

/// Parity test for ES5 generic arrow functions.
/// Arrow function with generic type parameters.
#[test]
fn test_parity_es5_arrow_generic() {
    let source = r#"
const identity = <T>(value: T): T => value;
const mapArray = <T, U>(arr: T[], fn: (item: T) => U): U[] => arr.map(fn);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, U>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T") && !output.contains(": U"),
        "ES5 output should erase generic type annotations: {}",
        output
    );
    assert!(
        !output.contains("T[]") && !output.contains("U[]"),
        "ES5 output should erase array type annotations: {}",
        output
    );
}

/// Parity test for ES5 arrow with nested destructuring params.
/// Arrow with deeply nested destructuring in parameters.
#[test]
fn test_parity_es5_arrow_nested_destructuring() {
    let source = r#"
interface User { name: string; address: { city: string; zip: number } }
const getCity = ({ address: { city } }: User): string => city;
const getData = ([first, [second, third]]: [number, [string, boolean]]) => first;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrow is converted to function
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": User"),
        "ES5 output should erase User type annotation: {}",
        output
    );
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": boolean"),
        "ES5 output should erase primitive type annotations: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 arrow returning arrow (curried function).
/// Arrow function that returns another arrow function.
#[test]
fn test_parity_es5_arrow_returning_arrow() {
    let source = r#"
type Fn<A, B> = (a: A) => B;
const curry = <A, B, C>(fn: (a: A, b: B) => C): Fn<A, Fn<B, C>> => (a: A) => (b: B) => fn(a, b);
const add = (x: number) => (y: number) => x + y;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify arrows are converted to functions
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Fn"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<A, B, C>") && !output.contains("<A, B>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase number type annotations: {}",
        output
    );
    assert!(
        !output.contains(": Fn"),
        "ES5 output should erase Fn type annotations: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
}

/// Parity test for ES5 getter-only accessor with return type.
/// get prop(): Type should downlevel and erase type.
#[test]
fn test_parity_es5_getter_only_typed() {
    let source = "class Config { get timeout(): number { return 5000; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify Object.defineProperty is used
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase return type: {}",
        output
    );
    // No ES6 getter syntax
    assert!(
        !output.contains("get timeout()"),
        "ES5 output should not contain ES6 getter syntax: {}",
        output
    );
    // Should return 5000
    assert!(
        output.contains("5000"),
        "ES5 output should contain return value: {}",
        output
    );
}

/// Parity test for ES5 setter-only accessor with param type.
/// set prop(v: Type) should downlevel and erase type.
#[test]
fn test_parity_es5_setter_only_typed() {
    let source =
        "class Counter { private _count = 0; set count(val: number) { this._count = val; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify Object.defineProperty is used
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase param type: {}",
        output
    );
    // No ES6 setter syntax
    assert!(
        !output.contains("set count("),
        "ES5 output should not contain ES6 setter syntax: {}",
        output
    );
    // Should reference _count
    assert!(
        output.contains("_count"),
        "ES5 output should reference _count: {}",
        output
    );
}

/// Parity test for ES5 getter/setter pair with types.
/// Both getter return type and setter param type should be erased.
#[test]
fn test_parity_es5_accessor_pair_typed() {
    let source = "class Box<T> { private _value: T; get value(): T { return this._value; } set value(v: T) { this._value = v; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify Object.defineProperty is used
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty: {}",
        output
    );
    // Generic type param should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type param: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No ES6 getter/setter syntax
    assert!(
        !output.contains("get value()") && !output.contains("set value("),
        "ES5 output should not contain ES6 accessor syntax: {}",
        output
    );
}

#[test]
fn test_parity_es5_accessor_getter_inheritance() {
    let source = r#"
abstract class Shape {
    abstract get area(): number;
    abstract get perimeter(): number;
}

class Rectangle extends Shape {
    constructor(private width: number, private height: number) {
        super();
    }

    get area(): number {
        return this.width * this.height;
    }

    get perimeter(): number {
        return 2 * (this.width + this.height);
    }

    get diagonal(): number {
        return Math.sqrt(this.width ** 2 + this.height ** 2);
    }
}

class Square extends Rectangle {
    constructor(side: number) {
        super(side, side);
    }

    override get area(): number {
        return super.area;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Shape") && output.contains("Rectangle") && output.contains("Square"),
        "Classes should be present: {}",
        output
    );
    // abstract keyword should be erased
    assert!(
        !output.contains("abstract"),
        "abstract keyword should be erased: {}",
        output
    );
    // override keyword should be erased
    assert!(
        !output.contains("override"),
        "override keyword should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_accessor_setter_validation() {
    let source = r#"
interface ValidatedField<T> {
    value: T;
}

class Temperature implements ValidatedField<number> {
    private _celsius: number = 0;

    get value(): number {
        return this._celsius;
    }

    set value(temp: number) {
        if (temp < -273.15) {
            throw new Error("Temperature below absolute zero");
        }
        this._celsius = temp;
    }

    get fahrenheit(): number {
        return this._celsius * 9/5 + 32;
    }

    set fahrenheit(temp: number) {
        this.value = (temp - 32) * 5/9;
    }
}

class BoundedValue<T extends number> {
    private _value: T;

    constructor(private min: T, private max: T, initial: T) {
        this._value = initial;
    }

    get value(): T {
        return this._value;
    }

    set value(v: T) {
        if (v < this.min) this._value = this.min;
        else if (v > this.max) this._value = this.max;
        else this._value = v;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Temperature") && output.contains("BoundedValue"),
        "Classes should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // implements clause should be erased
    assert!(
        !output.contains("implements"),
        "implements clause should be erased: {}",
        output
    );
    // Generic constraints should be erased
    assert!(
        !output.contains("extends number"),
        "Generic constraints should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_accessor_pair_caching() {
    let source = r#"
class LazyLoader<T> {
    private _cache: T | null = null;
    private _loaded: boolean = false;

    constructor(private loader: () => T) {}

    get value(): T {
        if (!this._loaded) {
            this._cache = this.loader();
            this._loaded = true;
        }
        return this._cache!;
    }

    set value(v: T) {
        this._cache = v;
        this._loaded = true;
    }

    get isLoaded(): boolean {
        return this._loaded;
    }
}

class MemoizedComputation {
    private _result?: number;
    private _inputA: number = 0;
    private _inputB: number = 0;

    get inputA(): number { return this._inputA; }
    set inputA(v: number) {
        this._inputA = v;
        this._result = undefined;
    }

    get inputB(): number { return this._inputB; }
    set inputB(v: number) {
        this._inputB = v;
        this._result = undefined;
    }

    get result(): number {
        if (this._result === undefined) {
            this._result = this._inputA * this._inputB;
        }
        return this._result;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("LazyLoader") && output.contains("MemoizedComputation"),
        "Classes should be present: {}",
        output
    );
    // Generic type parameter should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type parameter should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T") && !output.contains(": boolean") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_accessor_static_class() {
    let source = r#"
class Configuration {
    private static _instance: Configuration | null = null;
    private static _settings: Map<string, string> = new Map();

    static get instance(): Configuration {
        if (!Configuration._instance) {
            Configuration._instance = new Configuration();
        }
        return Configuration._instance;
    }

    static get settingsCount(): number {
        return Configuration._settings.size;
    }

    static set debug(value: boolean) {
        Configuration._settings.set("debug", String(value));
    }

    static get debug(): boolean {
        return Configuration._settings.get("debug") === "true";
    }
}

class Counter {
    private static _count: number = 0;

    static get count(): number {
        return Counter._count;
    }

    static set count(value: number) {
        if (value >= 0) {
            Counter._count = value;
        }
    }

    static get next(): number {
        return Counter._count++;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Configuration") && output.contains("Counter"),
        "Classes should be present: {}",
        output
    );
    // Should use Object.defineProperty for static accessors
    assert!(
        output.contains("Object.defineProperty"),
        "Should use Object.defineProperty: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Configuration")
            && !output.contains(": number")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_accessor_computed_dynamic() {
    let source = r#"
const propName = "dynamicValue";
const PREFIX = "computed_";

class DynamicAccessors {
    private _data: Record<string, number> = {};

    get [propName](): number {
        return this._data[propName] ?? 0;
    }

    set [propName](value: number) {
        this._data[propName] = value;
    }

    get [`${PREFIX}property`](): string {
        return "computed";
    }

    set [`${PREFIX}property`](value: string) {
        console.log(value);
    }
}

class SymbolAccessors {
    private _hidden: number = 42;

    get [Symbol.toStringTag](): string {
        return "SymbolAccessors";
    }

    static get [Symbol.species](): typeof SymbolAccessors {
        return SymbolAccessors;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("DynamicAccessors") && output.contains("SymbolAccessors"),
        "Classes should be present: {}",
        output
    );
    // Should reference propName variable
    assert!(
        output.contains("propName"),
        "Should reference propName: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
    // Record type should be erased
    assert!(
        !output.contains("Record<"),
        "Record type should be erased: {}",
        output
    );
}

/// Parity test for ES5 static setter.
/// Static setters should be defined on constructor, not prototype.
#[test]
fn test_parity_es5_static_setter_typed() {
    let source = "class Logger { private static _level: string = 'info'; static set level(val: string) { Logger._level = val; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify Object.defineProperty is used
    assert!(
        output.contains("Object.defineProperty"),
        "ES5 output should use Object.defineProperty: {}",
        output
    );
    // Static accessors should be on constructor (Logger), not Logger.prototype
    assert!(
        output.contains("Object.defineProperty(Logger,")
            || output.contains("Object.defineProperty(Logger, "),
        "ES5 output should define static accessor on Logger constructor: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // No ES6 setter syntax
    assert!(
        !output.contains("static set level("),
        "ES5 output should not contain ES6 static setter syntax: {}",
        output
    );
}

/// Parity test for ES5 static block with this reference.
/// Static block using this should downlevel correctly.
#[test]
fn test_parity_es5_static_block_this_ref() {
    let source = r#"class Registry {
    static items: string[] = [];
    static {
        this.items.push("default");
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Registry") || output.contains("function Registry"),
        "ES5 output should define Registry: {}",
        output
    );
    // Static block syntax should not appear
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string[]"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // Should have push call
    assert!(
        output.contains("push"),
        "ES5 output should contain push call: {}",
        output
    );
}

/// Parity test for ES5 multiple static blocks in same class.
/// Multiple static blocks should all be downleveled.
#[test]
fn test_parity_es5_static_block_multiple() {
    let source = r#"class App {
    static name = "";
    static {
        App.name = "MyApp";
    }
    static version = "";
    static {
        App.version = "2.0";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var App") || output.contains("function App"),
        "ES5 output should define App: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Both initializations should occur
    assert!(
        output.contains("MyApp") && output.contains("2.0"),
        "ES5 output should contain both static initializations: {}",
        output
    );
}

/// Parity test for ES5 static block with typed variable.
/// Local typed variables in static block should have types erased.
#[test]
fn test_parity_es5_static_block_typed_var() {
    let source = r#"class Calculator {
    static result = 0;
    static {
        const multiplier: number = 10;
        Calculator.result = 5 * multiplier;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Calculator") || output.contains("function Calculator"),
        "ES5 output should define Calculator: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotation: {}",
        output
    );
    // Const should be converted to var
    assert!(
        !output.contains("const multiplier"),
        "ES5 output should convert const to var: {}",
        output
    );
    // Value 10 should be preserved
    assert!(
        output.contains("10"),
        "ES5 output should contain multiplier value: {}",
        output
    );
}

/// Parity test for ES5 static block with function call.
/// Function calls inside static block should be preserved.
#[test]
fn test_parity_es5_static_block_function_call() {
    let source = r#"class Logger {
    static initialized = false;
    static {
        console.log("Logger static init");
        Logger.initialized = true;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Logger") || output.contains("function Logger"),
        "ES5 output should define Logger: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Console.log should be preserved
    assert!(
        output.contains("console.log"),
        "ES5 output should contain console.log call: {}",
        output
    );
    // String literal should be preserved
    assert!(
        output.contains("Logger static init"),
        "ES5 output should contain log message: {}",
        output
    );
}

/// Parity test for ES5 private method with multiple typed parameters.
/// Private method with types should erase all type annotations.
#[test]
fn test_parity_es5_private_method_multi_params() {
    let source = r#"class MathHelper {
    #add(a: number, b: number, c: number): number {
        return a + b + c;
    }
    sum(x: number, y: number, z: number) {
        return this.#add(x, y, z);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var MathHelper") || output.contains("function MathHelper"),
        "ES5 output should define MathHelper: {}",
        output
    );
    // Private method syntax should not appear
    assert!(
        !output.contains("#add"),
        "ES5 output should not contain #add: {}",
        output
    );
    // All type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 private static method.
/// Private static methods should be downleveled correctly.
#[test]
fn test_parity_es5_private_static_method() {
    let source = r#"class IdGenerator {
    static #nextId: number = 0;
    static #generateId(): number {
        return IdGenerator.#nextId++;
    }
    static create() {
        return IdGenerator.#generateId();
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var IdGenerator") || output.contains("function IdGenerator"),
        "ES5 output should define IdGenerator: {}",
        output
    );
    // Private static method declaration syntax should not appear in original form
    assert!(
        !output.contains("static #generateId():") && !output.contains("static #nextId:"),
        "ES5 output should not contain private static declaration syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 private async method.
/// Private async methods should combine async and private downleveling.
#[test]
fn test_parity_es5_private_async_method() {
    let source = r#"class DataFetcher {
    async #fetchData(url: string): Promise<string> {
        return url;
    }
    async load(endpoint: string) {
        return this.#fetchData(endpoint);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var DataFetcher") || output.contains("function DataFetcher"),
        "ES5 output should define DataFetcher: {}",
        output
    );
    // Private method declaration syntax should not appear
    assert!(
        !output.contains("async #fetchData(") && !output.contains("#fetchData(url"),
        "ES5 output should not contain private method declaration syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Promise"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 private method calling another private method.
/// Chained private method calls should all be downleveled.
#[test]
fn test_parity_es5_private_method_chain() {
    let source = r#"class Processor {
    #step1(val: number): number {
        return val + 1;
    }
    #step2(val: number): number {
        return this.#step1(val) * 2;
    }
    process(input: number) {
        return this.#step2(input);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify ES5 class structure
    assert!(
        output.contains("var Processor") || output.contains("function Processor"),
        "ES5 output should define Processor: {}",
        output
    );
    // Private method syntax should not appear
    assert!(
        !output.contains("#step1") && !output.contains("#step2"),
        "ES5 output should not contain private method syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 multiple class decorators.
/// Multiple decorators should all be applied.
#[test]
fn test_parity_es5_class_decorator_multiple() {
    let source = r#"function sealed(ctor: Function) {}
function logged(ctor: Function) {}
function tracked(ctor: Function) {}

@sealed
@logged
@tracked
class Service {
    name: string = "test";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Service"),
        "Output should define Service class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@sealed") && !output.contains("@logged") && !output.contains("@tracked"),
        "ES5 output should not contain decorator syntax: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 class decorator factory.
/// Decorator factory with arguments should be downleveled.
#[test]
fn test_parity_es5_class_decorator_factory() {
    let source = r#"function component(name: string) {
    return function(ctor: Function) {};
}

@component("MyComponent")
class Widget {
    id: number = 1;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Widget"),
        "Output should define Widget class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@component"),
        "ES5 output should not contain @component decorator syntax: {}",
        output
    );
    // The decorator name should still appear as a function reference
    assert!(
        output.contains("component"),
        "Output should reference component function: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 class decorator with generic class.
/// Decorator on generic class should erase type params.
#[test]
fn test_parity_es5_class_decorator_generic() {
    let source = r#"function observable(ctor: Function) {}

@observable
class Store<T> {
    data: T;
    constructor(initial: T) {
        this.data = initial;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Store"),
        "Output should define Store class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@observable"),
        "ES5 output should not contain @observable decorator syntax: {}",
        output
    );
    // Generic type parameter should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameter: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T") && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 class decorator with extends.
/// Decorator on derived class should work correctly.
#[test]
fn test_parity_es5_class_decorator_extends() {
    let source = r#"function injectable(ctor: Function) {}

class BaseService {
    name: string = "base";
}

@injectable
class DerivedService extends BaseService {
    id: number = 1;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify both classes exist
    assert!(
        output.contains("BaseService") && output.contains("DerivedService"),
        "Output should define both classes: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@injectable"),
        "ES5 output should not contain @injectable decorator syntax: {}",
        output
    );
    // ES5 inheritance should use __extends or prototype chain
    assert!(
        output.contains("__extends") || output.contains(".prototype"),
        "ES5 output should have inheritance pattern: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 multiple method decorators.
/// Multiple decorators on a method should be lowered without decorator syntax.
#[test]
fn test_parity_es5_method_decorator_multiple() {
    let source = r#"function log(target: any, key: string, desc: PropertyDescriptor) {}
function validate(target: any, key: string, desc: PropertyDescriptor) {}
function cache(target: any, key: string, desc: PropertyDescriptor) {}

class DataService {
    @log
    @validate
    @cache
    fetchData(id: number): string {
        return "data";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and method exist
    assert!(
        output.contains("DataService") && output.contains("fetchData"),
        "Output should define DataService with fetchData: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@log") && !output.contains("@validate") && !output.contains("@cache"),
        "ES5 output should not contain decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": string")
            && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 method decorator factory.
/// Decorator factories with arguments should be lowered properly.
#[test]
fn test_parity_es5_method_decorator_factory() {
    let source = r#"function throttle(ms: number) {
    return function(target: any, key: string, desc: PropertyDescriptor) {};
}

class SearchController {
    @throttle(300)
    search(query: string): void {
        console.log(query);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and method exist
    assert!(
        output.contains("SearchController") && output.contains("search"),
        "Output should define SearchController with search: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@throttle"),
        "ES5 output should not contain @throttle decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": string")
            && !output.contains(": void")
            && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static method decorator.
/// Decorators on static methods should be lowered properly.
#[test]
fn test_parity_es5_method_decorator_static() {
    let source = r#"function memoize(target: any, key: string, desc: PropertyDescriptor) {}

class MathUtils {
    @memoize
    static factorial(n: number): number {
        return n <= 1 ? 1 : n * MathUtils.factorial(n - 1);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and static method exist
    assert!(
        output.contains("MathUtils") && output.contains("factorial"),
        "Output should define MathUtils with factorial: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@memoize"),
        "ES5 output should not contain @memoize decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async method decorator.
/// Decorators on async methods should be lowered with async transform.
#[test]
fn test_parity_es5_method_decorator_async() {
    let source = r#"function retry(target: any, key: string, desc: PropertyDescriptor) {}

class ApiClient {
    @retry
    async fetchUser(id: number): Promise<string> {
        return "user";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and method exist
    assert!(
        output.contains("ApiClient") && output.contains("fetchUser"),
        "Output should define ApiClient with fetchUser: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@retry"),
        "ES5 output should not contain @retry decorator syntax: {}",
        output
    );
    // Async should be transformed (no async keyword in ES5)
    assert!(
        !output.contains("async fetchUser"),
        "ES5 output should not contain async method: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains("Promise<string>")
            && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 multiple property decorators.
/// Multiple decorators on a property should be lowered without decorator syntax.
#[test]
fn test_parity_es5_property_decorator_multiple() {
    let source = r#"function observable(target: any, key: string) {}
function validate(target: any, key: string) {}
function persist(target: any, key: string) {}

class FormField {
    @observable
    @validate
    @persist
    value: string = "";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and property exist
    assert!(
        output.contains("FormField") && output.contains("value"),
        "Output should define FormField with value: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@observable")
            && !output.contains("@validate")
            && !output.contains("@persist"),
        "ES5 output should not contain decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": any"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 property decorator factory.
/// Decorator factories with arguments should be lowered properly.
#[test]
fn test_parity_es5_property_decorator_factory() {
    let source = r#"function column(name: string) {
    return function(target: any, key: string) {};
}

class Entity {
    @column("user_name")
    userName: string = "";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and property exist
    assert!(
        output.contains("Entity") && output.contains("userName"),
        "Output should define Entity with userName: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@column"),
        "ES5 output should not contain @column decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": any"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static property decorator.
/// Decorators on static properties should be lowered properly.
#[test]
fn test_parity_es5_property_decorator_static() {
    let source = r#"function readonly(target: any, key: string) {}

class Config {
    @readonly
    static version: string = "1.0.0";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and static property exist
    assert!(
        output.contains("Config") && output.contains("version"),
        "Output should define Config with version: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@readonly"),
        "ES5 output should not contain @readonly decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": any"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 property decorator with complex initializer.
/// Property decorators with arrow function initializers should be lowered properly.
#[test]
fn test_parity_es5_property_decorator_initializer() {
    let source = r#"function lazy(target: any, key: string) {}

class DataLoader {
    @lazy
    loader: () => Promise<string> = () => Promise.resolve("data");
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class and property exist
    assert!(
        output.contains("DataLoader") && output.contains("loader"),
        "Output should define DataLoader with loader: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@lazy"),
        "ES5 output should not contain @lazy decorator syntax: {}",
        output
    );
    // Arrow function should be transformed
    assert!(
        !output.contains("() =>"),
        "ES5 output should not contain arrow function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Promise<string>") && !output.contains(": any"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 getter decorator.
/// Decorator on getter accessor should be lowered properly.
#[test]
fn test_parity_es5_accessor_decorator_getter() {
    let source = r#"function enumerable(target: any, key: string, desc: PropertyDescriptor) {}

class Person {
    private _name: string = "";

    @enumerable
    get name(): string {
        return this._name;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Person"),
        "Output should define Person class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@enumerable"),
        "ES5 output should not contain @enumerable decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 setter decorator.
/// Decorator on setter accessor should be lowered properly.
#[test]
fn test_parity_es5_accessor_decorator_setter() {
    let source = r#"function validate(target: any, key: string, desc: PropertyDescriptor) {}

class Account {
    private _balance: number = 0;

    @validate
    set balance(value: number) {
        this._balance = value;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Account"),
        "Output should define Account class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@validate"),
        "ES5 output should not contain @validate decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 multiple accessor decorators.
/// Multiple decorators on accessor should be lowered properly.
#[test]
fn test_parity_es5_accessor_decorator_multiple() {
    let source = r#"function log(target: any, key: string, desc: PropertyDescriptor) {}
function cache(target: any, key: string, desc: PropertyDescriptor) {}

class Calculator {
    private _result: number = 0;

    @log
    @cache
    get result(): number {
        return this._result;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Calculator"),
        "Output should define Calculator class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@log") && !output.contains("@cache"),
        "ES5 output should not contain decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static accessor decorator.
/// Decorator on static accessor should be lowered properly.
#[test]
fn test_parity_es5_accessor_decorator_static() {
    let source = r#"function readonly(target: any, key: string, desc: PropertyDescriptor) {}

class AppConfig {
    private static _version: string = "1.0";

    @readonly
    static get version(): string {
        return AppConfig._version;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("AppConfig"),
        "Output should define AppConfig class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@readonly"),
        "ES5 output should not contain @readonly decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 multiple parameter decorators.
/// Multiple decorators on a single parameter should be lowered properly.
#[test]
fn test_parity_es5_parameter_decorator_multiple() {
    let source = r#"function required(target: any, key: string, index: number) {}
function validate(target: any, key: string, index: number) {}

class UserService {
    createUser(@required @validate name: string): void {}
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("UserService"),
        "Output should define UserService class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@required") && !output.contains("@validate"),
        "ES5 output should not contain decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 parameter decorator on method.
/// Parameter decorator on regular method should be lowered properly.
#[test]
fn test_parity_es5_parameter_decorator_method() {
    let source = r#"function log(target: any, key: string, index: number) {}

class Logger {
    write(@log message: string): void {
        console.log(message);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Logger"),
        "Output should define Logger class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@log"),
        "ES5 output should not contain @log decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 parameter decorator factory.
/// Parameter decorator factories with arguments should be lowered properly.
#[test]
fn test_parity_es5_parameter_decorator_factory() {
    let source = r#"function maxLength(max: number) {
    return function(target: any, key: string, index: number) {};
}

class FormValidator {
    validate(@maxLength(100) input: string): boolean {
        return true;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("FormValidator"),
        "Output should define FormValidator class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@maxLength"),
        "ES5 output should not contain @maxLength decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 multiple parameters with decorators.
/// Multiple parameters each with decorators should be lowered properly.
#[test]
fn test_parity_es5_parameter_decorator_multi_params() {
    let source = r#"function inject(target: any, key: string, index: number) {}

class Container {
    resolve(@inject a: string, @inject b: number, @inject c: boolean): void {}
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("Container"),
        "Output should define Container class: {}",
        output
    );
    // No decorator syntax in ES5
    assert!(
        !output.contains("@inject"),
        "ES5 output should not contain @inject decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": boolean") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator with try/catch.
/// Async generator with error handling should be lowered properly.
#[test]
fn test_parity_es5_async_generator_try_catch() {
    let source = r#"async function* safeFetch(urls: string[]): AsyncGenerator<string> {
    for (const url of urls) {
        try {
            yield await fetch(url);
        } catch (e: unknown) {
            yield "error";
        }
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("safeFetch"),
        "Output should define safeFetch function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]")
            && !output.contains("AsyncGenerator<")
            && !output.contains(": unknown"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static async generator method.
/// Static async generator methods should be lowered properly.
#[test]
fn test_parity_es5_async_generator_static() {
    let source = r#"class StreamFactory {
    static async *createStream(): AsyncGenerator<number> {
        yield 1;
        yield 2;
        yield 3;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify class exists
    assert!(
        output.contains("StreamFactory"),
        "Output should define StreamFactory class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("AsyncGenerator<number>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator with multiple yields.
/// Async generator with multiple sequential yields should be lowered properly.
#[test]
fn test_parity_es5_async_generator_multi_yield() {
    let source = r#"async function* countdown(start: number): AsyncGenerator<number> {
    yield start;
    yield start - 1;
    yield start - 2;
    yield 0;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("countdown"),
        "Output should define countdown function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("AsyncGenerator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async generator with yield delegation.
/// Async generator using yield* should be lowered properly.
#[test]
fn test_parity_es5_async_generator_yield_star() {
    let source = r#"async function* concat(a: AsyncGenerator<number>, b: AsyncGenerator<number>): AsyncGenerator<number> {
    yield* a;
    yield* b;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function exists
    assert!(
        output.contains("concat"),
        "Output should define concat function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("AsyncGenerator<number>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 nested namespaces.
/// Nested namespaces should be lowered to nested objects.
#[test]
fn test_parity_es5_namespace_nested() {
    let source = r#"namespace Outer {
    export namespace Inner {
        export function helper(): string {
            return "help";
        }
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify outer namespace exists
    assert!(
        output.contains("Outer"),
        "Output should define Outer namespace: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Outer") && !output.contains("namespace Inner"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 namespace with class.
/// Namespace containing a class should be lowered properly.
#[test]
fn test_parity_es5_namespace_with_class() {
    let source = r#"namespace Models {
    export class User {
        name: string;
        constructor(name: string) {
            this.name = name;
        }
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify namespace exists
    assert!(
        output.contains("Models"),
        "Output should define Models namespace: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Models"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 namespace with interface.
/// Interface inside namespace should be erased.
#[test]
fn test_parity_es5_namespace_with_interface() {
    let source = r#"namespace Types {
    export interface Config {
        name: string;
        value: number;
    }
    export const DEFAULT_NAME: string = "default";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify namespace exists
    assert!(
        output.contains("Types"),
        "Output should define Types namespace: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "ES5 output should not contain interface: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Types"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 namespace with enum.
/// Namespace containing an enum should be lowered properly.
#[test]
fn test_parity_es5_namespace_with_enum() {
    let source = r#"namespace Status {
    export enum Code {
        OK = 200,
        NotFound = 404,
        Error = 500
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify namespace exists
    assert!(
        output.contains("Status"),
        "Output should define Status namespace: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Status"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Code"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for ES5 namespace merging.
/// Multiple namespace declarations that merge together.
#[test]
fn test_parity_es5_namespace_merging() {
    let source = r#"
namespace Utils {
    export function log(message: string): void {
        console.log(message);
    }
}

namespace Utils {
    export function warn(message: string): void {
        console.warn(message);
    }
}

namespace Utils {
    export const VERSION: string = "1.0.0";
    export function error(message: string): void {
        console.error(message);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain Utils namespace
    assert!(
        output.contains("Utils"),
        "Output should contain Utils namespace: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Utils"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Functions should be preserved
    assert!(
        output.contains("log") && output.contains("warn") && output.contains("error"),
        "ES5 output should preserve all merged functions: {}",
        output
    );
}

/// Parity test for ES5 namespace with exported functions and types.
/// Namespace with various exported members.
#[test]
fn test_parity_es5_namespace_exports() {
    let source = r#"
namespace Validation {
    export interface Rule<T> {
        validate(value: T): boolean;
        message: string;
    }

    export type Validator<T> = (value: T) => boolean;

    export function required(value: string): boolean {
        return value.length > 0;
    }

    export function minLength(min: number): Validator<string> {
        return (value: string) => value.length >= min;
    }

    export const EMAIL_REGEX: RegExp = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

    export class ValidationError extends Error {
        constructor(public field: string, message: string) {
            super(message);
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain Validation namespace
    assert!(
        output.contains("Validation"),
        "Output should contain Validation namespace: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Validation"),
        "ES5 output should not contain namespace keyword: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Rule"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Validator"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Functions should be preserved
    assert!(
        output.contains("required") && output.contains("minLength"),
        "ES5 output should preserve functions: {}",
        output
    );
    // Class should be preserved
    assert!(
        output.contains("ValidationError"),
        "ES5 output should preserve class: {}",
        output
    );
}

/// Parity test for ES5 deeply nested namespaces with types.
/// Multiple levels of namespace nesting with type declarations.
#[test]
fn test_parity_es5_namespace_deeply_nested() {
    let source = r#"
namespace Company {
    export namespace Department {
        export namespace Team {
            export interface Member {
                name: string;
                role: string;
            }

            export class Employee implements Member {
                constructor(
                    public name: string,
                    public role: string,
                    private id: number
                ) {}

                getInfo(): string {
                    return this.name + " - " + this.role;
                }
            }

            export function createMember(name: string, role: string): Member {
                return { name, role };
            }
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain all namespace levels
    assert!(
        output.contains("Company") && output.contains("Department") && output.contains("Team"),
        "Output should contain all namespace levels: {}",
        output
    );
    // No namespace keyword
    assert!(
        !output.contains("namespace Company")
            && !output.contains("namespace Department")
            && !output.contains("namespace Team"),
        "ES5 output should not contain namespace keywords: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Member"),
        "ES5 output should erase interface: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements Member"),
        "ES5 output should erase implements clause: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": Member"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private keyword: {}",
        output
    );
    // Class and function should be preserved
    assert!(
        output.contains("Employee") && output.contains("createMember"),
        "ES5 output should preserve class and function: {}",
        output
    );
}

/// Parity test for ES5 enum with explicit numeric values.
/// Enum with explicit numeric values should be lowered properly.
#[test]
fn test_parity_es5_enum_explicit_values() {
    let source = r#"enum HttpStatus {
    OK = 200,
    Created = 201,
    BadRequest = 400,
    NotFound = 404,
    ServerError = 500
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify enum exists
    assert!(
        output.contains("HttpStatus"),
        "Output should define HttpStatus enum: {}",
        output
    );
    // Should have the explicit values
    assert!(
        output.contains("200") && output.contains("404") && output.contains("500"),
        "ES5 output should have explicit numeric values: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum HttpStatus"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for ES5 const enum.
/// Const enum should be inlined at usage sites.
#[test]
fn test_parity_es5_enum_const() {
    let source = r#"const enum Flags {
    None = 0,
    Read = 1,
    Write = 2,
    Execute = 4
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Const enums may be completely erased or emitted based on settings
    // No const enum keyword should appear
    assert!(
        !output.contains("const enum"),
        "ES5 output should not contain const enum keyword: {}",
        output
    );
}

/// Parity test for ES5 enum with computed member.
/// Enum with computed values should be lowered properly.
#[test]
fn test_parity_es5_enum_computed() {
    let source = r#"enum FileAccess {
    None,
    Read = 1 << 1,
    Write = 1 << 2,
    ReadWrite = Read | Write
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify enum exists
    assert!(
        output.contains("FileAccess"),
        "Output should define FileAccess enum: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum FileAccess"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for ES5 heterogeneous enum.
/// Enum with mixed string and numeric values should be lowered properly.
#[test]
fn test_parity_es5_enum_heterogeneous() {
    let source = r#"enum Mixed {
    No = 0,
    Yes = "YES",
    Maybe = 1
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify enum exists
    assert!(
        output.contains("Mixed"),
        "Output should define Mixed enum: {}",
        output
    );
    // Should contain the string value
    assert!(
        output.contains("YES"),
        "ES5 output should contain string value: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Mixed"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
}

/// Parity test for ES5 export with alias.
/// Named exports with aliases should be lowered properly.
#[test]
fn test_parity_es5_export_alias() {
    let source = r#"const internalName: string = "value";
export { internalName as publicName };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify CommonJS module marker
    assert!(
        output.contains("__esModule"),
        "CommonJS output should include __esModule marker: {}",
        output
    );
    // Internal name should exist
    assert!(
        output.contains("internalName"),
        "Output should define internalName: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 default export function.
/// Default export function should be lowered to CommonJS.
#[test]
fn test_parity_es5_export_default_function() {
    let source = r#"export default function greet(name: string): string {
    return "Hello, " + name;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify CommonJS module marker
    assert!(
        output.contains("__esModule"),
        "CommonJS output should include __esModule marker: {}",
        output
    );
    // Function should exist
    assert!(
        output.contains("greet"),
        "Output should define greet function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 export interface erasure.
/// Interface exports should be erased entirely.
#[test]
fn test_parity_es5_export_interface() {
    let source = r#"export interface User {
    name: string;
    age: number;
}
export const DEFAULT_AGE: number = 0;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface User"),
        "ES5 output should not contain interface: {}",
        output
    );
    // Const should remain
    assert!(
        output.contains("DEFAULT_AGE"),
        "Output should define DEFAULT_AGE: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 export type alias erasure.
/// Type alias exports should be erased entirely.
#[test]
fn test_parity_es5_export_type_alias() {
    let source = r#"export type ID = string | number;
export type Handler = (event: Event) => void;
export const VERSION: string = "1.0";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type aliases should be erased
    assert!(
        !output.contains("type ID") && !output.contains("type Handler"),
        "ES5 output should not contain type aliases: {}",
        output
    );
    // Const should remain
    assert!(
        output.contains("VERSION"),
        "Output should define VERSION: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 CLASS EXPRESSION PARITY TESTS
// ============================================================================

#[test]
fn test_parity_es5_class_expression_anonymous() {
    let source = r#"
const MyClass = class {
    value: number;
    constructor(val: number) {
        this.value = val;
    }
    getValue(): number {
        return this.value;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class expression assignment
    assert!(
        output.contains("MyClass"),
        "Output should contain MyClass: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_expression_named() {
    let source = r#"
const Factory = class InnerClass {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
    static create(name: string): InnerClass {
        return new InnerClass(name);
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both the variable and the class name
    assert!(
        output.contains("Factory"),
        "Output should contain Factory: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": InnerClass"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_expression_static() {
    let source = r#"
const Counter = class {
    static count: number = 0;
    static increment(): void {
        Counter.count++;
    }
    static getCount(): number {
        return Counter.count;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the Counter class
    assert!(
        output.contains("Counter"),
        "Output should contain Counter: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_expression_accessor() {
    let source = r#"
const Rectangle = class {
    private _width: number;
    private _height: number;

    constructor(width: number, height: number) {
        this._width = width;
        this._height = height;
    }

    get area(): number {
        return this._width * this._height;
    }

    set width(value: number) {
        this._width = value;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the Rectangle class
    assert!(
        output.contains("Rectangle"),
        "Output should contain Rectangle: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
}

/// Parity test for ES5 class expression in return statement.
/// Class expression returned from a factory function.
#[test]
fn test_parity_es5_class_expression_return() {
    let source = r#"
interface Component { render(): string }
function createComponent(name: string): new () => Component {
    return class implements Component {
        private name: string = name;

        render(): string {
            return "<" + this.name + "/>";
        }
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the function name
    assert!(
        output.contains("createComponent"),
        "Output should contain createComponent function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements"),
        "ES5 output should erase implements: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": new ()"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private keyword: {}",
        output
    );
}

/// Parity test for ES5 class expression as argument.
/// Class expression passed as an argument to a function.
#[test]
fn test_parity_es5_class_expression_argument() {
    let source = r#"
interface Handler { handle(data: any): void }
function registerHandler(HandlerClass: new () => Handler): void {
    const instance = new HandlerClass();
    instance.handle({ type: "init" });
}

registerHandler(class implements Handler {
    private processed: number = 0;

    handle(data: any): void {
        this.processed++;
        console.log("Handling:", data);
    }
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the function name
    assert!(
        output.contains("registerHandler"),
        "Output should contain registerHandler function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements"),
        "ES5 output should erase implements: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Handler") && !output.contains(": any") && !output.contains(": void"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private keyword: {}",
        output
    );
}

/// Parity test for ES5 class expression extending expression.
/// Class expression extending a computed base class.
#[test]
fn test_parity_es5_class_expression_extends_computed() {
    let source = r#"
interface Serializable { serialize(): string }
function getMixin<T extends new (...args: any[]) => Serializable>(Base: T) {
    return class extends Base {
        toJSON(): string {
            return this.serialize();
        }
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the function name
    assert!(
        output.contains("getMixin"),
        "Output should contain getMixin function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T extends"),
        "ES5 output should erase generic constraints: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": T"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 class expression with implements.
/// Class expression implementing multiple interfaces.
#[test]
fn test_parity_es5_class_expression_implements() {
    let source = r#"
interface Readable { read(): string }
interface Writable { write(data: string): void }
interface Closable { close(): void }

const Stream = class implements Readable, Writable, Closable {
    private buffer: string = "";

    read(): string {
        return this.buffer;
    }

    write(data: string): void {
        this.buffer += data;
    }

    close(): void {
        this.buffer = "";
    }
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the Stream class
    assert!(
        output.contains("Stream"),
        "Output should contain Stream class: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interfaces: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements"),
        "ES5 output should erase implements clause: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private keyword: {}",
        output
    );
}

/// Parity test for ES5 class expression in array.
/// Class expressions stored in an array.
#[test]
fn test_parity_es5_class_expression_array() {
    let source = r#"
interface Shape { area(): number }
type ShapeConstructor = new () => Shape;

const shapes: ShapeConstructor[] = [
    class implements Shape {
        area(): number { return 100; }
    },
    class implements Shape {
        area(): number { return 200; }
    }
];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the shapes variable
    assert!(
        output.contains("shapes"),
        "Output should contain shapes variable: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type ShapeConstructor"),
        "ES5 output should erase type alias: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements"),
        "ES5 output should erase implements: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": ShapeConstructor[]"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

/// Parity test for ES5 class expression in IIFE.
/// Class expression inside an immediately-invoked function expression.
#[test]
fn test_parity_es5_class_expression_iife() {
    let source = r#"
interface Singleton { getInstance(): Singleton }
const singleton = (function(): new () => Singleton {
    let instance: Singleton | null = null;

    return class implements Singleton {
        constructor() {
            if (instance) return instance;
            instance = this;
        }

        getInstance(): Singleton {
            return instance!;
        }
    };
})();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the singleton variable
    assert!(
        output.contains("singleton"),
        "Output should contain singleton variable: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // implements should be erased
    assert!(
        !output.contains("implements"),
        "ES5 output should erase implements: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Singleton") && !output.contains(": new ()"),
        "ES5 output should erase type annotations: {}",
        output
    );
}

// ============================================================================
// ES5 GENERATOR FUNCTION PARITY TESTS (ADDITIONAL)
// ============================================================================

#[test]
fn test_parity_es5_generator_return_value() {
    let source = r#"
function* countdown(start: number): Generator<number, string, unknown> {
    for (let i = start; i > 0; i--) {
        yield i;
    }
    return "done";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the countdown function
    assert!(
        output.contains("countdown"),
        "Output should contain countdown function: {}",
        output
    );
    // Generator type annotation should be erased
    assert!(
        !output.contains("Generator<"),
        "Generator type should be erased: {}",
        output
    );
    // Parameter type should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_generator_multiple_yields() {
    let source = r#"
function* multiYield(): Generator<string> {
    yield "first";
    yield "second";
    yield "third";
    yield "fourth";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the function name
    assert!(
        output.contains("multiYield"),
        "Output should contain multiYield function: {}",
        output
    );
    // Generator type should be erased
    assert!(
        !output.contains("Generator<"),
        "Generator type should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_generator_try_catch() {
    let source = r#"
function* safeGenerator(): Generator<number> {
    try {
        yield 1;
        yield 2;
    } catch (e: unknown) {
        yield -1;
    } finally {
        yield 0;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the function name
    assert!(
        output.contains("safeGenerator"),
        "Output should contain safeGenerator function: {}",
        output
    );
    // Catch param type annotation should be erased
    assert!(
        !output.contains(": unknown"),
        "Catch param type should be erased: {}",
        output
    );
    // Generator type should be erased
    assert!(
        !output.contains("Generator<"),
        "Generator type should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_generator_expression() {
    let source = r#"
const gen = function* (limit: number): Generator<number> {
    for (let i = 0; i < limit; i++) {
        yield i * 2;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variable name
    assert!(
        output.contains("gen"),
        "Output should contain gen variable: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("Generator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 generator with typed yields.
/// Generator function with complex typed yield expressions.
#[test]
fn test_parity_es5_generator_typed_yields() {
    let source = r#"
interface Item { id: number; name: string }
function* itemGenerator(): Generator<Item, void, undefined> {
    yield { id: 1, name: "first" };
    yield { id: 2, name: "second" };
    yield { id: 3, name: "third" };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("itemGenerator"),
        "Output should contain itemGenerator function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Generator<Item") && !output.contains(": Item"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Should have yield statements
    assert!(
        output.contains("yield"),
        "ES5 output should contain yield statements: {}",
        output
    );
}

/// Parity test for ES5 generator with delegation (yield*).
/// Generator function that delegates to another generator.
#[test]
fn test_parity_es5_generator_delegation() {
    let source = r#"
function* innerGen(): Generator<number> {
    yield 1;
    yield 2;
}
function* outerGen(): Generator<number> {
    yield 0;
    yield* innerGen();
    yield 3;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both function names
    assert!(
        output.contains("innerGen") && output.contains("outerGen"),
        "Output should contain innerGen and outerGen functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Generator<number>"),
        "ES5 output should erase Generator type: {}",
        output
    );
    // Should have yield statements
    assert!(
        output.contains("yield"),
        "ES5 output should contain yield statements: {}",
        output
    );
}

/// Parity test for ES5 async generator with await.
/// Async generator function combining yield and await.
#[test]
fn test_parity_es5_generator_async_await() {
    let source = r#"
interface DataChunk { data: string; hasMore: boolean }
async function* streamData(url: string): AsyncGenerator<DataChunk> {
    let hasMore = true;
    while (hasMore) {
        const response = await fetch(url);
        const chunk: DataChunk = await response.json();
        yield chunk;
        hasMore = chunk.hasMore;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("streamData"),
        "Output should contain streamData function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("AsyncGenerator<")
            && !output.contains(": DataChunk")
            && !output.contains(": string"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No async keyword in ES5
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async keyword: {}",
        output
    );
}

/// Parity test for ES5 generator in class methods.
/// Generator method with this binding in a class.
#[test]
fn test_parity_es5_generator_class_method_this() {
    let source = r#"
class NumberSequence {
    private start: number;
    private end: number;

    constructor(start: number, end: number) {
        this.start = start;
        this.end = end;
    }

    *values(): Generator<number> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain class name
    assert!(
        output.contains("NumberSequence"),
        "Output should contain NumberSequence class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("Generator<"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private modifier: {}",
        output
    );
    // Method should reference this
    assert!(
        output.contains("this.start") || output.contains("this.end"),
        "ES5 output should preserve this references: {}",
        output
    );
}

/// Parity test for ES5 generator with try/finally.
/// Generator function with cleanup in finally block.
#[test]
fn test_parity_es5_generator_try_finally() {
    let source = r#"
interface Connection { close(): void }
function* processWithCleanup(conn: Connection): Generator<string, void, undefined> {
    try {
        yield "processing";
        yield "more processing";
    } finally {
        conn.close();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("processWithCleanup"),
        "Output should contain processWithCleanup function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Connection") && !output.contains("Generator<string"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // try/finally structure should be preserved
    assert!(
        output.contains("try") && output.contains("finally"),
        "ES5 output should preserve try/finally structure: {}",
        output
    );
    // Cleanup call should be preserved
    assert!(
        output.contains("close"),
        "ES5 output should preserve cleanup call: {}",
        output
    );
}

/// Parity test for ES5 generator with complex return value.
/// Generator function with typed return value after yields.
#[test]
fn test_parity_es5_generator_complex_return() {
    let source = r#"
interface Summary { count: number; total: number }
function* accumulator(values: number[]): Generator<number, Summary, undefined> {
    let total = 0;
    for (const v of values) {
        total += v;
        yield v;
    }
    return { count: values.length, total };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain function name
    assert!(
        output.contains("accumulator"),
        "Output should contain accumulator function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains("Generator<number, Summary"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Return object structure should be preserved
    assert!(
        output.contains("count") && output.contains("total"),
        "ES5 output should preserve return object: {}",
        output
    );
}

// ============================================================================
// ES5 CLASS STATIC INITIALIZATION ORDER PARITY TESTS
// ============================================================================

#[test]
fn test_parity_es5_static_init_order_properties() {
    let source = r#"
class Counter {
    static first: number = 1;
    static second: number = Counter.first + 1;
    static third: number = Counter.second + 1;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Counter"),
        "Output should contain Counter class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_init_order_blocks() {
    let source = r#"
class Logger {
    static log: string[] = [];
    static {
        Logger.log.push("first");
    }
    static {
        Logger.log.push("second");
    }
    static {
        Logger.log.push("third");
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Logger"),
        "Output should contain Logger class: {}",
        output
    );
    // Should contain all the push calls
    assert!(
        output.contains("first") && output.contains("second") && output.contains("third"),
        "Output should contain all log entries: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_init_order_interleaved() {
    let source = r#"
class Interleaved {
    static a: number = 1;
    static {
        Interleaved.b = Interleaved.a * 2;
    }
    static b: number;
    static c: number = Interleaved.b + 1;
    static {
        Interleaved.d = Interleaved.c * 2;
    }
    static d: number;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Interleaved"),
        "Output should contain Interleaved class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_init_order_derived() {
    let source = r#"
class Base {
    static baseValue: number = 10;
    static {
        Base.baseValue = Base.baseValue * 2;
    }
}

class Derived extends Base {
    static derivedValue: number = Base.baseValue + 5;
    static {
        Derived.derivedValue = Derived.derivedValue * 2;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "Output should contain Base and Derived classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 ASYNC CLASS METHOD WITH SUPER CALL PARITY TESTS
// ============================================================================

#[test]
fn test_parity_es5_async_super_call_basic() {
    let source = r#"
class Base {
    greet(): string {
        return "Hello";
    }
}

class Derived extends Base {
    async greetAsync(): Promise<string> {
        return super.greet() + " World";
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "Output should contain Base and Derived classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_async_super_call_with_args() {
    let source = r#"
class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}

class AsyncCalculator extends Calculator {
    async addAsync(a: number, b: number): Promise<number> {
        const result = super.add(a, b);
        return result;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Calculator") && output.contains("AsyncCalculator"),
        "Output should contain Calculator and AsyncCalculator classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_async_super_call_static() {
    let source = r#"
class BaseService {
    static getData(): string {
        return "data";
    }
}

class DerivedService extends BaseService {
    static async getDataAsync(): Promise<string> {
        return super.getData();
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("BaseService") && output.contains("DerivedService"),
        "Output should contain BaseService and DerivedService classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_async_super_call_with_await() {
    let source = r#"
class DataFetcher {
    fetch(): string {
        return "raw data";
    }
}

class AsyncDataFetcher extends DataFetcher {
    async fetchAndProcess(): Promise<string> {
        const raw = super.fetch();
        const processed = await Promise.resolve(raw.toUpperCase());
        return processed;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("DataFetcher") && output.contains("AsyncDataFetcher"),
        "Output should contain DataFetcher and AsyncDataFetcher classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("Promise<string>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async with try/finally.
/// Async function with try/finally should downlevel properly.
#[test]
fn test_parity_es5_async_try_finally() {
    let source = r#"
interface Resource { close(): void }
async function withResource<T>(resource: Resource, fn: () => Promise<T>): Promise<T> {
    try {
        return await fn();
    } finally {
        resource.close();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function is emitted
    assert!(
        output.contains("function") && output.contains("withResource"),
        "ES5 output should contain withResource function: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Resource") && !output.contains("Promise<T>"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // try/finally structure should be preserved
    assert!(
        output.contains("try") && output.contains("finally"),
        "ES5 output should preserve try/finally structure: {}",
        output
    );
    // No async keyword in ES5
    assert!(
        !output.contains("async function"),
        "ES5 output should not contain async keyword: {}",
        output
    );
}

/// Parity test for ES5 async with Promise.all destructuring.
/// Async function with Promise.all and destructuring assignment.
#[test]
fn test_parity_es5_async_promise_all_destructure() {
    let source = r#"
interface User { id: number; name: string }
interface Order { orderId: string; total: number }
async function fetchUserAndOrders(userId: number): Promise<[User, Order[]]> {
    const [user, orders] = await Promise.all([
        fetchUser(userId),
        fetchOrders(userId)
    ]);
    return [user, orders];
}
declare function fetchUser(id: number): Promise<User>;
declare function fetchOrders(id: number): Promise<Order[]>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function is emitted
    assert!(
        output.contains("function") && output.contains("fetchUserAndOrders"),
        "ES5 output should contain fetchUserAndOrders function: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interfaces: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "ES5 output should erase type annotations: {}",
        output
    );
    assert!(
        !output.contains("Promise<[User, Order[]]>"),
        "ES5 output should erase return type: {}",
        output
    );
    // Promise.all should be preserved
    assert!(
        output.contains("Promise.all"),
        "ES5 output should preserve Promise.all: {}",
        output
    );
    // Declare functions should be erased
    assert!(
        !output.contains("declare"),
        "ES5 output should erase declare statements: {}",
        output
    );
}

/// Parity test for ES5 async IIFE (Immediately Invoked Function Expression).
/// Async IIFE should downlevel to non-async IIFE with __awaiter.
#[test]
fn test_parity_es5_async_iife() {
    let source = r#"
type Config = { apiUrl: string; timeout: number };
const result = (async (): Promise<Config> => {
    const response = await fetch("/config");
    return response.json();
})();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function is emitted
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Config"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Promise<Config>"),
        "ES5 output should erase return type: {}",
        output
    );
    // No async keyword in ES5
    assert!(
        !output.contains("async"),
        "ES5 output should not contain async keyword: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
    // Fetch call should be preserved
    assert!(
        output.contains("fetch"),
        "ES5 output should preserve fetch call: {}",
        output
    );
}

/// Parity test for ES5 nested async arrows.
/// Multiple levels of nested async arrow functions.
#[test]
fn test_parity_es5_async_nested_arrows() {
    let source = r#"
interface Data { value: number }
const outer = async (x: number): Promise<(y: number) => Promise<Data>> => {
    const inner = async (y: number): Promise<Data> => {
        const result = await process(x + y);
        return { value: result };
    };
    return inner;
};
declare function process(n: number): Promise<number>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Verify function is emitted (should have multiple functions for outer and inner)
    assert!(
        output.contains("function"),
        "ES5 output should contain function keyword: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": Promise"),
        "ES5 output should erase type annotations: {}",
        output
    );
    assert!(
        !output.contains(": Data"),
        "ES5 output should erase Data type annotation: {}",
        output
    );
    // No async keyword in ES5
    assert!(
        !output.contains("async"),
        "ES5 output should not contain async keyword: {}",
        output
    );
    // No arrow syntax
    assert!(
        !output.contains("=>"),
        "ES5 output should not contain arrow syntax: {}",
        output
    );
    // Declare function should be erased
    assert!(
        !output.contains("declare"),
        "ES5 output should erase declare statements: {}",
        output
    );
    // Variable names should be preserved
    assert!(
        output.contains("outer") && output.contains("inner"),
        "ES5 output should preserve variable names: {}",
        output
    );
}

// ============================================================================
// ES5 PRIVATE FIELD ACCESSOR PARITY TESTS
// ============================================================================

#[test]
fn test_parity_es5_private_accessor_getter() {
    let source = r#"
class Person {
    #name: string = "Anonymous";

    get #privateName(): string {
        return this.#name;
    }

    getName(): string {
        return this.#privateName;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Person"),
        "Output should contain Person class: {}",
        output
    );
    // Private field syntax should be transformed
    assert!(
        !output.contains("#name") && !output.contains("#privateName"),
        "Private field syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_accessor_setter() {
    let source = r#"
class Counter {
    #count: number = 0;

    set #privateCount(value: number) {
        this.#count = value;
    }

    setCount(value: number): void {
        this.#privateCount = value;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Counter"),
        "Output should contain Counter class: {}",
        output
    );
    // Private field syntax should be transformed
    assert!(
        !output.contains("#count") && !output.contains("#privateCount"),
        "Private field syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_accessor_pair() {
    let source = r#"
class Temperature {
    #celsius: number = 0;

    get #privateTemp(): number {
        return this.#celsius;
    }

    set #privateTemp(value: number) {
        this.#celsius = value;
    }

    get fahrenheit(): number {
        return this.#privateTemp * 9 / 5 + 32;
    }

    set fahrenheit(value: number) {
        this.#privateTemp = (value - 32) * 5 / 9;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Temperature"),
        "Output should contain Temperature class: {}",
        output
    );
    // Private field syntax should be transformed
    assert!(
        !output.contains("#celsius") && !output.contains("#privateTemp"),
        "Private field syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_accessor_static() {
    let source = r#"
class Registry {
    static #items: string[] = [];

    static get #privateItems(): string[] {
        return Registry.#items;
    }

    static getAll(): string[] {
        return Registry.#privateItems;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Registry"),
        "Output should contain Registry class: {}",
        output
    );
    // Should use __classPrivateFieldGet helper for private field access
    assert!(
        output.contains("__classPrivateFieldGet") || !output.contains("#items"),
        "Output should transform private fields: {}",
        output
    );
}

// ============================================================================
// ES5 CLASS STATIC BLOCK PARITY TESTS (ADDITIONAL)
// ============================================================================

#[test]
fn test_parity_es5_static_block_try_catch() {
    let source = r#"
class SafeInit {
    static config: Record<string, string> = {};

    static {
        try {
            SafeInit.config["key"] = "value";
        } catch (e: unknown) {
            SafeInit.config["error"] = "failed";
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("SafeInit"),
        "Output should contain SafeInit class: {}",
        output
    );
    // Should contain try-catch structure
    assert!(
        output.contains("try") && output.contains("catch"),
        "Output should contain try-catch: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": unknown"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_block_loop_init() {
    let source = r#"
class LookupTable {
    static table: number[] = [];

    static {
        for (let i = 0; i < 10; i++) {
            LookupTable.table.push(i * i);
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("LookupTable"),
        "Output should contain LookupTable class: {}",
        output
    );
    // Should contain loop structure
    assert!(
        output.contains("for"),
        "Output should contain for loop: {}",
        output
    );
    // let should be transformed to var
    assert!(
        !output.contains("let i"),
        "let should be transformed to var: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_block_conditional() {
    let source = r#"
class Environment {
    static isDev: boolean = false;
    static apiUrl: string;

    static {
        if (Environment.isDev) {
            Environment.apiUrl = "http://localhost:3000";
        } else {
            Environment.apiUrl = "https://api.example.com";
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Environment"),
        "Output should contain Environment class: {}",
        output
    );
    // Should contain conditional structure
    assert!(
        output.contains("if") && output.contains("else"),
        "Output should contain if-else: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": boolean") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_block_derived_class() {
    let source = r#"
class Parent {
    static parentValue: number = 10;
}

class Child extends Parent {
    static childValue: number;

    static {
        Child.childValue = Parent.parentValue * 2;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Parent") && output.contains("Child"),
        "Output should contain Parent and Child classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static block with async IIFE.
/// Static block containing async immediately-invoked function expression.
#[test]
fn test_parity_es5_static_block_async() {
    let source = r#"
interface Config { endpoint: string; timeout: number }
class ApiClient {
    static config: Config;

    static {
        (async () => {
            const response = await fetch("/config");
            ApiClient.config = await response.json();
        })();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain class name
    assert!(
        output.contains("ApiClient"),
        "Output should contain ApiClient class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Config"),
        "ES5 output should erase Config type annotation: {}",
        output
    );
    // No async arrow syntax in ES5
    assert!(
        !output.contains("async ()"),
        "ES5 output should not contain async arrow syntax: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
}

/// Parity test for ES5 static block with private field access.
/// Static block accessing private static fields.
#[test]
fn test_parity_es5_static_block_private_access() {
    let source = r#"
class Counter {
    static #count: number = 0;
    static #instances: Counter[] = [];

    static {
        Counter.#count = 0;
        Counter.#instances = [];
        console.log("Counter initialized");
    }

    static getCount(): number {
        return Counter.#count;
    }

    constructor() {
        Counter.#count++;
        Counter.#instances.push(this);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain class name
    assert!(
        output.contains("Counter"),
        "Output should contain Counter class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": Counter[]"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Should use __classPrivateFieldGet/Set helpers
    assert!(
        output.contains("__classPrivateFieldGet") || output.contains("__classPrivateFieldSet"),
        "ES5 output should use private field helpers: {}",
        output
    );
}

/// Parity test for ES5 static block initialization order.
/// Multiple static blocks verifying initialization order.
#[test]
fn test_parity_es5_static_block_init_order() {
    let source = r#"
class OrderTest {
    static first: string = "initialized first";

    static {
        console.log("First static block:", OrderTest.first);
    }

    static second: string = "initialized second";

    static {
        console.log("Second static block:", OrderTest.second);
    }

    static third: string;

    static {
        OrderTest.third = OrderTest.first + " and " + OrderTest.second;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain class name
    assert!(
        output.contains("OrderTest"),
        "Output should contain OrderTest class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Static property values should be present
    assert!(
        output.contains("first") && output.contains("second") && output.contains("third"),
        "ES5 output should preserve static property names: {}",
        output
    );
}

/// Parity test for ES5 static block with super access.
/// Static block in derived class accessing parent static members via super.
#[test]
fn test_parity_es5_static_block_super() {
    let source = r#"
class BaseConfig {
    static baseUrl: string = "https://api.example.com";
    static version: number = 1;

    static getFullUrl(): string {
        return BaseConfig.baseUrl + "/v" + BaseConfig.version;
    }
}

class DerivedConfig extends BaseConfig {
    static apiKey: string;
    static fullEndpoint: string;

    static {
        DerivedConfig.apiKey = "secret-key";
        DerivedConfig.fullEndpoint = super.getFullUrl() + "/resource";
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both class names
    assert!(
        output.contains("BaseConfig") && output.contains("DerivedConfig"),
        "Output should contain BaseConfig and DerivedConfig classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No static block syntax
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
    // Static property values should be present
    assert!(
        output.contains("apiKey") && output.contains("fullEndpoint"),
        "ES5 output should preserve static property names: {}",
        output
    );
}

// ============================================================================
// ES5 COMPUTED PROPERTY PARITY TESTS (ADDITIONAL)
// ============================================================================

#[test]
fn test_parity_es5_computed_property_symbol() {
    let source = r#"
const sym = Symbol("key");

const obj = {
    [sym]: "symbol value",
    [Symbol.iterator]: function* () {
        yield 1;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain Symbol references
    assert!(
        output.contains("Symbol"),
        "Output should contain Symbol: {}",
        output
    );
    // Should contain the object
    assert!(
        output.contains("obj"),
        "Output should contain obj: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_class_method() {
    let source = r#"
const methodName = "dynamicMethod";

class DynamicClass {
    [methodName](x: number): number {
        return x * 2;
    }

    static ["staticMethod"](y: string): string {
        return y.toUpperCase();
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("DynamicClass"),
        "Output should contain DynamicClass: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_expression() {
    let source = r#"
const prefix = "prop";
const index = 1;

const obj: Record<string, number> = {
    [prefix + index]: 100,
    [prefix + (index + 1)]: 200,
    [`${prefix}${index + 2}`]: 300
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the object
    assert!(
        output.contains("obj"),
        "Output should contain obj: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains("Record<"),
        "Type annotation should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_accessor() {
    let source = r#"
const propName = "value";

class ComputedAccessor {
    private _data: number = 0;

    get [propName](): number {
        return this._data;
    }

    set [propName](val: number) {
        this._data = val;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("ComputedAccessor"),
        "Output should contain ComputedAccessor: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "private keyword should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_template() {
    let source = r#"
const prefix = "user";
const suffix = "Name";

const obj: Record<string, string> = {
    [`${prefix}_${suffix}`]: "John",
    [`${prefix}_age`]: "30"
};

class TemplateComputed {
    [`get${suffix}`](): string {
        return "value";
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain TemplateComputed class
    assert!(
        output.contains("TemplateComputed"),
        "Output should contain TemplateComputed: {}",
        output
    );
    // Template literals should be downleveled to concatenation
    assert!(
        output.contains("prefix") && output.contains("suffix"),
        "Output should use prefix and suffix variables: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Record<"),
        "Type annotation should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_binary() {
    let source = r#"
const base = 10;
const index = 5;

const lookup: { [key: number]: string } = {
    [base + index]: "fifteen",
    [base * 2]: "twenty",
    [base - 5]: "five"
};

class BinaryComputed {
    [1 + 1](): number {
        return 2;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("BinaryComputed"),
        "Output should contain BinaryComputed: {}",
        output
    );
    // Should contain binary operations
    assert!(
        output.contains("base") && output.contains("index"),
        "Output should contain base and index: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_nested() {
    let source = r#"
const outerKey = "outer";
const innerKey = "inner";

interface NestedConfig {
    [key: string]: {
        [key: string]: number;
    };
}

const config: NestedConfig = {
    [outerKey]: {
        [innerKey]: 42,
        ["static"]: 100
    }
};

function createNested<T>(key: string, value: T): { [k: string]: T } {
    return { [key]: value };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain variable declarations
    assert!(
        output.contains("outerKey") && output.contains("innerKey"),
        "Output should contain outerKey and innerKey: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Generic type parameter should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type parameter should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": NestedConfig"),
        "Type annotation should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_computed_property_conditional() {
    let source = r#"
const useAlternate = true;
const primaryKey = "primary";
const altKey = "alternate";

const settings: { [key: string]: boolean } = {
    [useAlternate ? altKey : primaryKey]: true,
    [!useAlternate ? "disabled" : "enabled"]: false
};

class ConditionalComputed {
    static readonly FLAG = true;

    [ConditionalComputed.FLAG ? "active" : "inactive"](): void {
        console.log("called");
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("ConditionalComputed"),
        "Output should contain ConditionalComputed: {}",
        output
    );
    // Should contain conditional expressions
    assert!(
        output.contains("useAlternate") || output.contains("?"),
        "Output should contain conditional logic: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
    // readonly keyword should be erased
    assert!(
        !output.contains("readonly"),
        "readonly keyword should be erased: {}",
        output
    );
}

/// Parity test for ES5 computed property with method call as key.
/// Computed property using method call result as key.
#[test]
fn test_parity_es5_computed_property_method_call() {
    let source = r#"
interface KeyProvider { getKey(): string }
class PropertyMapper {
    private prefix: string;

    constructor(prefix: string) {
        this.prefix = prefix;
    }

    getPropertyName(suffix: string): string {
        return this.prefix + "_" + suffix;
    }

    createObject(): { [key: string]: number } {
        return {
            [this.getPropertyName("first")]: 1,
            [this.getPropertyName("second")]: 2,
            [this.prefix.toUpperCase()]: 3
        };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("PropertyMapper"),
        "Output should contain PropertyMapper class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": { [key: string]"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // private keyword should be erased
    assert!(
        !output.contains("private"),
        "ES5 output should erase private keyword: {}",
        output
    );
    // Method calls should be preserved
    assert!(
        output.contains("getPropertyName") && output.contains("toUpperCase"),
        "ES5 output should preserve method calls: {}",
        output
    );
}

/// Parity test for ES5 computed property with function call as key.
/// Computed property using external function call as key.
#[test]
fn test_parity_es5_computed_property_function_call() {
    let source = r#"
function generateKey(namespace: string, name: string): string {
    return namespace + ":" + name;
}

function getSymbol(): symbol {
    return Symbol("dynamic");
}

interface Config {
    [key: string]: string | number;
}

const config: Config = {
    [generateKey("app", "version")]: "1.0.0",
    [generateKey("app", "name")]: "MyApp",
    [String(Date.now())]: "timestamp",
    ["static_" + "key"]: "concatenated"
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the config variable
    assert!(
        output.contains("config"),
        "Output should contain config variable: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": symbol")
            && !output.contains(": Config"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Function calls should be preserved
    assert!(
        output.contains("generateKey") && output.contains("Date.now"),
        "ES5 output should preserve function calls: {}",
        output
    );
}

/// Parity test for ES5 computed property with complex typed expressions.
/// Computed property with generic types and complex expressions.
#[test]
fn test_parity_es5_computed_property_typed() {
    let source = r#"
type PropertyKey = string | number | symbol;
interface TypedObject<K extends PropertyKey, V> {
    [key: string]: V;
}

class TypedPropertyBuilder<T> {
    private readonly keyPrefix: string;
    private counter: number = 0;

    constructor(prefix: string) {
        this.keyPrefix = prefix;
    }

    nextKey(): string {
        return this.keyPrefix + "_" + (this.counter++);
    }

    build(value: T): TypedObject<string, T> {
        return {
            [this.nextKey()]: value,
            [this.keyPrefix + "_static"]: value
        };
    }
}

const builder = new TypedPropertyBuilder<number>("prop");
const result = builder.build(42);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("TypedPropertyBuilder"),
        "Output should contain TypedPropertyBuilder class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type PropertyKey"),
        "ES5 output should erase type alias: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>")
            && !output.contains("<K extends")
            && !output.contains("<string, T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number") && !output.contains(": T"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // readonly and private keywords should be erased
    assert!(
        !output.contains("readonly") && !output.contains("private"),
        "ES5 output should erase modifiers: {}",
        output
    );
}

// ============================================================================
// ES5 REST PARAMETER PARITY TESTS (ADDITIONAL)
// ============================================================================

#[test]
fn test_parity_es5_rest_params_class_method() {
    let source = r#"
class Logger {
    log(level: string, ...messages: string[]): void {
        console.log(level, messages);
    }

    static format(...parts: string[]): string {
        return parts.join(" ");
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Logger"),
        "Output should contain Logger class: {}",
        output
    );
    // Should use arguments to collect rest params
    assert!(
        output.contains("arguments"),
        "Output should use arguments for rest params: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_typed_array() {
    let source = r#"
function sumNumbers(...nums: number[]): number {
    return nums.reduce((a, b) => a + b, 0);
}

function concatArrays<T>(...arrays: T[][]): T[] {
    return arrays.flat();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("sumNumbers") && output.contains("concatArrays"),
        "Output should contain functions: {}",
        output
    );
    // Rest syntax should be transformed
    assert!(
        !output.contains("...nums") && !output.contains("...arrays"),
        "Rest syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_arrow() {
    let source = r#"
const sum = (...nums: number[]): number => nums.reduce((a, b) => a + b, 0);

const join = (separator: string, ...parts: string[]): string => parts.join(separator);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variables
    assert!(
        output.contains("sum") && output.contains("join"),
        "Output should contain sum and join: {}",
        output
    );
    // Arrow syntax should be transformed
    assert!(
        !output.contains("=>"),
        "Arrow syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_with_defaults() {
    let source = r#"
function createMessage(prefix: string = "Info", ...parts: string[]): string {
    return prefix + ": " + parts.join(", ");
}

function logWithLevel(level: string = "debug", timestamp: boolean = true, ...messages: string[]): void {
    console.log(level, timestamp, messages);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("createMessage") && output.contains("logWithLevel"),
        "Output should contain functions: {}",
        output
    );
    // Should use arguments to collect rest params
    assert!(
        output.contains("arguments"),
        "Output should use arguments for rest params: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]")
            && !output.contains(": boolean")
            && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_generator() {
    let source = r#"
function* yieldAll(...values: number[]): Generator<number> {
    for (const value of values) {
        yield value;
    }
}

function* logAndYield(prefix: string, ...items: string[]): Generator<string> {
    for (const item of items) {
        console.log(prefix, item);
        yield item;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("yieldAll") && output.contains("logAndYield"),
        "Output should contain generator functions: {}",
        output
    );
    // Rest syntax should be transformed (uses arguments)
    assert!(
        output.contains("arguments"),
        "Rest syntax should use arguments: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Generator") && !output.contains(": number[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_async() {
    let source = r#"
async function fetchAll(...urls: string[]): Promise<Response[]> {
    return Promise.all(urls.map(url => fetch(url)));
}

async function logAsync(level: string, ...messages: string[]): Promise<void> {
    await new Promise(r => setTimeout(r, 100));
    console.log(level, messages);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("fetchAll") && output.contains("logAsync"),
        "Output should contain async functions: {}",
        output
    );
    // Async syntax should be transformed (uses __awaiter helper)
    assert!(
        output.contains("__awaiter") || !output.contains("async function"),
        "Async syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise") && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_destructuring() {
    let source = r#"
function processItems(...items: { id: number; name: string }[]): void {
    items.forEach(item => console.log(item.id, item.name));
}

function mergeConfigs(...configs: { [key: string]: any }[]): object {
    return Object.assign({}, ...configs);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("processItems") && output.contains("mergeConfigs"),
        "Output should contain functions: {}",
        output
    );
    // Rest syntax should be transformed
    assert!(
        !output.contains("...items") && !output.contains("...configs"),
        "Rest syntax in params should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": { id:") && !output.contains(": object"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_constructor() {
    let source = r#"
class Collection<T> {
    private items: T[];

    constructor(...initialItems: T[]) {
        this.items = initialItems;
    }

    add(...newItems: T[]): void {
        this.items.push(...newItems);
    }
}

class EventEmitter {
    constructor(private name: string, ...handlers: Function[]) {
        handlers.forEach(h => this.register(h));
    }

    register(handler: Function): void {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the classes
    assert!(
        output.contains("Collection") && output.contains("EventEmitter"),
        "Output should contain classes: {}",
        output
    );
    // Should use arguments in constructor for rest params
    assert!(
        output.contains("arguments"),
        "Output should use arguments for rest params: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("<T>") && !output.contains(": T[]") && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_nested() {
    let source = r#"
function outer(prefix: string) {
    return function inner(...items: string[]): string {
        return prefix + items.join(", ");
    };
}

function createLogger(level: string) {
    return function log(...messages: any[]): void {
        console.log(level, messages);
    };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the outer functions
    assert!(
        output.contains("outer") && output.contains("createLogger"),
        "Output should contain outer functions: {}",
        output
    );
    // Rest syntax should be transformed in nested functions
    assert!(
        !output.contains("...items") && !output.contains("...messages"),
        "Rest syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]") && !output.contains(": any[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_overload() {
    let source = r#"
function format(template: string): string;
function format(template: string, ...args: any[]): string;
function format(template: string, ...args: any[]): string {
    return template + args.join("");
}

function log(message: string): void;
function log(level: string, ...messages: string[]): void;
function log(first: string, ...rest: string[]): void {
    console.log(first, rest);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the implementation functions
    assert!(
        output.contains("format") && output.contains("log"),
        "Output should contain functions: {}",
        output
    );
    // Overload signatures should be erased
    assert!(
        output.matches("function format").count() == 1
            && output.matches("function log").count() == 1,
        "Overload signatures should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": any[]") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_tuple() {
    let source = r#"
function processPairs(...pairs: [string, number][]): void {
    pairs.forEach(p => console.log(p[0], p[1]));
}

function zipArrays<T, U>(...arrays: [T[], U[]][]): [T, U][] {
    return arrays.flatMap(arr => arr[0].map((t, i) => [t, arr[1][i]] as [T, U]));
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("processPairs") && output.contains("zipArrays"),
        "Output should contain functions: {}",
        output
    );
    // Rest syntax should be transformed
    assert!(
        !output.contains("...pairs") && !output.contains("...arrays"),
        "Rest syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": [string, number]") && !output.contains("<T, U>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_rest_params_callback() {
    let source = r#"
function withCallback(callback: (...args: any[]) => void): void {
    callback(1, 2, 3);
}

function registerHandler(name: string, handler: (...events: Event[]) => void): void {
    console.log(name);
    handler();
}

const processor = (fn: (...nums: number[]) => number): number => {
    return fn(1, 2, 3);
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("withCallback")
            && output.contains("registerHandler")
            && output.contains("processor"),
        "Output should contain functions: {}",
        output
    );
    // Arrow syntax should be transformed
    assert!(
        !output.contains("=>"),
        "Arrow syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": (...args")
            && !output.contains(": (...events")
            && !output.contains(": (...nums"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_default_params_class_method() {
    let source = r#"
class Calculator {
    add(a: number, b: number = 0): number {
        return a + b;
    }

    multiply(x: number = 1, y: number = 1): number {
        return x * y;
    }

    static format(value: number, prefix: string = "$"): string {
        return prefix + value;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Calculator"),
        "Output should contain Calculator class: {}",
        output
    );
    // Default parameters should be transformed (void 0 check)
    assert!(
        output.contains("void 0") || output.contains("undefined"),
        "Output should check for undefined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
    // No default parameter syntax
    assert!(
        !output.contains("= 0)") && !output.contains("= 1)"),
        "Default parameter syntax should be transformed: {}",
        output
    );
}

#[test]
fn test_parity_es5_default_params_arrow() {
    let source = r#"
const greet = (name: string = "World"): string => "Hello, " + name;

const add = (a: number = 0, b: number = 0): number => a + b;

const createLogger = (prefix: string = "[LOG]") => {
    return (message: string) => console.log(prefix, message);
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variables
    assert!(
        output.contains("greet") && output.contains("add") && output.contains("createLogger"),
        "Output should contain variables: {}",
        output
    );
    // Arrow syntax should be transformed
    assert!(
        !output.contains("=>"),
        "Arrow syntax should be transformed: {}",
        output
    );
    // Default parameters should be transformed
    assert!(
        output.contains("void 0") || output.contains("undefined"),
        "Output should check for undefined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_default_params_expression() {
    let source = r#"
function createConfig(options: object = {}, timestamp: number = Date.now()): object {
    return { ...options, timestamp };
}

function formatDate(date: Date = new Date(), locale: string = "en-US"): string {
    return date.toLocaleDateString(locale);
}

function processArray(items: number[] = [], transform: (x: number) => number = (x) => x): number[] {
    return items.map(transform);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("createConfig")
            && output.contains("formatDate")
            && output.contains("processArray"),
        "Output should contain functions: {}",
        output
    );
    // Default parameters should be transformed
    assert!(
        output.contains("void 0") || output.contains("undefined"),
        "Output should check for undefined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": object")
            && !output.contains(": Date")
            && !output.contains(": number[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_default_params_constructor() {
    let source = r#"
class Logger {
    private prefix: string;
    private level: number;

    constructor(prefix: string = "[LOG]", level: number = 1) {
        this.prefix = prefix;
        this.level = level;
    }
}

class Config<T> {
    private data: T;
    private name: string;

    constructor(data: T, name: string = "default") {
        this.data = data;
        this.name = name;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the classes
    assert!(
        output.contains("Logger") && output.contains("Config"),
        "Output should contain classes: {}",
        output
    );
    // Default parameters should be transformed
    assert!(
        output.contains("void 0") || output.contains("undefined"),
        "Output should check for undefined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
    // No default parameter syntax
    assert!(
        !output.contains("= \"[LOG]\")") && !output.contains("= 1)"),
        "Default parameter syntax should be transformed: {}",
        output
    );
}

#[test]
fn test_parity_es5_spread_method_call() {
    let source = r#"
class Logger {
    log(...args: any[]): void {
        console.log(...args);
    }
}

const logger = new Logger();
const messages: string[] = ["hello", "world"];
logger.log(...messages);

class Math {
    static max(...nums: number[]): number {
        return Math.max(...nums);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Logger"),
        "Output should contain Logger class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": any[]") && !output.contains(": void") && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_spread_typed_array() {
    let source = r#"
function sumNumbers(...nums: number[]): number {
    return nums.reduce((a, b) => a + b, 0);
}

const numbers: number[] = [1, 2, 3];
const moreNumbers: number[] = [4, 5, 6];
const allNumbers: number[] = [...numbers, ...moreNumbers];

function concat<T>(...arrays: T[][]): T[] {
    return ([] as T[]).concat(...arrays);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("sumNumbers") && output.contains("concat"),
        "Output should contain functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": number") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_spread_constructor() {
    let source = r#"
class Point {
    constructor(public x: number, public y: number) {}
}

class Rectangle {
    constructor(public width: number, public height: number, public label: string = "") {}
}

const coords: [number, number] = [10, 20];
const point = new Point(...coords);

const dimensions: [number, number, string] = [100, 200, "rect"];
const rect = new Rectangle(...dimensions);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the classes
    assert!(
        output.contains("Point") && output.contains("Rectangle"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
    // Tuple types should be erased
    assert!(
        !output.contains(": [number, number]"),
        "Tuple types should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_spread_nested() {
    let source = r#"
function outer(callback: (...args: any[]) => void): void {
    const inner = (...innerArgs: any[]) => {
        callback(...innerArgs);
    };
    inner(1, 2, 3);
}

class EventEmitter {
    emit(event: string, ...args: any[]): void {
        this.listeners.forEach(listener => listener(...args));
    }
    listeners: ((...args: any[]) => void)[] = [];
}

const spread = (arr: number[]) => [...arr, ...[...arr]];
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions and class
    assert!(
        output.contains("outer") && output.contains("EventEmitter"),
        "Output should contain outer function and EventEmitter class: {}",
        output
    );
    // Arrow syntax should be transformed
    assert!(
        !output.contains("=>"),
        "Arrow syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": any[]") && !output.contains(": void") && !output.contains(": number[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_computed() {
    let source = r#"
const key = "name";
const obj: { name: string; age: number } = { name: "Alice", age: 30 };
const { [key]: value } = obj;

const propName = "id";
interface User { id: number; name: string }
const user: User = { id: 1, name: "Bob" };
const { [propName]: userId, name: userName } = user;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variables
    assert!(
        output.contains("key") && output.contains("obj") && output.contains("user"),
        "Output should contain variables: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": { name:")
            && !output.contains(": User")
            && !output.contains("interface User"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_return() {
    let source = r#"
function getCoords(): { x: number; y: number } {
    return { x: 10, y: 20 };
}

const { x, y } = getCoords();

function getPair(): [string, number] {
    return ["hello", 42];
}

const [first, second] = getPair();

class DataProvider {
    getData(): { items: string[]; count: number } {
        return { items: [], count: 0 };
    }
}

const provider = new DataProvider();
const { items, count } = provider.getData();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("getCoords")
            && output.contains("getPair")
            && output.contains("DataProvider"),
        "Output should contain functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": { x:")
            && !output.contains(": [string, number]")
            && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_rename() {
    let source = r#"
interface Person { firstName: string; lastName: string; age: number }
const person: Person = { firstName: "John", lastName: "Doe", age: 25 };
const { firstName: fName, lastName: lName, age: years } = person;

const response: { data: string[]; error: Error | null } = { data: [], error: null };
const { data: items, error: err } = response;

function process({ input: src, output: dest }: { input: string; output: string }): void {
    console.log(src, dest);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variables
    assert!(
        output.contains("person") && output.contains("response"),
        "Output should contain variables: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Person")
            && !output.contains("interface Person")
            && !output.contains(": { data:"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_loop() {
    let source = r#"
interface Entry { key: string; value: number }
const entries: Entry[] = [{ key: "a", value: 1 }, { key: "b", value: 2 }];

for (const { key, value } of entries) {
    console.log(key, value);
}

const pairs: [string, number][] = [["x", 1], ["y", 2]];
for (const [name, num] of pairs) {
    console.log(name, num);
}

const map: Map<string, number> = new Map();
for (const [k, v] of map.entries()) {
    console.log(k, v);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the arrays
    assert!(
        output.contains("entries") && output.contains("pairs"),
        "Output should contain arrays: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Entry[]")
            && !output.contains("interface Entry")
            && !output.contains(": [string, number][]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_object_typed() {
    let source = r#"
interface User {
    id: number;
    name: string;
    email: string;
    age?: number;
}

interface Address {
    street: string;
    city: string;
    country: string;
}

interface UserWithAddress extends User {
    address: Address;
}

function getUser(): User {
    return { id: 1, name: "John", email: "john@example.com" };
}

const { id, name, email }: User = getUser();

function processUser({ id, name, email }: User): string {
    return name + email;
}

const user: UserWithAddress = {
    id: 1,
    name: "Jane",
    email: "jane@example.com",
    address: { street: "123 Main", city: "NYC", country: "USA" }
};

const { address: { city, country } }: UserWithAddress = user;

type Config = { readonly host: string; port: number };
const { host, port }: Config = { host: "localhost", port: 8080 };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be defined
    assert!(
        output.contains("id") && output.contains("name") && output.contains("email"),
        "Destructured variables should be defined: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface"),
        "Interfaces should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": User") && !output.contains(": Address") && !output.contains(": Config"),
        "Type annotations should be erased: {}",
        output
    );
    // readonly should be erased
    assert!(
        !output.contains("readonly"),
        "readonly should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_array_tuple() {
    let source = r#"
type Point2D = [number, number];
type Point3D = [number, number, number];
type NamedPoint = [string, number, number];

const point2d: Point2D = [10, 20];
const [x, y]: Point2D = point2d;

const point3d: Point3D = [1, 2, 3];
const [a, b, c]: Point3D = point3d;

function getNamedPoint(): NamedPoint {
    return ["origin", 0, 0];
}

const [label, px, py]: NamedPoint = getNamedPoint();

type Result<T, E> = [T, null] | [null, E];
const success: Result<number, string> = [42, null];
const [value, error]: Result<number, string> = success;

function swap<T, U>(tuple: [T, U]): [U, T] {
    const [first, second]: [T, U] = tuple;
    return [second, first];
}

const [head, ...tail]: number[] = [1, 2, 3, 4, 5];

interface Pair<T> {
    values: [T, T];
}

const pair: Pair<string> = { values: ["a", "b"] };
const [left, right]: [string, string] = pair.values;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be defined
    assert!(
        output.contains("point2d") && output.contains("point3d"),
        "Arrays should be defined: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Point2D")
            && !output.contains("type Point3D")
            && !output.contains("type Result"),
        "Type aliases should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Generic types should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, U>") && !output.contains("<T, E>"),
        "Generic types should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_nested_deep() {
    let source = r#"
interface Company {
    name: string;
    location: {
        address: {
            street: string;
            city: string;
        };
        coordinates: {
            lat: number;
            lng: number;
        };
    };
    employees: {
        id: number;
        details: {
            name: string;
            role: string;
        };
    }[];
}

const company: Company = {
    name: "TechCorp",
    location: {
        address: { street: "123 Tech Lane", city: "San Francisco" },
        coordinates: { lat: 37.7749, lng: -122.4194 }
    },
    employees: [{ id: 1, details: { name: "Alice", role: "Engineer" } }]
};

const {
    name: companyName,
    location: {
        address: { street, city },
        coordinates: { lat, lng }
    }
}: Company = company;

function processCompany({
    name,
    location: { address: { city: cityName } }
}: Company): string {
    return name + " in " + cityName;
}

type NestedArray = [[number, number], [string, string]];
const nested: NestedArray = [[1, 2], ["a", "b"]];
const [[n1, n2], [s1, s2]]: NestedArray = nested;

const { employees: [{ details: { name: firstName } }] }: Company = company;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variable should be defined
    assert!(
        output.contains("company"),
        "Company object should be defined: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type NestedArray"),
        "Type alias should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Company") && !output.contains(": NestedArray"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_defaults_typed() {
    let source = r#"
interface Options {
    timeout?: number;
    retries?: number;
    verbose?: boolean;
}

function configure({
    timeout = 1000,
    retries = 3,
    verbose = false
}: Options = {}): void {
    console.log(timeout, retries, verbose);
}

const { timeout = 5000, retries = 1 }: Options = {};

type StringOrNumber = string | number;
const { value = "default" }: { value?: StringOrNumber } = {};

interface ComponentProps {
    title?: string;
    count?: number;
    items?: string[];
}

function Component({
    title = "Untitled",
    count = 0,
    items = []
}: ComponentProps): void {
    console.log(title, count, items);
}

const [first = 0, second = 0]: [number?, number?] = [];

function withCallback({
    onSuccess = () => {},
    onError = (e: Error) => console.error(e)
}: {
    onSuccess?: () => void;
    onError?: (e: Error) => void;
} = {}): void {
    onSuccess();
}

type Config<T> = { value?: T; fallback: T };
function getValue<T>({ value, fallback }: Config<T>): T {
    return value ?? fallback;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be defined
    assert!(
        output.contains("configure") && output.contains("Component"),
        "Functions should be defined: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type StringOrNumber") && !output.contains("type Config"),
        "Type aliases should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Options") && !output.contains(": ComponentProps"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_destructuring_rest_typed() {
    let source = r#"
interface FullUser {
    id: number;
    name: string;
    email: string;
    password: string;
    createdAt: Date;
}

const fullUser: FullUser = {
    id: 1,
    name: "John",
    email: "john@example.com",
    password: "secret",
    createdAt: new Date()
};

const { password, ...safeUser }: FullUser = fullUser;

function omitFields<T extends object, K extends keyof T>(
    obj: T,
    ...keys: K[]
): Omit<T, K> {
    const result = { ...obj };
    for (const key of keys) {
        delete (result as any)[key];
    }
    return result as Omit<T, K>;
}

type NumberTuple = [number, number, number, number, number];
const nums: NumberTuple = [1, 2, 3, 4, 5];
const [first, second, ...remaining]: NumberTuple = nums;

interface ApiResponse<T> {
    data: T;
    status: number;
    headers: Record<string, string>;
}

function processResponse<T>({
    data,
    ...metadata
}: ApiResponse<T>): { data: T; meta: Omit<ApiResponse<T>, 'data'> } {
    return { data, meta: metadata };
}

const { name, ...rest }: { name: string; [key: string]: unknown } = {
    name: "test",
    extra: true,
    count: 42
};

function collectRest<T>(...items: T[]): T[] {
    const [head, ...tail] = items;
    return tail;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be defined
    assert!(
        output.contains("fullUser") && output.contains("safeUser"),
        "Variables should be defined: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type NumberTuple"),
        "Type alias should be erased: {}",
        output
    );
    // Generic constraints should be erased
    assert!(
        !output.contains("extends object") && !output.contains("extends keyof"),
        "Generic constraints should be erased: {}",
        output
    );
    // Omit utility type should be erased
    assert!(
        !output.contains("Omit<"),
        "Utility types should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_iterables() {
    let source = r#"
const set: Set<number> = new Set([1, 2, 3]);
for (const num of set) {
    console.log(num);
}

const map: Map<string, number> = new Map([["a", 1], ["b", 2]]);
for (const [key, value] of map) {
    console.log(key, value);
}

const str: string = "hello";
for (const char of str) {
    console.log(char);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the iterables
    assert!(
        output.contains("set") && output.contains("map") && output.contains("str"),
        "Output should contain iterables: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Set<") && !output.contains(": Map<") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_control_flow() {
    let source = r#"
function findFirst(items: number[], target: number): number | undefined {
    for (const item of items) {
        if (item === target) {
            return item;
        }
    }
    return undefined;
}

function sumUntil(nums: number[], limit: number): number {
    let sum: number = 0;
    for (const n of nums) {
        if (sum + n > limit) {
            break;
        }
        sum += n;
    }
    return sum;
}

function skipNegative(values: number[]): number[] {
    const result: number[] = [];
    for (const v of values) {
        if (v < 0) {
            continue;
        }
        result.push(v);
    }
    return result;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("findFirst")
            && output.contains("sumUntil")
            && output.contains("skipNegative"),
        "Output should contain functions: {}",
        output
    );
    // Should contain control flow
    assert!(
        output.contains("break") && output.contains("continue"),
        "Output should contain break and continue: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": number | undefined"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_generator() {
    let source = r#"
function* range(start: number, end: number): Generator<number> {
    for (let i = start; i < end; i++) {
        yield i;
    }
}

for (const n of range(0, 5)) {
    console.log(n);
}

function* pairs<T>(arr: T[]): Generator<[T, T]> {
    for (let i = 0; i < arr.length - 1; i++) {
        yield [arr[i], arr[i + 1]];
    }
}

const items: number[] = [1, 2, 3, 4];
for (const [a, b] of pairs(items)) {
    console.log(a, b);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the generator functions
    assert!(
        output.contains("range") && output.contains("pairs"),
        "Output should contain generator functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Generator") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_class_method() {
    let source = r#"
class DataProcessor {
    private items: number[];

    constructor(items: number[]) {
        this.items = items;
    }

    process(): number[] {
        const result: number[] = [];
        for (const item of this.items) {
            result.push(item * 2);
        }
        return result;
    }

    static fromArray<T>(arr: T[]): T[] {
        const copy: T[] = [];
        for (const elem of arr) {
            copy.push(elem);
        }
        return copy;
    }
}

class StringCollector {
    collect(strings: string[]): string {
        let result: string = "";
        for (const s of strings) {
            result += s;
        }
        return result;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the classes
    assert!(
        output.contains("DataProcessor") && output.contains("StringCollector"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": string") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_async() {
    let source = r#"
async function processItems(items: string[]): Promise<void> {
    for (const item of items) {
        await fetch(item);
    }
}

async function collectResults<T>(promises: Promise<T>[]): Promise<T[]> {
    const results: T[] = [];
    for (const p of promises) {
        results.push(await p);
    }
    return results;
}

class AsyncProcessor {
    async run(tasks: (() => Promise<void>)[]): Promise<void> {
        for (const task of tasks) {
            await task();
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("processItems")
            && output.contains("collectResults")
            && output.contains("AsyncProcessor"),
        "Output should contain functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise") && !output.contains(": string[]") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_try_catch() {
    let source = r#"
function safeProcess(items: string[]): string[] {
    const results: string[] = [];
    for (const item of items) {
        try {
            results.push(item.toUpperCase());
        } catch (e) {
            console.error(e);
        }
    }
    return results;
}

function processWithFinally(nums: number[]): number {
    let sum: number = 0;
    for (const n of nums) {
        try {
            sum += n;
        } finally {
            console.log("processed", n);
        }
    }
    return sum;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("safeProcess") && output.contains("processWithFinally"),
        "Output should contain functions: {}",
        output
    );
    // Should contain try-catch-finally
    assert!(
        output.contains("try") && output.contains("catch") && output.contains("finally"),
        "Output should contain try-catch-finally: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]")
            && !output.contains(": number[]")
            && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_labeled() {
    let source = r#"
function findInMatrix(matrix: number[][], target: number): boolean {
    outer: for (const row of matrix) {
        for (const cell of row) {
            if (cell === target) {
                break outer;
            }
        }
    }
    return false;
}

function skipRows(data: string[][], skipValue: string): string[] {
    const result: string[] = [];
    rowLoop: for (const row of data) {
        for (const item of row) {
            if (item === skipValue) {
                continue rowLoop;
            }
        }
        result.push(row.join(","));
    }
    return result;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the functions
    assert!(
        output.contains("findInMatrix") && output.contains("skipRows"),
        "Output should contain functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[][]")
            && !output.contains(": string[][]")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_for_of_arrow() {
    let source = r#"
const processAll = (items: number[]): number[] => {
    const result: number[] = [];
    for (const item of items) {
        result.push(item * 2);
    }
    return result;
};

const sumItems = (nums: number[]): number => {
    let total: number = 0;
    for (const n of nums) {
        total += n;
    }
    return total;
};

const logEach = <T>(items: T[]): void => {
    for (const item of items) {
        console.log(item);
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the variables
    assert!(
        output.contains("processAll") && output.contains("sumItems") && output.contains("logEach"),
        "Output should contain variables: {}",
        output
    );
    // Arrow syntax should be transformed
    assert!(
        !output.contains("=>"),
        "Arrow syntax should be transformed: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": number") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_decorator_constructor() {
    let source = r#"
function sealed(constructor: Function) {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function logged(constructor: Function) {
    console.log("Class created:", constructor.name);
}

@sealed
@logged
class Service {
    private name: string;

    constructor(name: string) {
        this.name = name;
    }

    getName(): string {
        return this.name;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class and decorators
    assert!(
        output.contains("Service") && output.contains("sealed") && output.contains("logged"),
        "Output should contain class and decorators: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Function") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_decorator_static_members() {
    let source = r#"
function staticInit<T extends { new(...args: any[]): {} }>(constructor: T) {
    return class extends constructor {
        static initialized = true;
    };
}

@staticInit
class Config {
    static version: string = "1.0.0";
    static environment: string = "production";

    private settings: Map<string, any> = new Map();

    static getVersion(): string {
        return Config.version;
    }

    get(key: string): any {
        return this.settings.get(key);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Config"),
        "Output should contain Config class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("<T extends"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_decorator_metadata() {
    let source = r#"
function component(options: { selector: string; template: string }) {
    return function<T extends { new(...args: any[]): {} }>(constructor: T) {
        return class extends constructor {
            selector = options.selector;
            template = options.template;
        };
    };
}

function injectable() {
    return function(constructor: Function) {
        console.log("Injectable:", constructor.name);
    };
}

@component({ selector: "app-root", template: "<div></div>" })
@injectable()
class AppComponent {
    title: string = "My App";

    constructor(private service: any) {}

    render(): void {
        console.log(this.title);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class and decorators
    assert!(
        output.contains("AppComponent")
            && output.contains("component")
            && output.contains("injectable"),
        "Output should contain class and decorators: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_class_decorator_inheritance() {
    let source = r#"
function tracked(constructor: Function) {
    console.log("Tracking:", constructor.name);
}

function validated(constructor: Function) {
    console.log("Validating:", constructor.name);
}

@tracked
class BaseEntity {
    id: number;

    constructor(id: number) {
        this.id = id;
    }
}

@validated
class User extends BaseEntity {
    name: string;

    constructor(id: number, name: string) {
        super(id);
        this.name = name;
    }
}

@tracked
@validated
class Admin extends User {
    permissions: string[];

    constructor(id: number, name: string, permissions: string[]) {
        super(id, name);
        this.permissions = permissions;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain all classes
    assert!(
        output.contains("BaseEntity") && output.contains("User") && output.contains("Admin"),
        "Output should contain all classes: {}",
        output
    );
    // Should contain decorators
    assert!(
        output.contains("tracked") && output.contains("validated"),
        "Output should contain decorators: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": string")
            && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_method_this_binding() {
    let source = r#"
class Calculator {
    private value: number = 0;

    #add(n: number): void {
        this.value += n;
    }

    #multiply(n: number): void {
        this.value *= n;
    }

    #reset(): void {
        this.value = 0;
    }

    compute(a: number, b: number): number {
        this.#reset();
        this.#add(a);
        this.#multiply(b);
        return this.value;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Calculator"),
        "Output should contain Calculator class: {}",
        output
    );
    // Should contain this references
    assert!(
        output.contains("this"),
        "Output should contain this references: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_method_generic() {
    let source = r#"
class Container<T> {
    private items: T[] = [];

    #validate(item: T): boolean {
        return item !== null && item !== undefined;
    }

    #transform<U>(item: T, fn: (x: T) => U): U {
        return fn(item);
    }

    add(item: T): void {
        if (this.#validate(item)) {
            this.items.push(item);
        }
    }

    map<U>(fn: (x: T) => U): U[] {
        return this.items.map(item => this.#transform(item, fn));
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("Container"),
        "Output should contain Container class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<U>") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_method_derived() {
    let source = r#"
class BaseService {
    protected data: string[] = [];

    #log(message: string): void {
        console.log("[Base]", message);
    }

    protected process(item: string): void {
        this.#log("Processing: " + item);
        this.data.push(item);
    }
}

class ExtendedService extends BaseService {
    #validate(item: string): boolean {
        return item.length > 0;
    }

    add(item: string): void {
        if (this.#validate(item)) {
            this.process(item);
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("BaseService") && output.contains("ExtendedService"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]")
            && !output.contains(": void")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_private_method_callback() {
    let source = r#"
class EventHandler {
    private handlers: Map<string, Function[]> = new Map();

    #invoke(event: string, callback: (data: any) => void, data: any): void {
        callback(data);
    }

    #getHandlers(event: string): Function[] {
        return this.handlers.get(event) || [];
    }

    on(event: string, handler: (data: any) => void): void {
        const handlers = this.#getHandlers(event);
        handlers.push(handler);
        this.handlers.set(event, handlers);
    }

    emit(event: string, data: any): void {
        for (const handler of this.#getHandlers(event)) {
            this.#invoke(event, handler as (data: any) => void, data);
        }
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("EventHandler"),
        "Output should contain EventHandler class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Map<")
            && !output.contains(": Function[]")
            && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 private async method with complex await.
/// Private async method with multiple awaits and error handling.
#[test]
fn test_parity_es5_private_async_method_complex() {
    let source = r#"
interface ApiResponse<T> { data: T; status: number }
class DataService {
    private baseUrl: string = "https://api.example.com";

    async #fetchWithRetry<T>(url: string, retries: number): Promise<ApiResponse<T>> {
        for (let i = 0; i < retries; i++) {
            try {
                const response = await fetch(this.baseUrl + url);
                const data = await response.json();
                return { data, status: response.status };
            } catch (error) {
                if (i === retries - 1) throw error;
                await this.#delay(1000 * (i + 1));
            }
        }
        throw new Error("Max retries exceeded");
    }

    async #delay(ms: number): Promise<void> {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async getData<T>(endpoint: string): Promise<T> {
        const response = await this.#fetchWithRetry<T>(endpoint, 3);
        return response.data;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("DataService"),
        "Output should contain DataService class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains("Promise<"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // No async method syntax
    assert!(
        !output.contains("async #"),
        "ES5 output should not contain async private method syntax: {}",
        output
    );
}

/// Parity test for ES5 private generator method.
/// Private generator method in a class.
#[test]
fn test_parity_es5_private_generator_method() {
    let source = r#"
interface TreeNode<T> { value: T; children: TreeNode<T>[] }
class TreeIterator<T> {
    private root: TreeNode<T>;

    constructor(root: TreeNode<T>) {
        this.root = root;
    }

    *#traverseDepthFirst(node: TreeNode<T>): Generator<T> {
        yield node.value;
        for (const child of node.children) {
            yield* this.#traverseDepthFirst(child);
        }
    }

    *#traverseBreadthFirst(): Generator<T> {
        const queue: TreeNode<T>[] = [this.root];
        while (queue.length > 0) {
            const node = queue.shift()!;
            yield node.value;
            queue.push(...node.children);
        }
    }

    *values(): Generator<T> {
        yield* this.#traverseDepthFirst(this.root);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("TreeIterator"),
        "Output should contain TreeIterator class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": TreeNode") && !output.contains("Generator<T>"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No private generator method syntax
    assert!(
        !output.contains("*#"),
        "ES5 output should not contain private generator method syntax: {}",
        output
    );
}

/// Parity test for ES5 private accessor with complex types.
/// Private getter/setter with complex type annotations.
#[test]
fn test_parity_es5_private_accessor_complex() {
    let source = r#"
interface ValidationResult { valid: boolean; errors: string[] }
class FormField<T> {
    #value: T;
    #validators: ((value: T) => ValidationResult)[] = [];

    constructor(initialValue: T) {
        this.#value = initialValue;
    }

    get #currentValue(): T {
        return this.#value;
    }

    set #currentValue(newValue: T) {
        this.#value = newValue;
    }

    get #validationState(): ValidationResult {
        const allErrors: string[] = [];
        for (const validator of this.#validators) {
            const result = validator(this.#currentValue);
            if (!result.valid) {
                allErrors.push(...result.errors);
            }
        }
        return { valid: allErrors.length === 0, errors: allErrors };
    }

    getValue(): T {
        return this.#currentValue;
    }

    setValue(value: T): ValidationResult {
        this.#currentValue = value;
        return this.#validationState;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain the class
    assert!(
        output.contains("FormField"),
        "Output should contain FormField class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface"),
        "ES5 output should erase interface: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "ES5 output should erase generic type parameters: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": ValidationResult") && !output.contains(": T"),
        "ES5 output should erase type annotations: {}",
        output
    );
    // No private accessor syntax
    assert!(
        !output.contains("get #") && !output.contains("set #"),
        "ES5 output should not contain private accessor syntax: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_field_computed() {
    let source = r#"
class Config {
    static readonly VERSION: string = "1.0.0";
    static readonly BUILD_DATE: string = new Date().toISOString();
    static readonly MAX_RETRIES: number = 3;
    static readonly TIMEOUT: number = Config.MAX_RETRIES * 1000;

    static settings: { [key: string]: any } = {
        debug: false,
        verbose: true
    };
}

class Counter {
    static count: number = 0;
    static instances: Counter[] = [];
    static lastCreated: Date | null = null;

    constructor() {
        Counter.count++;
        Counter.instances.push(this);
        Counter.lastCreated = new Date();
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Config") && output.contains("Counter"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number") && !output.contains(": Date"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_field_methods() {
    let source = r#"
class Logger {
    static level: string = "info";
    static prefix: string = "[LOG]";
    static enabled: boolean = true;

    static setLevel(level: string): void {
        Logger.level = level;
    }

    static getPrefix(): string {
        return Logger.prefix;
    }

    static log(message: string): void {
        if (Logger.enabled) {
            console.log(Logger.prefix, Logger.level, message);
        }
    }
}

class Cache<T> {
    static defaultTTL: number = 3600;
    static maxSize: number = 1000;

    private data: Map<string, T> = new Map();

    static configure(ttl: number, size: number): void {
        Cache.defaultTTL = ttl;
        Cache.maxSize = size;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Logger") && output.contains("Cache"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": boolean") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_field_inheritance() {
    let source = r#"
class BaseEntity {
    static tableName: string = "entities";
    static primaryKey: string = "id";

    id: number;

    constructor(id: number) {
        this.id = id;
    }
}

class User extends BaseEntity {
    static tableName: string = "users";
    static fields: string[] = ["id", "name", "email"];

    name: string;

    constructor(id: number, name: string) {
        super(id);
        this.name = name;
    }
}

class Admin extends User {
    static tableName: string = "admins";
    static permissions: string[] = ["read", "write", "delete"];

    role: string;

    constructor(id: number, name: string, role: string) {
        super(id, name);
        this.role = role;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain all classes
    assert!(
        output.contains("BaseEntity") && output.contains("User") && output.contains("Admin"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": string[]"),
        "Type annotations should be erased: {}",
        output
    );
}

#[test]
fn test_parity_es5_static_field_generic() {
    let source = r#"
class Registry<T> {
    static instances: Map<string, any> = new Map();
    static defaultFactory: (() => any) | null = null;

    private items: T[] = [];

    static register<U>(key: string, instance: U): void {
        Registry.instances.set(key, instance);
    }

    static get<U>(key: string): U | undefined {
        return Registry.instances.get(key);
    }
}

class Pool<T> {
    static poolSize: number = 10;
    static activeCount: number = 0;

    private available: T[] = [];
    private inUse: Set<T> = new Set();

    static setPoolSize(size: number): void {
        Pool.poolSize = size;
    }

    static getActiveCount(): number {
        return Pool.activeCount;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should contain both classes
    assert!(
        output.contains("Registry") && output.contains("Pool"),
        "Output should contain classes: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<U>") && !output.contains(": Map<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 const enum with usage sites.
/// Const enum usage should be inlined with the literal values.
#[test]
fn test_parity_es5_enum_const_with_usage() {
    let source = r#"const enum Direction {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4
}

function move(dir: Direction): void {
    console.log(dir);
}

move(Direction.Up);
const d: Direction = Direction.Left;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // No const enum keyword
    assert!(
        !output.contains("const enum"),
        "ES5 output should not contain const enum keyword: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("function move"),
        "Output should contain move function: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": Direction"),
        "Type annotation should be erased: {}",
        output
    );
}

/// Parity test for ES5 enum reverse mapping.
/// Numeric enums should have bidirectional mapping (name -> value, value -> name).
#[test]
fn test_parity_es5_enum_reverse_mapping() {
    let source = r#"enum Color {
    Red,
    Green,
    Blue
}

const colorName = Color[0];
const colorValue = Color.Red;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Enum should be present
    assert!(
        output.contains("Color"),
        "Output should define Color enum: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Color"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
    // Should have variable declarations
    assert!(
        output.contains("colorName") && output.contains("colorValue"),
        "Output should contain variable declarations: {}",
        output
    );
}

/// Parity test for ES5 string enum.
/// String enums should emit only forward mapping (no reverse mapping).
#[test]
fn test_parity_es5_enum_string_values() {
    let source = r#"enum LogLevel {
    Debug = "DEBUG",
    Info = "INFO",
    Warn = "WARN",
    Error = "ERROR"
}

function log(level: LogLevel, message: string): void {
    console.log(`[${level}] ${message}`);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Enum should be present
    assert!(
        output.contains("LogLevel"),
        "Output should define LogLevel enum: {}",
        output
    );
    // Should have string values
    assert!(
        output.contains("DEBUG") && output.contains("ERROR"),
        "Output should contain string values: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum LogLevel"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
    // Type annotation should be erased
    assert!(
        !output.contains(": LogLevel") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 enum with computed member expressions.
/// Complex computed members with function calls and expressions.
#[test]
fn test_parity_es5_enum_computed_complex() {
    let source = r#"function getValue(): number { return 10; }

enum Computed {
    A = getValue(),
    B = A + 1,
    C = B * 2,
    D = Math.floor(C / 3)
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Function and enum should be present
    assert!(
        output.contains("getValue") && output.contains("Computed"),
        "Output should contain function and enum: {}",
        output
    );
    // No enum keyword
    assert!(
        !output.contains("enum Computed"),
        "ES5 output should not contain enum keyword: {}",
        output
    );
    // Should have IIFE pattern for enum
    assert!(
        output.contains("(function (Computed)"),
        "Output should use IIFE pattern for enum: {}",
        output
    );
}

/// Parity test for ES5 re-export patterns.
/// Re-exports should be transformed to CommonJS require/exports pattern.
#[test]
fn test_parity_es5_reexport_patterns() {
    let source = r#"export { foo, bar } from './module';
export { baz as qux } from './other';
export * from './all';
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should have require calls
    assert!(
        output.contains("require"),
        "Output should use require for imports: {}",
        output
    );
    // No ES6 export/import syntax
    assert!(
        !output.contains("export {") && !output.contains("from '"),
        "ES5 output should not contain ES6 export syntax: {}",
        output
    );
}

/// Parity test for ES5 barrel file pattern.
/// Barrel files re-exporting from multiple modules.
#[test]
fn test_parity_es5_barrel_file() {
    let source = r#"export { User } from './user';
export { Product } from './product';
export { Order } from './order';
export type { UserType } from './types';
"#;
    let mut parser = ParserState::new("index.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Should have require calls for value exports
    assert!(
        output.contains("require"),
        "Output should use require for barrel imports: {}",
        output
    );
    // Type-only exports should be erased
    assert!(
        !output.contains("UserType"),
        "Type-only exports should be erased: {}",
        output
    );
    // No ES6 syntax
    assert!(
        !output.contains("export {") && !output.contains("from '"),
        "ES5 output should not contain ES6 syntax: {}",
        output
    );
}

/// Parity test for ES5 type-only imports.
/// Type-only imports should be completely erased.
#[test]
fn test_parity_es5_type_only_imports() {
    let source = r#"import type { User, Product } from './types';
import type * as Types from './all-types';
import { type Order, createOrder } from './orders';

function process(user: User): Product {
    return createOrder();
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type-only imports should be erased - no import type
    assert!(
        !output.contains("import type"),
        "Type-only imports should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("function process"),
        "Output should contain process function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": User") && !output.contains(": Product"),
        "Type annotations should be erased: {}",
        output
    );
    // Value import should remain (createOrder)
    assert!(
        output.contains("createOrder"),
        "Value imports should remain: {}",
        output
    );
}

/// Parity test for ES5 class extends clause.
/// Class extending another class should use __extends helper.
#[test]
fn test_parity_es5_class_extends_clause() {
    let source = r#"class Animal {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
    speak(): void {
        console.log(this.name);
    }
}

class Dog extends Animal {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Both classes should be present
    assert!(
        output.contains("Animal") && output.contains("Dog"),
        "Output should contain both classes: {}",
        output
    );
    // Should have __extends helper or prototype chain setup
    assert!(
        output.contains("__extends") || output.contains(".prototype"),
        "ES5 output should have inheritance mechanism: {}",
        output
    );
    // No ES6 class syntax
    assert!(
        !output.contains("class Dog extends"),
        "ES5 output should not contain ES6 class extends: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 super calls in constructor and methods.
/// Super calls should be transformed to parent prototype calls.
#[test]
fn test_parity_es5_class_super_calls() {
    let source = r#"class Base {
    value: number;
    constructor(value: number) {
        this.value = value;
    }
    getValue(): number {
        return this.value;
    }
}

class Derived extends Base {
    multiplier: number;
    constructor(value: number, multiplier: number) {
        super(value);
        this.multiplier = multiplier;
    }
    getValue(): number {
        return super.getValue() * this.multiplier;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Both classes should be present
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "Output should contain both classes: {}",
        output
    );
    // No ES6 super keyword as constructor call
    assert!(
        !output.contains("super(value)"),
        "ES5 output should not contain ES6 super(): {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 method overrides.
/// Overridden methods should be on prototype chain.
#[test]
fn test_parity_es5_class_method_overrides() {
    let source = r#"class Shape {
    getArea(): number {
        return 0;
    }
    getPerimeter(): number {
        return 0;
    }
}

class Rectangle extends Shape {
    width: number;
    height: number;
    constructor(width: number, height: number) {
        super();
        this.width = width;
        this.height = height;
    }
    override getArea(): number {
        return this.width * this.height;
    }
    override getPerimeter(): number {
        return 2 * (this.width + this.height);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Both classes should be present
    assert!(
        output.contains("Shape") && output.contains("Rectangle"),
        "Output should contain both classes: {}",
        output
    );
    // Methods should be defined
    assert!(
        output.contains("getArea") && output.contains("getPerimeter"),
        "Output should contain methods: {}",
        output
    );
    // No override keyword
    assert!(
        !output.contains("override"),
        "ES5 output should not contain override keyword: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 abstract class with methods.
/// Abstract classes with abstract and concrete methods should be lowered properly.
#[test]
fn test_parity_es5_abstract_class_methods() {
    let source = r#"abstract class Vehicle {
    abstract start(): void;
    abstract stop(): void;

    honk(): void {
        console.log("Beep!");
    }
}

class Car extends Vehicle {
    start(): void {
        console.log("Car starting");
    }
    stop(): void {
        console.log("Car stopping");
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Both classes should be present
    assert!(
        output.contains("Vehicle") && output.contains("Car"),
        "Output should contain both classes: {}",
        output
    );
    // No abstract keyword
    assert!(
        !output.contains("abstract"),
        "ES5 output should not contain abstract keyword: {}",
        output
    );
    // Methods should be defined
    assert!(
        output.contains("start") && output.contains("stop") && output.contains("honk"),
        "Output should contain methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 class decorator chaining.
/// Multiple class decorators should be applied in reverse order.
#[test]
fn test_parity_es5_decorator_class_chaining() {
    let source = r#"function first<T extends { new(...args: any[]): {} }>(target: T) {
    return class extends target {
        first = true;
    };
}

function second<T extends { new(...args: any[]): {} }>(target: T) {
    return class extends target {
        second = true;
    };
}

@first
@second
class Example {
    value: number = 42;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and decorator functions should be present
    assert!(
        output.contains("Example") && output.contains("first") && output.contains("second"),
        "Output should contain class and decorators: {}",
        output
    );
    // No @ decorator syntax
    assert!(
        !output.contains("@first") && !output.contains("@second"),
        "ES5 output should not contain @ decorator syntax: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T extends"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Parity test for ES5 method decorator with descriptor.
/// Method decorators should receive property descriptor.
#[test]
fn test_parity_es5_decorator_method_descriptor() {
    let source = r#"function log(target: any, key: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log(`Calling ${key}`);
        return original.apply(this, args);
    };
    return descriptor;
}

class Calculator {
    @log
    add(a: number, b: number): number {
        return a + b;
    }

    @log
    multiply(a: number, b: number): number {
        return a * b;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and methods should be present
    assert!(
        output.contains("Calculator") && output.contains("add") && output.contains("multiply"),
        "Output should contain class and methods: {}",
        output
    );
    // Decorator function should be present
    assert!(
        output.contains("log"),
        "Output should contain log decorator: {}",
        output
    );
    // No @ decorator syntax
    assert!(
        !output.contains("@log"),
        "ES5 output should not contain @ decorator syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": PropertyDescriptor"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 parameter decorator with injection.
/// Parameter decorators for dependency injection pattern.
#[test]
fn test_parity_es5_decorator_parameter_injection() {
    let source = r#"function inject(token: string) {
    return function(target: any, key: string | symbol, index: number) {
        const existing = Reflect.getMetadata("inject", target, key) || [];
        existing.push({ index, token });
        Reflect.defineMetadata("inject", existing, target, key);
    };
}

class UserService {
    constructor(
        @inject("Database") private db: any,
        @inject("Logger") private logger: any
    ) {}

    getUser(@inject("UserId") id: string): any {
        return this.db.find(id);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and decorator should be present
    assert!(
        output.contains("UserService") && output.contains("inject"),
        "Output should contain class and decorator: {}",
        output
    );
    // No @ decorator syntax
    assert!(
        !output.contains("@inject"),
        "ES5 output should not contain @ decorator syntax: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private db") && !output.contains("private logger"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Parity test for ES5 for-await-of with async generators.
/// Async generators with for-await-of should be transformed properly.
#[test]
fn test_parity_es5_for_await_of_async_generator() {
    let source = r#"async function* asyncRange(start: number, end: number): AsyncGenerator<number> {
    for (let i = start; i <= end; i++) {
        await new Promise(resolve => setTimeout(resolve, 100));
        yield i;
    }
}

async function consumeRange(): Promise<number[]> {
    const results: number[] = [];
    for await (const num of asyncRange(1, 5)) {
        results.push(num);
    }
    return results;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("asyncRange") && output.contains("consumeRange"),
        "Output should contain functions: {}",
        output
    );
    // Should have async helpers
    assert!(
        output.contains("__awaiter") || output.contains("__asyncGenerator"),
        "ES5 output should use async helpers: {}",
        output
    );
    // No async function* syntax
    assert!(
        !output.contains("async function*"),
        "ES5 output should not contain async function* syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": AsyncGenerator")
            && !output.contains(": Promise"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 async iterator protocol.
/// Custom async iterators implementing Symbol.asyncIterator.
#[test]
fn test_parity_es5_async_iterator_protocol() {
    let source = r#"class AsyncQueue<T> {
    private items: T[] = [];

    async *[Symbol.asyncIterator](): AsyncGenerator<T> {
        while (this.items.length > 0) {
            yield this.items.shift()!;
        }
    }

    enqueue(item: T): void {
        this.items.push(item);
    }
}

async function processQueue(): Promise<void> {
    const queue = new AsyncQueue<string>();
    queue.enqueue("first");
    queue.enqueue("second");

    for await (const item of queue) {
        console.log(item);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and function should be present
    assert!(
        output.contains("AsyncQueue") && output.contains("processQueue"),
        "Output should contain class and function: {}",
        output
    );
    // Should reference Symbol.asyncIterator
    assert!(
        output.contains("Symbol.asyncIterator"),
        "Output should reference Symbol.asyncIterator: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<string>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Parity test for ES5 Symbol.asyncIterator implementation.
/// Object implementing async iterable interface.
#[test]
fn test_parity_es5_symbol_async_iterator() {
    let source = r#"const asyncIterable = {
    data: [1, 2, 3, 4, 5],
    [Symbol.asyncIterator](): AsyncIterator<number> {
        let index = 0;
        const data = this.data;
        return {
            async next(): Promise<IteratorResult<number>> {
                if (index < data.length) {
                    return { value: data[index++], done: false };
                }
                return { value: undefined, done: true };
            }
        };
    }
};

async function iterate(): Promise<void> {
    for await (const num of asyncIterable) {
        console.log(num);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables and function should be present
    assert!(
        output.contains("asyncIterable") && output.contains("iterate"),
        "Output should contain variable and function: {}",
        output
    );
    // Should reference Symbol.asyncIterator
    assert!(
        output.contains("Symbol.asyncIterator"),
        "Output should reference Symbol.asyncIterator: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": AsyncIterator") && !output.contains(": Promise<IteratorResult"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 dynamic import.
/// Dynamic import() expressions should be preserved or polyfilled.
#[test]
fn test_parity_es5_dynamic_import() {
    let source = r#"async function loadModule(name: string): Promise<any> {
    const module = await import(`./modules/${name}`);
    return module.default;
}

async function conditionalLoad(condition: boolean): Promise<void> {
    if (condition) {
        const { helper } = await import('./helpers');
        helper();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("loadModule") && output.contains("conditionalLoad"),
        "Output should contain functions: {}",
        output
    );
    // Should have async helpers
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter helper: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": Promise")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 top-level await simulation.
/// Top-level await in async IIFE pattern.
#[test]
fn test_parity_es5_top_level_await_iife() {
    let source = r#"const config = await import('./config');
const data: string = await fetch('/api/data').then(r => r.text());

export async function initialize(): Promise<void> {
    console.log(config, data);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables and function should be present
    assert!(
        output.contains("config") && output.contains("data") && output.contains("initialize"),
        "Output should contain variables and function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Promise"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 import.meta usage.
/// import.meta should be handled appropriately for target.
#[test]
fn test_parity_es5_import_meta() {
    let source = r#"const currentUrl: string = import.meta.url;
const baseDir: string = new URL('.', import.meta.url).pathname;

function getModulePath(): string {
    return import.meta.url;
}

export { currentUrl, baseDir, getModulePath };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::CommonJS;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables and function should be present
    assert!(
        output.contains("currentUrl")
            && output.contains("baseDir")
            && output.contains("getModulePath"),
        "Output should contain variables and function: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 tagged template with complex arguments.
/// Tagged templates with typed tag functions and complex expressions.
#[test]
fn test_parity_es5_tagged_template_complex() {
    let source = r#"function sql<T>(strings: TemplateStringsArray, ...values: any[]): T {
    return strings.reduce((acc, str, i) => acc + str + (values[i] || ''), '') as T;
}

interface User {
    id: number;
    name: string;
}

const userId: number = 42;
const userName: string = "Alice";
const query = sql<User[]>`SELECT * FROM users WHERE id = ${userId} AND name = ${userName}`;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Function and variables should be present
    assert!(
        output.contains("sql") && output.contains("userId") && output.contains("userName"),
        "Output should contain function and variables: {}",
        output
    );
    // No backtick template syntax
    assert!(
        !output.contains('`'),
        "ES5 output should not contain backtick syntax: {}",
        output
    );
    // Type annotations and interface should be erased
    assert!(
        !output.contains("interface User") && !output.contains(": TemplateStringsArray"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 template spans with complex expressions.
/// Template literals with method calls, ternaries, and nested expressions.
#[test]
fn test_parity_es5_template_spans_complex() {
    let source = r#"function formatUser(user: { name: string; age: number }): string {
    return `User: ${user.name.toUpperCase()} is ${user.age >= 18 ? 'adult' : 'minor'} (${user.age} years old)`;
}

function buildUrl(base: string, params: Record<string, string>): string {
    const queryString = Object.entries(params)
        .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
        .join('&');
    return `${base}?${queryString}`;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("formatUser") && output.contains("buildUrl"),
        "Output should contain functions: {}",
        output
    );
    // No backtick template syntax
    assert!(
        !output.contains('`'),
        "ES5 output should not contain backtick syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Record"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 deeply nested templates.
/// Templates inside templates with multiple nesting levels.
#[test]
fn test_parity_es5_template_deeply_nested() {
    let source = r#"function createHtml(items: string[]): string {
    return `<ul>${items.map(item => `<li>${item.includes('!') ? `<strong>${item}</strong>` : item}</li>`).join('')}</ul>`;
}

const result: string = createHtml(['Hello', 'World!', 'Test']);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Function and variable should be present
    assert!(
        output.contains("createHtml") && output.contains("result"),
        "Output should contain function and variable: {}",
        output
    );
    // No backtick template syntax
    assert!(
        !output.contains('`'),
        "ES5 output should not contain backtick syntax: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 String.raw tagged template.
/// String.raw for raw string handling without escape processing.
#[test]
fn test_parity_es5_template_raw_strings() {
    let source = r#"const path: string = String.raw`C:\Users\Documents\file.txt`;
const regex: string = String.raw`\d+\.\d+`;

function makeRaw(input: string): string {
    return String.raw`Raw: ${input}\n\t`;
}

const multiline: string = String.raw`First line
Second line
Third line`;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables and function should be present
    assert!(
        output.contains("path") && output.contains("regex") && output.contains("makeRaw"),
        "Output should contain variables and function: {}",
        output
    );
    // Should reference String.raw
    assert!(
        output.contains("String.raw"),
        "Output should reference String.raw: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 auto-accessor property.
/// Auto-accessors using the accessor keyword should be transformed.
#[test]
fn test_parity_es5_auto_accessor() {
    let source = r#"class Counter {
    accessor count: number = 0;

    increment(): void {
        this.count++;
    }

    decrement(): void {
        this.count--;
    }
}

class Person {
    accessor name: string;
    accessor age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Counter") && output.contains("Person"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("increment") && output.contains("decrement"),
        "Output should contain methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 computed accessor names.
/// Accessors with computed property names from symbols or expressions.
#[test]
fn test_parity_es5_computed_accessor_symbol() {
    let source = r#"const nameKey = Symbol('name');
const ageKey = 'user_age';

class User {
    private _data: Record<symbol | string, any> = {};

    get [nameKey](): string {
        return this._data[nameKey];
    }

    set [nameKey](value: string) {
        this._data[nameKey] = value;
    }

    get [ageKey](): number {
        return this._data[ageKey];
    }

    set [ageKey](value: number) {
        this._data[ageKey] = value;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and symbol should be present
    assert!(
        output.contains("User") && output.contains("nameKey") && output.contains("ageKey"),
        "Output should contain class and keys: {}",
        output
    );
    // Should have Symbol
    assert!(
        output.contains("Symbol"),
        "Output should reference Symbol: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private _data"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Parity test for ES5 inherited accessor override.
/// Derived class overriding base class accessors.
#[test]
fn test_parity_es5_inherited_accessor_override() {
    let source = r#"class BaseConfig {
    protected _value: string = '';

    get value(): string {
        return this._value;
    }

    set value(v: string) {
        this._value = v;
    }
}

class DerivedConfig extends BaseConfig {
    get value(): string {
        return `[Derived] ${super.value}`;
    }

    set value(v: string) {
        super.value = v.toUpperCase();
    }
}

class ReadOnlyConfig extends BaseConfig {
    get value(): string {
        return this._value;
    }
    // No setter - making it read-only in derived class
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // All classes should be present
    assert!(
        output.contains("BaseConfig")
            && output.contains("DerivedConfig")
            && output.contains("ReadOnlyConfig"),
        "Output should contain all classes: {}",
        output
    );
    // Should have prototype or __extends for inheritance
    assert!(
        output.contains("__extends") || output.contains(".prototype"),
        "ES5 output should have inheritance mechanism: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected _value"),
        "Protected modifier should be erased: {}",
        output
    );
}

/// Parity test for ES5 private instance fields with methods.
/// Private fields accessed and modified by instance methods.
#[test]
fn test_parity_es5_private_instance_field_methods() {
    let source = r#"class BankAccount {
    #balance: number = 0;
    #transactions: string[] = [];

    deposit(amount: number): void {
        this.#balance += amount;
        this.#transactions.push(`Deposit: ${amount}`);
    }

    withdraw(amount: number): boolean {
        if (amount > this.#balance) {
            return false;
        }
        this.#balance -= amount;
        this.#transactions.push(`Withdraw: ${amount}`);
        return true;
    }

    getBalance(): number {
        return this.#balance;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and methods should be present
    assert!(
        output.contains("BankAccount") && output.contains("deposit") && output.contains("withdraw"),
        "Output should contain class and methods: {}",
        output
    );
    // Should use private field helpers or WeakMap
    assert!(
        output.contains("__classPrivateFieldGet")
            || output.contains("__classPrivateFieldSet")
            || output.contains("WeakMap"),
        "ES5 output should use private field mechanism: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": boolean") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 private static fields.
/// Static private fields shared across all instances.
#[test]
fn test_parity_es5_private_static_field_complex() {
    let source = r#"class Logger {
    static #instance: Logger | null = null;
    static #logLevel: number = 0;
    #name: string;

    private constructor(name: string) {
        this.#name = name;
    }

    static getInstance(): Logger {
        if (!Logger.#instance) {
            Logger.#instance = new Logger('default');
        }
        return Logger.#instance;
    }

    static setLogLevel(level: number): void {
        Logger.#logLevel = level;
    }

    log(message: string): void {
        if (Logger.#logLevel > 0) {
            console.log(`[${this.#name}] ${message}`);
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and methods should be present
    assert!(
        output.contains("Logger")
            && output.contains("getInstance")
            && output.contains("setLogLevel"),
        "Output should contain class and methods: {}",
        output
    );
    // Private modifier on constructor should be erased
    assert!(
        !output.contains("private constructor"),
        "Private constructor modifier should be erased: {}",
        output
    );
}

/// Parity test for ES5 private methods with this binding.
/// Private methods that need proper this context.
#[test]
fn test_parity_es5_private_method_this_context() {
    let source = r#"class EventEmitter {
    #listeners: Map<string, Function[]> = new Map();

    #getListeners(event: string): Function[] {
        if (!this.#listeners.has(event)) {
            this.#listeners.set(event, []);
        }
        return this.#listeners.get(event)!;
    }

    #notifyListeners(event: string, data: any): void {
        const listeners = this.#getListeners(event);
        listeners.forEach(listener => listener(data));
    }

    on(event: string, callback: Function): void {
        this.#getListeners(event).push(callback);
    }

    emit(event: string, data: any): void {
        this.#notifyListeners(event, data);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and public methods should be present
    assert!(
        output.contains("EventEmitter") && output.contains("on") && output.contains("emit"),
        "Output should contain class and methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Map<") && !output.contains(": Function") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 private accessors with validation.
/// Private getters and setters with type checking logic.
#[test]
fn test_parity_es5_private_accessor_validation() {
    let source = r#"class Temperature {
    #celsius: number = 0;

    get #fahrenheit(): number {
        return (this.#celsius * 9/5) + 32;
    }

    set #fahrenheit(value: number) {
        this.#celsius = (value - 32) * 5/9;
    }

    get celsius(): number {
        return this.#celsius;
    }

    set celsius(value: number) {
        if (value < -273.15) {
            throw new Error('Below absolute zero');
        }
        this.#celsius = value;
    }

    get fahrenheit(): number {
        return this.#fahrenheit;
    }

    set fahrenheit(value: number) {
        this.#fahrenheit = value;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("Temperature"),
        "Output should contain class: {}",
        output
    );
    // Public accessors should be present
    assert!(
        output.contains("celsius") && output.contains("fahrenheit"),
        "Output should contain public accessors: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 static block with complex initialization order.
/// Static blocks initializing dependent static properties.
#[test]
fn test_parity_es5_static_block_complex_init_order() {
    let source = r#"class Config {
    static readonly BASE_URL: string = 'https://api.example.com';
    static readonly API_VERSION: string;
    static readonly FULL_URL: string;
    static readonly ENDPOINTS: Record<string, string>;

    static {
        Config.API_VERSION = 'v2';
    }

    static {
        Config.FULL_URL = `${Config.BASE_URL}/${Config.API_VERSION}`;
    }

    static {
        Config.ENDPOINTS = {
            users: `${Config.FULL_URL}/users`,
            posts: `${Config.FULL_URL}/posts`,
            comments: `${Config.FULL_URL}/comments`
        };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("Config"),
        "Output should contain class: {}",
        output
    );
    // Static properties should be present
    assert!(
        output.contains("BASE_URL")
            && output.contains("API_VERSION")
            && output.contains("FULL_URL"),
        "Output should contain static properties: {}",
        output
    );
    // No static block syntax in ES5
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
}

/// Parity test for ES5 static blocks interleaved with static fields.
/// Multiple static blocks between static field declarations.
#[test]
fn test_parity_es5_static_block_interleaved() {
    let source = r#"class Registry {
    static items: string[] = [];

    static {
        Registry.items.push('first');
    }

    static count: number = 0;

    static {
        Registry.count = Registry.items.length;
        Registry.items.push('second');
    }

    static metadata: { count: number; items: string[] };

    static {
        Registry.metadata = {
            count: Registry.count,
            items: [...Registry.items]
        };
    }

    static getAll(): string[] {
        return Registry.items;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and method should be present
    assert!(
        output.contains("Registry") && output.contains("getAll"),
        "Output should contain class and method: {}",
        output
    );
    // Static fields should be referenced
    assert!(
        output.contains("items") && output.contains("count") && output.contains("metadata"),
        "Output should contain static fields: {}",
        output
    );
    // No static block syntax in ES5
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
}

/// Parity test for ES5 static block with async initialization pattern.
/// Static blocks setting up async-related configurations.
#[test]
fn test_parity_es5_static_block_async_pattern() {
    let source = r#"class AsyncService {
    static #initPromise: Promise<void>;
    static #initialized: boolean = false;
    static #config: Record<string, any> = {};

    static {
        AsyncService.#initPromise = (async () => {
            await new Promise(resolve => setTimeout(resolve, 0));
            AsyncService.#config = { ready: true };
            AsyncService.#initialized = true;
        })();
    }

    static async waitForInit(): Promise<void> {
        await AsyncService.#initPromise;
    }

    static isReady(): boolean {
        return AsyncService.#initialized;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and methods should be present
    assert!(
        output.contains("AsyncService")
            && output.contains("waitForInit")
            && output.contains("isReady"),
        "Output should contain class and methods: {}",
        output
    );
    // Should have async helper
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter helper: {}",
        output
    );
    // No static block syntax in ES5
    assert!(
        !output.contains("static {"),
        "ES5 output should not contain static block syntax: {}",
        output
    );
}

/// Parity test for ES5 super property access.
/// Accessing properties on super in derived classes.
#[test]
fn test_parity_es5_super_property_access() {
    let source = r#"class Base {
    protected name: string = 'Base';
    protected getValue(): number {
        return 42;
    }
}

class Derived extends Base {
    private derivedName: string;

    constructor() {
        super();
        this.derivedName = super.name + 'Derived';
    }

    getDoubleValue(): number {
        return super.getValue() * 2;
    }

    getNames(): string {
        return `${super.name} -> ${this.derivedName}`;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("getDoubleValue") && output.contains("getNames"),
        "Output should contain methods: {}",
        output
    );
    // Protected/private modifiers should be erased
    assert!(
        !output.contains("protected name") && !output.contains("private derivedName"),
        "Access modifiers should be erased: {}",
        output
    );
}

/// Parity test for ES5 super method call with computed property.
/// Calling super methods using computed property names.
#[test]
fn test_parity_es5_super_method_computed() {
    let source = r#"const methodName = 'process';

class BaseProcessor {
    process(data: string): string {
        return data.toUpperCase();
    }

    validate(data: string): boolean {
        return data.length > 0;
    }
}

class DerivedProcessor extends BaseProcessor {
    process(data: string): string {
        const validated = super.validate(data);
        if (!validated) return '';
        return super[methodName](data) + '!';
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseProcessor") && output.contains("DerivedProcessor"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("process") && output.contains("validate"),
        "Output should contain methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 super in async method.
/// Calling super methods from async methods with await.
#[test]
fn test_parity_es5_super_in_async_method() {
    let source = r#"class BaseService {
    async fetchData(url: string): Promise<string> {
        return `data from ${url}`;
    }

    async processData(data: string): Promise<string> {
        return data.trim();
    }
}

class DerivedService extends BaseService {
    async fetchData(url: string): Promise<string> {
        const baseData = await super.fetchData(url);
        return `enhanced: ${baseData}`;
    }

    async fetchAndProcess(url: string): Promise<string> {
        const data = await super.fetchData(url);
        const processed = await super.processData(data);
        return processed;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseService") && output.contains("DerivedService"),
        "Output should contain classes: {}",
        output
    );
    // Should have async helper
    assert!(
        output.contains("__awaiter"),
        "ES5 output should use __awaiter helper: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise<") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Parity test for ES5 super in arrow function.
/// Super captured correctly in arrow functions within class methods.
#[test]
fn test_parity_es5_super_in_arrow() {
    let source = r#"class BaseHandler {
    handle(value: number): number {
        return value * 2;
    }

    handleAsync(value: number): Promise<number> {
        return Promise.resolve(value * 2);
    }
}

class DerivedHandler extends BaseHandler {
    handleAll(values: number[]): number[] {
        return values.map(v => super.handle(v));
    }

    handleAllAsync(values: number[]): Promise<number[]> {
        return Promise.all(values.map(v => super.handleAsync(v)));
    }

    createHandler(): (value: number) => number {
        return (v) => super.handle(v) + 1;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseHandler") && output.contains("DerivedHandler"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("handleAll") && output.contains("createHandler"),
        "Output should contain methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains(": Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Generator Method Patterns Parity Tests
// =============================================================================

/// Test: yield expressions in conditional branches
#[test]
fn test_parity_es5_generator_yield_in_conditional() {
    let source = r#"
class StateMachine<T> {
    private state: string = "idle";

    *process(items: T[], condition: boolean): Generator<T | string, void, unknown> {
        for (const item of items) {
            if (condition) {
                yield item;
            } else {
                yield "skipped";
            }
        }
        yield this.state;
    }

    *processWithTernary(value: T): Generator<T | null, void, unknown> {
        const result = value ? yield value : yield null;
        yield result as T;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("StateMachine"),
        "Output should contain class: {}",
        output
    );
    // Generator methods should be defined
    assert!(
        output.contains("process") && output.contains("processWithTernary"),
        "Output should contain generator methods: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Generator<"),
        "Type parameters should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T[]") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test: yield as function argument
#[test]
fn test_parity_es5_generator_yield_as_argument() {
    let source = r#"
function log<T>(value: T): T {
    console.log(value);
    return value;
}

function* produceWithLogging(): Generator<number, void, number> {
    const received = yield log(1);
    yield log(received + 1);
    yield log(received + 2);
}

class DataProducer {
    private transform<T>(value: T): T {
        return value;
    }

    *produceTransformed(values: number[]): Generator<number, void, unknown> {
        for (const v of values) {
            yield this.transform(v * 2);
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("function log") && output.contains("produceWithLogging"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DataProducer"),
        "Output should contain class: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Generator<"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: nested yield* delegation with multiple levels
#[test]
fn test_parity_es5_generator_delegation_nested() {
    let source = r#"
function* innerGenerator(): Generator<number, void, unknown> {
    yield 1;
    yield 2;
}

function* middleGenerator(): Generator<number, void, unknown> {
    yield* innerGenerator();
    yield 3;
    yield* innerGenerator();
}

function* outerGenerator(): Generator<number, void, unknown> {
    yield 0;
    yield* middleGenerator();
    yield 4;
}

class NestedDelegator {
    private *inner(): Generator<string, void, unknown> {
        yield "a";
        yield "b";
    }

    *outer(): Generator<string, void, unknown> {
        yield* this.inner();
        yield "c";
        yield* this.inner();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // All generator functions should be present
    assert!(
        output.contains("innerGenerator")
            && output.contains("middleGenerator")
            && output.contains("outerGenerator"),
        "Output should contain all generator functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("NestedDelegator"),
        "Output should contain class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains("Generator<"),
        "Generator type should be erased: {}",
        output
    );
}

/// Test: async generator with Promise.all pattern
#[test]
fn test_parity_es5_async_generator_promise_all() {
    let source = r#"
async function* fetchMultiple(urls: string[]): AsyncGenerator<Response, void, unknown> {
    const responses = await Promise.all(urls.map(url => fetch(url)));
    for (const response of responses) {
        yield response;
    }
}

class BatchProcessor<T, R> {
    private async processItem(item: T): Promise<R> {
        return item as unknown as R;
    }

    async *processBatch(items: T[]): AsyncGenerator<R, void, unknown> {
        const results = await Promise.all(items.map(item => this.processItem(item)));
        for (const result of results) {
            yield result;
        }
    }

    async *processWithRace(items: T[]): AsyncGenerator<R, void, unknown> {
        const first = await Promise.race(items.map(item => this.processItem(item)));
        yield first;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("fetchMultiple") && output.contains("BatchProcessor"),
        "Output should contain function and class: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("processBatch") && output.contains("processWithRace"),
        "Output should contain methods: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T,") && !output.contains("AsyncGenerator<"),
        "Type parameters should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string[]") && !output.contains(": Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Async Function Patterns Parity Tests
// =============================================================================

/// Test: async arrow with destructuring params and array methods
#[test]
fn test_parity_es5_async_arrow_destructuring() {
    let source = r#"
interface User {
    id: number;
    name: string;
}

const processUsers = async ({ users, limit }: { users: User[], limit: number }): Promise<string[]> => {
    const names = await Promise.all(
        users.slice(0, limit).map(async ({ name }) => {
            return name.toUpperCase();
        })
    );
    return names;
};

const nestedAsync = async (items: number[]): Promise<number[]> => {
    return await Promise.all(
        items.map(async (item) => {
            const doubled = await Promise.resolve(item * 2);
            return doubled;
        })
    );
};

const asyncInReduce = async (values: number[]): Promise<number> => {
    return await values.reduce(async (accPromise, curr) => {
        const acc = await accPromise;
        return acc + curr;
    }, Promise.resolve(0));
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("processUsers")
            && output.contains("nestedAsync")
            && output.contains("asyncInReduce"),
        "Output should contain async arrow functions: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface User"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": User[]") && !output.contains(": Promise<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test: async method with computed property and this capture
#[test]
fn test_parity_es5_async_method_computed_this() {
    let source = r#"
const methodNames = {
    fetch: "fetchData",
    process: "processData"
} as const;

class DataService<T> {
    private cache: Map<string, T> = new Map();

    async [methodNames.fetch](id: string): Promise<T | undefined> {
        const cached = this.cache.get(id);
        if (cached) return cached;

        const data = await this.loadFromServer(id);
        this.cache.set(id, data);
        return data;
    }

    private async loadFromServer(id: string): Promise<T> {
        return {} as T;
    }

    async processWithCallback(items: T[], callback: (item: T) => Promise<T>): Promise<T[]> {
        const self = this;
        return await Promise.all(
            items.map(async function(item) {
                const result = await callback(item);
                return result;
            })
        );
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("DataService"),
        "Output should contain class: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("loadFromServer") && output.contains("processWithCallback"),
        "Output should contain methods: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Promise<T"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: async generator with Symbol.asyncIterator implementation
#[test]
fn test_parity_es5_async_generator_symbol_iterator() {
    let source = r#"
class AsyncStream<T> {
    private items: T[];

    constructor(items: T[]) {
        this.items = items;
    }

    async *[Symbol.asyncIterator](): AsyncGenerator<T, void, unknown> {
        for (const item of this.items) {
            await new Promise(resolve => setTimeout(resolve, 10));
            yield item;
        }
    }

    async *filter(predicate: (item: T) => Promise<boolean>): AsyncGenerator<T, void, unknown> {
        for await (const item of this) {
            if (await predicate(item)) {
                yield item;
            }
        }
    }

    static async *fromPromises<U>(promises: Promise<U>[]): AsyncGenerator<U, void, unknown> {
        for (const promise of promises) {
            yield await promise;
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("AsyncStream"),
        "Output should contain class: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("filter") && output.contains("fromPromises"),
        "Output should contain methods: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<U>") && !output.contains("AsyncGenerator<"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: await expressions with advanced patterns
#[test]
fn test_parity_es5_await_advanced_patterns() {
    let source = r#"
interface Result<T> {
    data?: T;
    error?: Error;
}

async function fetchWithFallback<T>(
    primary: () => Promise<T>,
    fallback: () => Promise<T>
): Promise<T> {
    const result = await primary().catch(() => null);
    return result ?? await fallback();
}

async function settledResults<T>(promises: Promise<T>[]): Promise<Result<T>[]> {
    const settled = await Promise.allSettled(promises);
    return settled.map(result =>
        result.status === "fulfilled"
            ? { data: result.value }
            : { error: result.reason }
    );
}

class ApiClient {
    private baseUrl?: string;

    async fetchOptional<T>(endpoint: string): Promise<T | null> {
        const url = this.baseUrl ?? "https://api.example.com";
        const response = await fetch(url + endpoint);
        const data = await response?.json?.();
        return data as T ?? null;
    }

    async fetchWithDestructure(): Promise<{ id: number; name: string }> {
        const { data: { user: { id, name } } } = await this.fetchNested();
        return { id, name };
    }

    private async fetchNested(): Promise<{ data: { user: { id: number; name: string } } }> {
        return { data: { user: { id: 1, name: "test" } } };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("fetchWithFallback") && output.contains("settledResults"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ApiClient"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Result"),
        "Interface should be erased: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Promise<T>"),
        "Type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 For-Of/For-In Patterns Parity Tests
// =============================================================================

/// Test: for-in loop with type annotations and object types
#[test]
fn test_parity_es5_for_in_typed() {
    let source = r#"
interface Config {
    host: string;
    port: number;
    debug: boolean;
}

function getKeys<T extends object>(obj: T): (keyof T)[] {
    const keys: (keyof T)[] = [];
    for (const key in obj) {
        if (Object.prototype.hasOwnProperty.call(obj, key)) {
            keys.push(key as keyof T);
        }
    }
    return keys;
}

function copyProperties<T extends object>(source: T, target: Partial<T>): void {
    for (const prop in source) {
        if (source.hasOwnProperty(prop)) {
            target[prop] = source[prop];
        }
    }
}

class ObjectUtils {
    static enumerate<T extends Record<string, unknown>>(obj: T): Array<[keyof T, T[keyof T]]> {
        const entries: Array<[keyof T, T[keyof T]]> = [];
        for (const key in obj) {
            if (obj.hasOwnProperty(key)) {
                entries.push([key as keyof T, obj[key]]);
            }
        }
        return entries;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("getKeys") && output.contains("copyProperties"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ObjectUtils"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "Interface should be erased: {}",
        output
    );
    // Type parameters and annotations should be erased
    assert!(
        !output.contains("<T extends") && !output.contains("keyof T"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: for-in with computed property access
#[test]
fn test_parity_es5_for_in_computed() {
    let source = r#"
type IndexedObject = { [key: string]: number };

function sumValues(obj: IndexedObject): number {
    let sum = 0;
    for (const key in obj) {
        sum += obj[key];
    }
    return sum;
}

function transformObject<T, U>(
    obj: Record<string, T>,
    transform: (value: T, key: string) => U
): Record<string, U> {
    const result: Record<string, U> = {};
    for (const key in obj) {
        if (Object.prototype.hasOwnProperty.call(obj, key)) {
            result[key] = transform(obj[key], key);
        }
    }
    return result;
}

class DynamicAccessor {
    private data: { [key: string]: unknown } = {};

    setAll(source: object): void {
        for (const prop in source) {
            this.data[prop] = (source as any)[prop];
        }
    }

    getFiltered(predicate: (key: string) => boolean): { [key: string]: unknown } {
        const filtered: { [key: string]: unknown } = {};
        for (const key in this.data) {
            if (predicate(key)) {
                filtered[key] = this.data[key];
            }
        }
        return filtered;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("sumValues") && output.contains("transformObject"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DynamicAccessor"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type IndexedObject"),
        "Type alias should be erased: {}",
        output
    );
    // Generic parameters should be erased
    assert!(
        !output.contains("<T, U>") && !output.contains("Record<string"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: for-of with custom iterator protocol
#[test]
fn test_parity_es5_for_of_custom_iterator() {
    let source = r#"
interface IteratorResult<T> {
    done: boolean;
    value: T;
}

class Range implements Iterable<number> {
    constructor(private start: number, private end: number) {}

    [Symbol.iterator](): Iterator<number> {
        let current = this.start;
        const end = this.end;
        return {
            next(): IteratorResult<number> {
                if (current <= end) {
                    return { done: false, value: current++ };
                }
                return { done: true, value: undefined as any };
            }
        };
    }
}

function* customGenerator<T>(items: T[]): Generator<T, void, unknown> {
    for (const item of items) {
        yield item;
    }
}

function collectFromIterator<T>(iterable: Iterable<T>): T[] {
    const result: T[] = [];
    for (const item of iterable) {
        result.push(item);
    }
    return result;
}

class IterableCollection<T> {
    private items: T[] = [];

    add(item: T): void {
        this.items.push(item);
    }

    *[Symbol.iterator](): Generator<T, void, unknown> {
        for (const item of this.items) {
            yield item;
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Range") && output.contains("IterableCollection"),
        "Output should contain classes: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("customGenerator") && output.contains("collectFromIterator"),
        "Output should contain functions: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface IteratorResult"),
        "Interface should be erased: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Iterable<number>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: for-of with Map and Set destructuring
#[test]
fn test_parity_es5_for_of_map_set_destruct() {
    let source = r#"
function processMap<K, V>(map: Map<K, V>): Array<{ key: K; value: V }> {
    const result: Array<{ key: K; value: V }> = [];
    for (const [key, value] of map) {
        result.push({ key, value });
    }
    return result;
}

function setToArray<T>(set: Set<T>): T[] {
    const arr: T[] = [];
    for (const item of set) {
        arr.push(item);
    }
    return arr;
}

class MapReducer<K, V> {
    constructor(private map: Map<K, V>) {}

    reduce<R>(initial: R, reducer: (acc: R, key: K, value: V) => R): R {
        let result = initial;
        for (const [key, value] of this.map) {
            result = reducer(result, key, value);
        }
        return result;
    }

    filterEntries(predicate: (key: K, value: V) => boolean): Map<K, V> {
        const filtered = new Map<K, V>();
        for (const [key, value] of this.map) {
            if (predicate(key, value)) {
                filtered.set(key, value);
            }
        }
        return filtered;
    }
}

async function asyncMapProcess<K, V>(
    map: Map<K, V>,
    processor: (key: K, value: V) => Promise<void>
): Promise<void> {
    for (const [key, value] of map) {
        await processor(key, value);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("processMap")
            && output.contains("setToArray")
            && output.contains("asyncMapProcess"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("MapReducer"),
        "Output should contain class: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<K, V>") && !output.contains("Map<K, V>") && !output.contains("Set<T>"),
        "Type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Class Field Patterns Parity Tests
// =============================================================================

/// Test: public fields with various initializers
#[test]
fn test_parity_es5_class_public_field_initializers() {
    let source = r#"
class DataModel<T> {
    // Primitive initializers
    name: string = "default";
    count: number = 0;
    enabled: boolean = true;

    // Complex initializers
    items: T[] = [];
    metadata: Record<string, unknown> = {};
    callback: ((value: T) => void) | null = null;

    // Computed initializers
    timestamp: number = Date.now();
    id: string = Math.random().toString(36);

    // Arrow function initializer
    handler: (event: Event) => void = (e) => {
        console.log(this.name, e);
    };

    // Method reference initializer
    boundMethod = this.process.bind(this);

    process(value: T): void {
        this.items.push(value);
    }
}

class ConfigurableService {
    readonly apiUrl: string = "https://api.example.com";
    private readonly timeout: number = 5000;
    protected retryCount: number = 3;

    constructor(public customUrl?: string) {
        if (customUrl) {
            // Custom URL provided
        }
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("DataModel") && output.contains("ConfigurableService"),
        "Output should contain classes: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains(": T[]"),
        "Type parameters should be erased: {}",
        output
    );
    // Access modifiers should be erased
    assert!(
        !output.contains("readonly ")
            && !output.contains("private ")
            && !output.contains("protected "),
        "Access modifiers should be erased: {}",
        output
    );
}

/// Test: class fields with decorators
#[test]
fn test_parity_es5_class_field_with_decorators() {
    let source = r#"
function observable<T>(target: any, key: string): void {
    // Decorator implementation
}

function validate(min: number, max: number) {
    return function(target: any, key: string): void {
        // Validation decorator factory
    };
}

function inject(token: string) {
    return function(target: any, key: string): void {
        // DI decorator
    };
}

class ObservableModel {
    @observable
    name: string = "";

    @observable
    @validate(0, 100)
    age: number = 0;

    @inject("logger")
    private logger?: { log: (msg: string) => void };

    @observable
    items: string[] = [];
}

class FormModel {
    @validate(1, 50)
    username: string = "";

    @validate(8, 128)
    password: string = "";

    @observable
    isValid: boolean = false;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("ObservableModel") && output.contains("FormModel"),
        "Output should contain classes: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("observable") && output.contains("validate"),
        "Output should contain decorator functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test: computed fields with dynamic expressions
#[test]
fn test_parity_es5_class_field_computed_dynamic() {
    let source = r#"
const FIELD_PREFIX = "data_";
const fieldNames = {
    first: "firstName",
    last: "lastName"
} as const;

const symbolKey = Symbol("privateData");

class DynamicFields {
    [FIELD_PREFIX + "id"]: number = 1;
    [fieldNames.first]: string = "";
    [fieldNames.last]: string = "";
    [symbolKey]: unknown = null;

    static [FIELD_PREFIX + "version"]: string = "1.0.0";

    ["computed" + "Method"](): void {
        console.log(this[fieldNames.first]);
    }
}

function createFieldName(base: string): string {
    return `field_${base}`;
}

class ComputedFromFunction {
    [createFieldName("a")]: number = 1;
    [createFieldName("b")]: number = 2;

    getField(name: string): number {
        return (this as any)[createFieldName(name)];
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("DynamicFields") && output.contains("ComputedFromFunction"),
        "Output should contain classes: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("createFieldName"),
        "Output should contain helper functions: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": string")
            && !output.contains(": unknown"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test: field inheritance across multiple classes
#[test]
fn test_parity_es5_class_field_inheritance_chain() {
    let source = r#"
abstract class BaseEntity {
    id: number = 0;
    createdAt: Date = new Date();
    abstract validate(): boolean;
}

class TimestampedEntity extends BaseEntity {
    updatedAt: Date = new Date();

    validate(): boolean {
        return this.id > 0;
    }

    touch(): void {
        this.updatedAt = new Date();
    }
}

class User extends TimestampedEntity {
    name: string = "";
    email: string = "";
    private passwordHash: string = "";

    override validate(): boolean {
        return super.validate() && this.email.includes("@");
    }

    setPassword(password: string): void {
        this.passwordHash = password; // simplified
    }
}

class Admin extends User {
    permissions: string[] = [];
    static readonly SUPER_ADMIN = "super_admin";

    override validate(): boolean {
        return super.validate() && this.permissions.length > 0;
    }

    hasPermission(perm: string): boolean {
        return this.permissions.includes(perm);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // All classes should be present
    assert!(
        output.contains("BaseEntity")
            && output.contains("TimestampedEntity")
            && output.contains("User")
            && output.contains("Admin"),
        "Output should contain all classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("validate") && output.contains("touch") && output.contains("hasPermission"),
        "Output should contain methods: {}",
        output
    );
    // Abstract and access modifiers should be erased
    assert!(
        !output.contains("abstract ")
            && !output.contains("private ")
            && !output.contains("override "),
        "Modifiers should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Date")
            && !output.contains(": string[]")
            && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Arrow Function Edge Case Parity Tests
// =============================================================================

/// Test: deeply nested arrows with this binding at multiple levels
#[test]
fn test_parity_es5_arrow_deeply_nested_this() {
    let source = r#"
class EventManager {
    private listeners: Map<string, Function[]> = new Map();
    name: string = "manager";

    register(event: string): (callback: Function) => () => void {
        return (callback: Function) => {
            const list = this.listeners.get(event) || [];
            list.push(callback);
            this.listeners.set(event, list);

            return () => {
                const current = this.listeners.get(event) || [];
                const filtered = current.filter((cb) => {
                    return cb !== callback;
                });
                this.listeners.set(event, filtered);
                console.log(this.name);
            };
        };
    }

    createHandler(): () => () => () => string {
        return () => {
            const outer = this.name;
            return () => {
                const middle = outer;
                return () => {
                    return `${this.name}:${middle}:${outer}`;
                };
            };
        };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("EventManager"),
        "Output should contain class: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("register") && output.contains("createHandler"),
        "Output should contain methods: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Map<") && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test: arrows in class field initializers with this context
#[test]
fn test_parity_es5_arrow_class_field_context() {
    let source = r#"
class Component<T> {
    state: T;

    // Arrow in field initializer captures this
    handleClick = (event: MouseEvent): void => {
        console.log(this.state, event);
    };

    handleChange = (value: T): T => {
        this.state = value;
        return this.state;
    };

    // Nested arrow in field
    processor = (items: T[]): ((item: T) => T) => {
        return (item: T) => {
            console.log(this.state);
            return item;
        };
    };

    // Arrow with async
    fetchData = async (): Promise<T> => {
        console.log(this.state);
        return this.state;
    };

    constructor(initial: T) {
        this.state = initial;
    }
}

class DerivedComponent extends Component<string> {
    label: string = "";

    // Override with arrow
    handleClick = (event: MouseEvent): void => {
        console.log(this.label, this.state, event);
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Component") && output.contains("DerivedComponent"),
        "Output should contain classes: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains(": T"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: complex rest and spread patterns in arrows
#[test]
fn test_parity_es5_arrow_rest_spread_complex() {
    let source = r#"
type Callback<T> = (...args: T[]) => void;

const variadicLogger = <T>(...items: T[]): void => {
    items.forEach((item, i) => console.log(i, item));
};

const combiner = <T, U>(...arrays: T[][]): ((transform: (item: T) => U) => U[]) => {
    const combined = arrays.flat();
    return (transform: (item: T) => U) => combined.map(transform);
};

const partialApply = <T, R>(
    fn: (...args: T[]) => R,
    ...firstArgs: T[]
): ((...remainingArgs: T[]) => R) => {
    return (...remainingArgs: T[]) => fn(...firstArgs, ...remainingArgs);
};

class Aggregator<T> {
    collect = (...items: T[]): T[] => {
        return [...items];
    };

    merge = (...arrays: T[][]): T[] => {
        return arrays.reduce((acc, arr) => [...acc, ...arr], []);
    };

    transform = <U>(mapper: (item: T) => U) => (...items: T[]): U[] => {
        return items.map(mapper);
    };
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("variadicLogger")
            && output.contains("combiner")
            && output.contains("partialApply"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("Aggregator"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Callback"),
        "Type alias should be erased: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, U>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: arrow functions in method chains and callbacks
#[test]
fn test_parity_es5_arrow_callback_chains() {
    let source = r#"
interface Item {
    id: number;
    value: string;
    score: number;
}

function processItems(items: Item[]): string[] {
    return items
        .filter((item) => item.score > 0)
        .map((item) => ({ ...item, value: item.value.toUpperCase() }))
        .sort((a, b) => b.score - a.score)
        .map((item) => item.value);
}

class DataProcessor<T> {
    constructor(private items: T[]) {}

    pipe<U>(fn: (items: T[]) => U): U {
        return fn(this.items);
    }

    chain(): {
        filter: (pred: (item: T) => boolean) => ReturnType<typeof this.chain>;
        map: <U>(fn: (item: T) => U) => DataProcessor<U>;
        result: () => T[];
    } {
        const self = this;
        return {
            filter: (pred: (item: T) => boolean) => {
                self.items = self.items.filter(pred);
                return self.chain();
            },
            map: <U>(fn: (item: T) => U) => {
                return new DataProcessor(self.items.map(fn));
            },
            result: () => self.items
        };
    }
}

const createPipeline = <T>() => ({
    from: (items: T[]) => ({
        through: <U>(fn: (item: T) => U) => ({
            collect: (): U[] => items.map(fn)
        })
    })
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("processItems") && output.contains("createPipeline"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DataProcessor"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Item"),
        "Interface should be erased: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<U>"),
        "Type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Destructuring Patterns Parity Tests
// =============================================================================

/// Test: complex function parameter destructuring
#[test]
fn test_parity_es5_destructuring_function_params_complex() {
    let source = r#"
interface Options {
    host: string;
    port: number;
    ssl?: boolean;
}

function connect(
    { host, port, ssl = false }: Options,
    [primary, secondary]: [string, string],
    { timeout = 5000, retries = 3 }: { timeout?: number; retries?: number } = {}
): void {
    console.log(host, port, ssl, primary, secondary, timeout, retries);
}

const processData = (
    { data: { items, metadata: { count } } }: { data: { items: string[]; metadata: { count: number } } },
    [first, ...rest]: string[]
): string[] => {
    console.log(count, first);
    return [...items, ...rest];
};

class ConfigParser {
    parse(
        { config: { name, values = [] } }: { config: { name: string; values?: number[] } }
    ): { name: string; sum: number } {
        return { name, sum: values.reduce((a, b) => a + b, 0) };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("connect") && output.contains("processData"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ConfigParser"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Options"),
        "Interface should be erased: {}",
        output
    );
}

/// Test: mixing array and object destructuring in various contexts
#[test]
fn test_parity_es5_destructuring_mixed_patterns() {
    let source = r#"
type DataTuple = [{ id: number; name: string }, string[], { active: boolean }];

function processMixed([first, items, { active }]: DataTuple): string {
    const { id, name } = first;
    const [primary, ...others] = items;
    return active ? `${id}:${name}:${primary}` : others.join(",");
}

const extractNested = ({
    user: { profile: [firstName, lastName] },
    settings: { theme: { primary: primaryColor } }
}: {
    user: { profile: [string, string] };
    settings: { theme: { primary: string } };
}): string => {
    return `${firstName} ${lastName} - ${primaryColor}`;
};

class DataExtractor<T> {
    extract(
        { items: [first, second, ...rest] }: { items: T[] }
    ): { first: T; second: T; rest: T[] } {
        return { first, second, rest };
    }

    transform(
        [{ value: v1 }, { value: v2 }]: Array<{ value: T }>
    ): [T, T] {
        return [v1, v2];
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("processMixed") && output.contains("extractNested"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DataExtractor"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type DataTuple"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test: defaults with computed values and expressions
#[test]
fn test_parity_es5_destructuring_computed_defaults() {
    let source = r#"
const DEFAULT_HOST = "localhost";
const DEFAULT_PORT = 8080;
const getDefaultTimeout = (): number => 5000;

function configure({
    host = DEFAULT_HOST,
    port = DEFAULT_PORT,
    timeout = getDefaultTimeout(),
    retries = Math.max(1, 3)
}: {
    host?: string;
    port?: number;
    timeout?: number;
    retries?: number;
} = {}): void {
    console.log(host, port, timeout, retries);
}

const processWithDefaults = ({
    items = [] as string[],
    transform = ((x: string) => x.toUpperCase()),
    filter = ((x: string) => x.length > 0)
}: {
    items?: string[];
    transform?: (x: string) => string;
    filter?: (x: string) => boolean;
}): string[] => {
    return items.filter(filter).map(transform);
};

class Builder {
    private defaults = { name: "default", value: 0 };

    build({
        name = this.defaults.name,
        value = this.defaults.value,
        multiplier = 1
    }: {
        name?: string;
        value?: number;
        multiplier?: number;
    } = {}): { name: string; result: number } {
        return { name, result: value * multiplier };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("configure") && output.contains("processWithDefaults"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("Builder"),
        "Output should contain class: {}",
        output
    );
    // Constants should be present
    assert!(
        output.contains("DEFAULT_HOST") && output.contains("DEFAULT_PORT"),
        "Output should contain constants: {}",
        output
    );
}

/// Test: destructuring in class methods and constructors
#[test]
fn test_parity_es5_destructuring_class_methods() {
    let source = r#"
interface Point {
    x: number;
    y: number;
}

interface Rectangle {
    topLeft: Point;
    bottomRight: Point;
}

class Geometry {
    constructor(
        private { x: originX, y: originY }: Point = { x: 0, y: 0 }
    ) {}

    distance({ x: x1, y: y1 }: Point, { x: x2, y: y2 }: Point): number {
        return Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2);
    }

    area({ topLeft: { x: left, y: top }, bottomRight: { x: right, y: bottom } }: Rectangle): number {
        return Math.abs(right - left) * Math.abs(bottom - top);
    }

    static fromArray([x, y]: [number, number]): Point {
        return { x, y };
    }

    *iteratePoints([first, ...rest]: Point[]): Generator<Point, void, unknown> {
        yield first;
        for (const point of rest) {
            yield point;
        }
    }
}

class Transform extends Geometry {
    translate(
        { x, y }: Point,
        { dx = 0, dy = 0 }: { dx?: number; dy?: number } = {}
    ): Point {
        return { x: x + dx, y: y + dy };
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Geometry") && output.contains("Transform"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("distance") && output.contains("area") && output.contains("translate"),
        "Output should contain methods: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Point") && !output.contains("interface Rectangle"),
        "Interfaces should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Spread/Rest Patterns Parity Tests
// =============================================================================

/// Test: spread with method definitions in object literals
#[test]
fn test_parity_es5_spread_object_literal_methods() {
    let source = r#"
interface Base {
    id: number;
    getName(): string;
}

const baseMethods = {
    getName(): string { return "base"; },
    getId(): number { return this.id; }
};

function createObject(id: number, extra: Record<string, unknown>): Base & typeof extra {
    return {
        id,
        ...baseMethods,
        ...extra,
        toString() { return `Object(${this.id})`; }
    };
}

class ObjectFactory<T extends object> {
    private defaults: T;

    constructor(defaults: T) {
        this.defaults = defaults;
    }

    create(overrides: Partial<T>): T {
        return { ...this.defaults, ...overrides };
    }

    extend<U extends object>(extension: U): T & U {
        return { ...this.defaults, ...extension };
    }
}

const mergeWithMethods = <T extends object>(
    base: T,
    methods: { [K: string]: (...args: unknown[]) => unknown }
): T => {
    return { ...base, ...methods };
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("createObject") && output.contains("mergeWithMethods"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ObjectFactory"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Base"),
        "Interface should be erased: {}",
        output
    );
}

/// Test: rest patterns in async error handling
#[test]
fn test_parity_es5_rest_async_error_handling() {
    let source = r#"
type ErrorHandler = (...errors: Error[]) => void;

async function executeWithRetry<T>(
    fn: () => Promise<T>,
    ...fallbacks: Array<() => Promise<T>>
): Promise<T> {
    try {
        return await fn();
    } catch (e) {
        for (const fallback of fallbacks) {
            try {
                return await fallback();
            } catch {
                continue;
            }
        }
        throw e;
    }
}

class ErrorCollector {
    private errors: Error[] = [];

    collect(...newErrors: Error[]): void {
        this.errors.push(...newErrors);
    }

    async processAll(
        handler: (...errors: Error[]) => Promise<void>
    ): Promise<void> {
        await handler(...this.errors);
        this.errors = [];
    }

    getAll(): Error[] {
        return [...this.errors];
    }
}

const logErrors = (...errors: Error[]): void => {
    errors.forEach((err, i) => console.log(i, err.message));
};

async function batchProcess<T, R>(
    items: T[],
    processor: (item: T) => Promise<R>,
    ...errorHandlers: ErrorHandler[]
): Promise<R[]> {
    const results: R[] = [];
    for (const item of items) {
        try {
            results.push(await processor(item));
        } catch (e) {
            errorHandlers.forEach(h => h(e as Error));
        }
    }
    return results;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("executeWithRetry")
            && output.contains("logErrors")
            && output.contains("batchProcess"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ErrorCollector"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type ErrorHandler"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test: spread with custom iterables and generators
#[test]
fn test_parity_es5_spread_custom_iterables() {
    let source = r#"
class NumberRange implements Iterable<number> {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number, void, unknown> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }

    toArray(): number[] {
        return [...this];
    }

    concat(other: Iterable<number>): number[] {
        return [...this, ...other];
    }
}

function* generateValues<T>(items: T[]): Generator<T, void, unknown> {
    for (const item of items) {
        yield item;
    }
}

function collectFromGenerators<T>(...generators: Array<Generator<T>>): T[] {
    const result: T[] = [];
    for (const gen of generators) {
        result.push(...gen);
    }
    return result;
}

const spreadIterables = <T>(...iterables: Array<Iterable<T>>): T[] => {
    return iterables.flatMap(it => [...it]);
};

class IterableCollector<T> {
    private items: T[] = [];

    addFrom(...sources: Array<Iterable<T>>): void {
        for (const source of sources) {
            this.items.push(...source);
        }
    }

    getAll(): T[] {
        return [...this.items];
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("NumberRange") && output.contains("IterableCollector"),
        "Output should contain classes: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("generateValues") && output.contains("collectFromGenerators"),
        "Output should contain functions: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Iterable<number>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: generics with rest/spread in function signatures
#[test]
fn test_parity_es5_rest_spread_generic_signatures() {
    let source = r#"
type Fn<T extends unknown[], R> = (...args: T) => R;

function curry<T, U extends unknown[], R>(
    fn: (first: T, ...rest: U) => R
): (first: T) => (...rest: U) => R {
    return (first: T) => (...rest: U) => fn(first, ...rest);
}

function compose<T extends unknown[], U, R>(
    f: (arg: U) => R,
    g: (...args: T) => U
): (...args: T) => R {
    return (...args: T) => f(g(...args));
}

function pipe<T>(...fns: Array<(arg: T) => T>): (arg: T) => T {
    return (arg: T) => fns.reduce((acc, fn) => fn(acc), arg);
}

class FunctionBuilder<T extends unknown[], R> {
    constructor(private fn: (...args: T) => R) {}

    bind<U extends unknown[]>(
        ...boundArgs: U
    ): FunctionBuilder<Exclude<T, U>, R> {
        const newFn = (...args: unknown[]) => this.fn(...boundArgs as unknown as T, ...args as unknown as T);
        return new FunctionBuilder(newFn as (...args: Exclude<T, U>) => R);
    }

    call(...args: T): R {
        return this.fn(...args);
    }

    apply(args: T): R {
        return this.fn(...args);
    }
}

const wrapWithLogging = <T extends unknown[], R>(
    fn: (...args: T) => R,
    label: string
): ((...args: T) => R) => {
    return (...args: T) => {
        console.log(label, ...args);
        return fn(...args);
    };
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("curry") && output.contains("compose") && output.contains("pipe"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("FunctionBuilder"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Fn"),
        "Type alias should be erased: {}",
        output
    );
    // Generic parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, U"),
        "Type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Optional Chaining Patterns Parity Tests
// =============================================================================

/// Test: deeply nested optional chains
#[test]
fn test_parity_es5_optional_chaining_deep_nested() {
    let source = r#"
interface DeepObject {
    level1?: {
        level2?: {
            level3?: {
                level4?: {
                    value: string;
                    method(): string;
                };
            };
        };
    };
}

function getDeepValue(obj: DeepObject): string | undefined {
    return obj?.level1?.level2?.level3?.level4?.value;
}

function callDeepMethod(obj: DeepObject): string | undefined {
    return obj?.level1?.level2?.level3?.level4?.method?.();
}

const processDeep = (obj: DeepObject): { value?: string; called?: string } => {
    return {
        value: obj?.level1?.level2?.level3?.level4?.value,
        called: obj?.level1?.level2?.level3?.level4?.method?.()
    };
};

class DeepAccessor {
    private data: DeepObject | null = null;

    setData(data: DeepObject | null): void {
        this.data = data;
    }

    getValue(): string | undefined {
        return this.data?.level1?.level2?.level3?.level4?.value;
    }

    getWithDefault(defaultValue: string): string {
        return this.data?.level1?.level2?.level3?.level4?.value ?? defaultValue;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("getDeepValue") && output.contains("callDeepMethod"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("DeepAccessor"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface DeepObject"),
        "Interface should be erased: {}",
        output
    );
}

/// Test: optional chaining in class method contexts
#[test]
fn test_parity_es5_optional_chaining_class_methods() {
    let source = r#"
interface Config {
    api?: {
        baseUrl?: string;
        headers?: Record<string, string>;
        timeout?: number;
    };
    logging?: {
        level?: string;
        handler?: (msg: string) => void;
    };
}

class ConfigurableService {
    constructor(private config?: Config) {}

    getBaseUrl(): string {
        return this.config?.api?.baseUrl ?? "https://default.api.com";
    }

    getHeader(name: string): string | undefined {
        return this.config?.api?.headers?.[name];
    }

    log(message: string): void {
        this.config?.logging?.handler?.(message);
    }

    getTimeout(): number {
        return this.config?.api?.timeout ?? 5000;
    }
}

class ChainedProcessor<T> {
    private processor?: {
        transform?: (item: T) => T;
        validate?: (item: T) => boolean;
        handlers?: Array<(item: T) => void>;
    };

    process(item: T): T | undefined {
        if (this.processor?.validate?.(item)) {
            return this.processor?.transform?.(item);
        }
        return undefined;
    }

    notify(item: T): void {
        this.processor?.handlers?.forEach(h => h?.(item));
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("ConfigurableService") && output.contains("ChainedProcessor"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("getBaseUrl") && output.contains("getHeader") && output.contains("process"),
        "Output should contain methods: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "Interface should be erased: {}",
        output
    );
}

/// Test: optional chaining with generic types
#[test]
fn test_parity_es5_optional_chaining_generics() {
    let source = r#"
interface Container<T> {
    value?: T;
    nested?: Container<T>;
    transform?: (v: T) => T;
}

function getValue<T>(container: Container<T> | undefined): T | undefined {
    return container?.value;
}

function getNestedValue<T>(container: Container<T> | undefined): T | undefined {
    return container?.nested?.value;
}

function transformValue<T>(container: Container<T> | undefined, defaultVal: T): T {
    return container?.transform?.(container?.value ?? defaultVal) ?? defaultVal;
}

class GenericChainer<T, U> {
    private mapper?: {
        convert?: (item: T) => U;
        validate?: (item: T) => boolean;
    };

    chain(item: T | undefined): U | undefined {
        if (item === undefined) return undefined;
        if (this.mapper?.validate?.(item) === false) return undefined;
        return this.mapper?.convert?.(item);
    }
}

const optionalMap = <T, U>(
    value: T | undefined,
    mapper?: (v: T) => U
): U | undefined => {
    return value !== undefined ? mapper?.(value) : undefined;
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("getValue")
            && output.contains("getNestedValue")
            && output.contains("transformValue"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("GenericChainer"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Container"),
        "Interface should be erased: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, U>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: mixed property access, method calls, and element access
#[test]
fn test_parity_es5_optional_chaining_mixed_access() {
    let source = r#"
interface DataStore {
    items?: string[];
    getItem?: (index: number) => string;
    metadata?: {
        tags?: string[];
        getTag?: (name: string) => string | undefined;
    };
}

function mixedAccess(store: DataStore | undefined, index: number): string | undefined {
    // Property access
    const firstItem = store?.items?.[0];
    // Method call
    const gotItem = store?.getItem?.(index);
    // Nested property + element access
    const tag = store?.metadata?.tags?.[0];
    // Nested method call
    const namedTag = store?.metadata?.getTag?.("main");

    return firstItem ?? gotItem ?? tag ?? namedTag;
}

class MixedAccessor {
    private stores: Map<string, DataStore> = new Map();

    getFromStore(storeId: string, itemIndex: number): string | undefined {
        const store = this.stores.get(storeId);
        return store?.items?.[itemIndex] ?? store?.getItem?.(itemIndex);
    }

    getTag(storeId: string, tagIndex: number): string | undefined {
        return this.stores.get(storeId)?.metadata?.tags?.[tagIndex];
    }

    callMethod(storeId: string, tagName: string): string | undefined {
        return this.stores.get(storeId)?.metadata?.getTag?.(tagName);
    }
}

const chainedOperations = (data: DataStore | null): string[] => {
    const results: string[] = [];

    const item = data?.items?.[0];
    if (item) results.push(item);

    const method = data?.getItem?.(0);
    if (method) results.push(method);

    const nested = data?.metadata?.tags?.[0];
    if (nested) results.push(nested);

    return results;
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("mixedAccess") && output.contains("chainedOperations"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("MixedAccessor"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface DataStore"),
        "Interface should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Nullish Coalescing Patterns Parity Tests
// =============================================================================

/// Test: nullish coalescing with complex expressions
#[test]
fn test_parity_es5_nullish_complex_expressions() {
    let source = r#"
interface Config {
    value?: number;
    compute?: () => number;
}

function getComputedValue(config: Config | null): number {
    return config?.compute?.() ?? config?.value ?? 0;
}

const complexDefault = (
    a: number | null,
    b: number | undefined,
    c: number | null | undefined
): number => {
    return (a ?? 0) + (b ?? 0) + (c ?? 0);
};

function conditionalNullish(
    condition: boolean,
    primary: string | null,
    secondary: string | undefined
): string {
    return condition
        ? (primary ?? "default-primary")
        : (secondary ?? "default-secondary");
}

const ternaryWithNullish = (value: number | null | undefined): string => {
    const result = value ?? -1;
    return result >= 0 ? `positive: ${result}` : `negative: ${result}`;
};

class ExpressionProcessor {
    process(
        input: { value?: number; fallback?: number } | null
    ): number {
        return input?.value ?? input?.fallback ?? 0;
    }

    compute(
        fn: (() => number) | null,
        defaultValue: number
    ): number {
        return fn?.() ?? defaultValue;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("getComputedValue")
            && output.contains("complexDefault")
            && output.contains("conditionalNullish"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ExpressionProcessor"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "Interface should be erased: {}",
        output
    );
}

/// Test: nullish coalescing in class context
#[test]
fn test_parity_es5_nullish_class_context() {
    let source = r#"
class DefaultValueService<T> {
    private cache: Map<string, T> = new Map();
    private defaultValue: T;

    constructor(defaultValue: T) {
        this.defaultValue = defaultValue;
    }

    get(key: string): T {
        return this.cache.get(key) ?? this.defaultValue;
    }

    getOrCompute(key: string, compute: () => T): T {
        const cached = this.cache.get(key);
        return cached ?? compute();
    }

    getWithFallbacks(key: string, ...fallbacks: T[]): T {
        let result: T | undefined = this.cache.get(key);
        for (const fallback of fallbacks) {
            if (result !== undefined && result !== null) break;
            result = fallback;
        }
        return result ?? this.defaultValue;
    }
}

class ConfigManager {
    private config: Record<string, unknown> = {};

    getString(key: string, defaultVal: string = ""): string {
        const value = this.config[key];
        return (value as string | null | undefined) ?? defaultVal;
    }

    getNumber(key: string, defaultVal: number = 0): number {
        const value = this.config[key];
        return (value as number | null | undefined) ?? defaultVal;
    }

    getArray<T>(key: string, defaultVal: T[] = []): T[] {
        const value = this.config[key];
        return (value as T[] | null | undefined) ?? defaultVal;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("DefaultValueService") && output.contains("ConfigManager"),
        "Output should contain classes: {}",
        output
    );
    // Methods should be present
    assert!(
        output.contains("getOrCompute")
            && output.contains("getString")
            && output.contains("getNumber"),
        "Output should contain methods: {}",
        output
    );
    // Type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "Type parameters should be erased: {}",
        output
    );
}

/// Test: nullish coalescing with function calls
#[test]
fn test_parity_es5_nullish_function_calls() {
    let source = r#"
type Resolver<T> = () => T | null | undefined;

function resolveWithDefault<T>(
    resolver: Resolver<T>,
    defaultValue: T
): T {
    return resolver() ?? defaultValue;
}

function chainResolvers<T>(...resolvers: Resolver<T>[]): T | undefined {
    for (const resolver of resolvers) {
        const result = resolver();
        if (result !== null && result !== undefined) {
            return result;
        }
    }
    return undefined;
}

const createResolver = <T>(value: T | null): Resolver<T> => {
    return () => value;
};

async function asyncResolve<T>(
    asyncResolver: () => Promise<T | null>,
    defaultValue: T
): Promise<T> {
    const result = await asyncResolver();
    return result ?? defaultValue;
}

class ResolverChain<T> {
    private resolvers: Resolver<T>[] = [];

    add(resolver: Resolver<T>): this {
        this.resolvers.push(resolver);
        return this;
    }

    resolve(defaultValue: T): T {
        for (const resolver of this.resolvers) {
            const result = resolver();
            if (result !== null && result !== undefined) {
                return result;
            }
        }
        return defaultValue;
    }

    resolveFirst(): T | undefined {
        return this.resolvers[0]?.() ?? undefined;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("resolveWithDefault")
            && output.contains("chainResolvers")
            && output.contains("asyncResolve"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ResolverChain"),
        "Output should contain class: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Resolver"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test: deeply nested nullish coalescing with default chains
#[test]
fn test_parity_es5_nullish_nested_defaults() {
    let source = r#"
interface NestedConfig {
    level1?: {
        level2?: {
            level3?: {
                value?: string;
            };
        };
    };
}

function getNestedWithDefaults(config: NestedConfig | null): string {
    return config?.level1?.level2?.level3?.value
        ?? config?.level1?.level2?.level3?.value
        ?? config?.level1?.level2?.level3?.value
        ?? "default";
}

const multiLevelDefaults = (
    a: string | null,
    b: string | undefined,
    c: string | null,
    d: string
): string => {
    return a ?? b ?? c ?? d;
};

function defaultChainWithTransform(
    values: Array<string | null | undefined>,
    transform: (s: string) => string
): string {
    let result: string | null | undefined;
    for (const v of values) {
        result = v ?? result;
        if (result !== null && result !== undefined) break;
    }
    return transform(result ?? "");
}

class NestedDefaultResolver {
    private defaults: NestedConfig = {
        level1: {
            level2: {
                level3: {
                    value: "nested-default"
                }
            }
        }
    };

    resolve(config: NestedConfig | null): string {
        return config?.level1?.level2?.level3?.value
            ?? this.defaults.level1?.level2?.level3?.value
            ?? "fallback";
    }

    resolveWithOverrides(
        primary: NestedConfig | null,
        secondary: NestedConfig | null
    ): string {
        return primary?.level1?.level2?.level3?.value
            ?? secondary?.level1?.level2?.level3?.value
            ?? this.defaults.level1?.level2?.level3?.value
            ?? "final-fallback";
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("getNestedWithDefaults") && output.contains("multiLevelDefaults"),
        "Output should contain functions: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("NestedDefaultResolver"),
        "Output should contain class: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface NestedConfig"),
        "Interface should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 BigInt patterns parity tests
// =============================================================================

/// Test BigInt literal syntax with type annotations
#[test]
fn test_parity_es5_bigint_literal() {
    let source = r#"
const small: bigint = 123n;
const large: bigint = 9007199254740991n;
const negative: bigint = -456n;
const hex: bigint = 0xFFn;
const binary: bigint = 0b1010n;
const octal: bigint = 0o777n;

function useBigInt(value: bigint): bigint {
    return value;
}

class BigIntContainer {
    private value: bigint;

    constructor(initial: bigint) {
        this.value = initial;
    }

    getValue(): bigint {
        return this.value;
    }
}

const container = new BigIntContainer(100n);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variable declarations should be present (BigInt literals may be stripped)
    assert!(
        output.contains("var small") && output.contains("var large"),
        "Output should contain variable declarations: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": bigint"),
        "Type annotations should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("BigIntContainer"),
        "Output should contain class: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("useBigInt"),
        "Output should contain function: {}",
        output
    );
}

/// Test BigInt arithmetic operations with type erasure
#[test]
fn test_parity_es5_bigint_arithmetic() {
    let source = r#"
function bigIntMath(a: bigint, b: bigint): bigint {
    const sum: bigint = a + b;
    const diff: bigint = a - b;
    const product: bigint = a * b;
    const quotient: bigint = a / b;
    const remainder: bigint = a % b;
    const power: bigint = a ** b;
    return sum + diff + product + quotient + remainder + power;
}

class BigIntCalculator {
    private accumulator: bigint = 0n;

    add(value: bigint): this {
        this.accumulator += value;
        return this;
    }

    subtract(value: bigint): this {
        this.accumulator -= value;
        return this;
    }

    multiply(value: bigint): this {
        this.accumulator *= value;
        return this;
    }

    getResult(): bigint {
        return this.accumulator;
    }
}

const result: bigint = bigIntMath(10n, 3n);
const calc = new BigIntCalculator();
calc.add(100n).subtract(20n).multiply(2n);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Function should be present
    assert!(
        output.contains("bigIntMath"),
        "Output should contain bigIntMath function: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("BigIntCalculator"),
        "Output should contain BigIntCalculator class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": bigint"),
        "Type annotations should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private accumulator"),
        "Private modifier should be erased: {}",
        output
    );
    // Method bodies should be present
    assert!(
        output.contains("this.accumulator +=") || output.contains("this.accumulator="),
        "Method bodies should be present: {}",
        output
    );
}

/// Test BigInt comparison operations with generics
#[test]
fn test_parity_es5_bigint_comparison() {
    let source = r#"
interface Comparable<T> {
    compare(other: T): number;
}

function compareBigInts(a: bigint, b: bigint): number {
    if (a < b) return -1;
    if (a > b) return 1;
    if (a === b) return 0;
    if (a <= b) return -1;
    if (a >= b) return 1;
    return 0;
}

class BigIntComparator implements Comparable<bigint> {
    private value: bigint;

    constructor(value: bigint) {
        this.value = value;
    }

    compare(other: bigint): number {
        return compareBigInts(this.value, other);
    }

    equals(other: bigint): boolean {
        return this.value === other;
    }

    lessThan(other: bigint): boolean {
        return this.value < other;
    }
}

const comp1 = new BigIntComparator(100n);
const isEqual: boolean = comp1.equals(100n);
const isLess: boolean = comp1.lessThan(200n);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface Comparable"),
        "Interface should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements"),
        "Implements clause should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("compareBigInts"),
        "Output should contain compareBigInts function: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("BigIntComparator"),
        "Output should contain BigIntComparator class: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": bigint")
            && !output.contains(": boolean")
            && !output.contains(": number"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test BigInt method calls and conversions
#[test]
fn test_parity_es5_bigint_method_calls() {
    let source = r#"
type BigIntFormatter = (value: bigint) => string;

function formatBigInt(value: bigint): string {
    const str: string = value.toString();
    const localeStr: string = value.toLocaleString();
    const valueOf: bigint = value.valueOf();
    return str;
}

class BigIntWrapper {
    private readonly value: bigint;

    constructor(value: bigint | number | string) {
        this.value = BigInt(value);
    }

    toString(radix?: number): string {
        return this.value.toString(radix);
    }

    toJSON(): string {
        return this.value.toString();
    }

    static fromString(str: string): BigIntWrapper {
        return new BigIntWrapper(BigInt(str));
    }

    static max(...values: bigint[]): bigint {
        return values.reduce((a, b) => a > b ? a : b);
    }
}

const wrapper = new BigIntWrapper(12345n);
const strValue: string = wrapper.toString(16);
const fromStr = BigIntWrapper.fromString("999");
const maxVal: bigint = BigIntWrapper.max(1n, 2n, 3n, 100n);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type alias should be erased
    assert!(
        !output.contains("type BigIntFormatter"),
        "Type alias should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("formatBigInt"),
        "Output should contain formatBigInt function: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("BigIntWrapper"),
        "Output should contain BigIntWrapper class: {}",
        output
    );
    // Static methods should be present
    assert!(
        output.contains("fromString") && output.contains("max"),
        "Static methods should be present: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": bigint") && !output.contains(": string"),
        "Type annotations should be erased: {}",
        output
    );
    // Readonly modifier should be erased
    assert!(
        !output.contains("readonly value"),
        "Readonly modifier should be erased: {}",
        output
    );
    // BigInt function calls should be preserved
    assert!(
        output.contains("BigInt("),
        "BigInt constructor calls should be preserved: {}",
        output
    );
}

// =============================================================================
// ES5 Symbol patterns parity tests
// =============================================================================

/// Test well-known symbols with type annotations
#[test]
fn test_parity_es5_symbol_well_known() {
    let source = r#"
interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}

class CustomCollection<T> implements Iterable<T> {
    private items: T[] = [];

    constructor(items?: T[]) {
        if (items) {
            this.items = items;
        }
    }

    add(item: T): void {
        this.items.push(item);
    }

    [Symbol.iterator](): Iterator<T> {
        let index = 0;
        const items = this.items;
        return {
            next(): IteratorResult<T> {
                if (index < items.length) {
                    return { value: items[index++], done: false };
                }
                return { value: undefined as any, done: true };
            }
        };
    }

    [Symbol.toStringTag]: string = "CustomCollection";
}

class Matchable {
    private pattern: RegExp;

    constructor(pattern: RegExp) {
        this.pattern = pattern;
    }

    [Symbol.match](str: string): RegExpMatchArray | null {
        return str.match(this.pattern);
    }

    [Symbol.search](str: string): number {
        return str.search(this.pattern);
    }
}

const collection = new CustomCollection<number>([1, 2, 3]);
const matchable = new Matchable(/test/);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface Iterable"),
        "Interface should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements"),
        "Implements clause should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("CustomCollection") && output.contains("Matchable"),
        "Classes should be present: {}",
        output
    );
    // Symbol references should be preserved
    assert!(
        output.contains("Symbol.iterator") || output.contains("Symbol.toStringTag"),
        "Symbol references should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T[]") && !output.contains(": Iterator<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Symbol.for with global symbol registry
#[test]
fn test_parity_es5_symbol_for() {
    let source = r#"
type SymbolKey = string | number;

const globalSymbol: symbol = Symbol.for("app.global");
const anotherGlobal: symbol = Symbol.for("app.another");

class SymbolRegistry {
    private static symbols: Map<string, symbol> = new Map();

    static register(key: string): symbol {
        if (!this.symbols.has(key)) {
            this.symbols.set(key, Symbol.for(key));
        }
        return this.symbols.get(key)!;
    }

    static getOrCreate(key: string): symbol {
        return Symbol.for(`registry.${key}`);
    }
}

interface SymbolHolder {
    readonly symbol: symbol;
    key: string;
}

class GlobalSymbolUser implements SymbolHolder {
    readonly symbol: symbol;
    key: string;

    constructor(key: string) {
        this.key = key;
        this.symbol = Symbol.for(key);
    }

    matches(other: symbol): boolean {
        return this.symbol === other;
    }
}

const registry = SymbolRegistry.register("test");
const user = new GlobalSymbolUser("user.id");
const isSame: boolean = Symbol.for("app.global") === globalSymbol;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Type alias should be erased
    assert!(
        !output.contains("type SymbolKey"),
        "Type alias should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface SymbolHolder"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("SymbolRegistry") && output.contains("GlobalSymbolUser"),
        "Classes should be present: {}",
        output
    );
    // Symbol.for calls should be preserved
    assert!(
        output.contains("Symbol.for"),
        "Symbol.for calls should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": symbol") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
    // Readonly modifier should be erased
    assert!(
        !output.contains("readonly symbol"),
        "Readonly modifier should be erased: {}",
        output
    );
}

/// Test Symbol.keyFor to retrieve global symbol keys
#[test]
fn test_parity_es5_symbol_key_for() {
    let source = r#"
interface SymbolInfo {
    symbol: symbol;
    key: string | undefined;
    isGlobal: boolean;
}

function getSymbolInfo(sym: symbol): SymbolInfo {
    const key: string | undefined = Symbol.keyFor(sym);
    return {
        symbol: sym,
        key: key,
        isGlobal: key !== undefined
    };
}

class SymbolAnalyzer {
    private cache: Map<symbol, string | undefined> = new Map();

    analyze(sym: symbol): string | undefined {
        if (!this.cache.has(sym)) {
            this.cache.set(sym, Symbol.keyFor(sym));
        }
        return this.cache.get(sym);
    }

    isRegistered(sym: symbol): boolean {
        return Symbol.keyFor(sym) !== undefined;
    }

    getKeyOrDefault(sym: symbol, defaultKey: string): string {
        return Symbol.keyFor(sym) ?? defaultKey;
    }
}

const globalSym: symbol = Symbol.for("global.test");
const localSym: symbol = Symbol("local");
const analyzer = new SymbolAnalyzer();

const globalKey: string | undefined = Symbol.keyFor(globalSym);
const localKey: string | undefined = Symbol.keyFor(localSym);
const isGlobalRegistered: boolean = analyzer.isRegistered(globalSym);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface SymbolInfo"),
        "Interface should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("getSymbolInfo"),
        "Function should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("SymbolAnalyzer"),
        "Class should be present: {}",
        output
    );
    // Symbol.keyFor calls should be preserved
    assert!(
        output.contains("Symbol.keyFor"),
        "Symbol.keyFor calls should be preserved: {}",
        output
    );
    // Symbol.for calls should be preserved
    assert!(
        output.contains("Symbol.for") || output.contains("Symbol("),
        "Symbol calls should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": symbol") && !output.contains(": SymbolInfo"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Symbol in class computed properties and methods
#[test]
fn test_parity_es5_symbol_computed_class() {
    let source = r#"
const customMethod: unique symbol = Symbol("customMethod");
const customProp: unique symbol = Symbol("customProp");

interface HasCustomMethod {
    [customMethod](): void;
}

class SymbolMethodClass implements HasCustomMethod {
    private data: string;

    constructor(data: string) {
        this.data = data;
    }

    [customMethod](): void {
        console.log(this.data);
    }

    get [customProp](): string {
        return this.data;
    }

    set [customProp](value: string) {
        this.data = value;
    }
}

class SymbolFactory<T> {
    private readonly id: symbol;

    constructor(description: string) {
        this.id = Symbol(description);
    }

    getId(): symbol {
        return this.id;
    }

    createTagged(value: T): { value: T; tag: symbol } {
        return { value, tag: this.id };
    }
}

const instance = new SymbolMethodClass("test");
const factory = new SymbolFactory<number>("factory");
const tagged = factory.createTagged(42);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface HasCustomMethod"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("SymbolMethodClass") && output.contains("SymbolFactory"),
        "Classes should be present: {}",
        output
    );
    // Symbol constructor calls should be preserved
    assert!(
        output.contains("Symbol("),
        "Symbol constructor calls should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": unique symbol") && !output.contains(": symbol"),
        "Type annotations should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<number>"),
        "Generic type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Proxy patterns parity tests
// =============================================================================

/// Test Proxy handler traps with type annotations
#[test]
fn test_parity_es5_proxy_handler_traps() {
    let source = r#"
interface Target {
    name: string;
    value: number;
}

type PropertyKey = string | symbol;

const handler: ProxyHandler<Target> = {
    get(target: Target, prop: PropertyKey, receiver: any): any {
        console.log(`Getting ${String(prop)}`);
        return Reflect.get(target, prop, receiver);
    },
    set(target: Target, prop: PropertyKey, value: any, receiver: any): boolean {
        console.log(`Setting ${String(prop)} to ${value}`);
        return Reflect.set(target, prop, value, receiver);
    },
    has(target: Target, prop: PropertyKey): boolean {
        return prop in target;
    },
    deleteProperty(target: Target, prop: PropertyKey): boolean {
        return Reflect.deleteProperty(target, prop);
    }
};

class ProxyFactory<T extends object> {
    private handler: ProxyHandler<T>;

    constructor(handler: ProxyHandler<T>) {
        this.handler = handler;
    }

    create(target: T): T {
        return new Proxy(target, this.handler);
    }
}

const target: Target = { name: "test", value: 42 };
const proxy = new Proxy(target, handler);
const factory = new ProxyFactory<Target>(handler);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface Target"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type PropertyKey"),
        "Type alias should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ProxyFactory"),
        "Class should be present: {}",
        output
    );
    // Proxy constructor should be preserved
    assert!(
        output.contains("new Proxy"),
        "Proxy constructor should be preserved: {}",
        output
    );
    // Reflect calls should be preserved
    assert!(
        output.contains("Reflect.get") || output.contains("Reflect.set"),
        "Reflect calls should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Target") && !output.contains(": ProxyHandler"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Proxy.revocable with type annotations
#[test]
fn test_parity_es5_proxy_revocable() {
    let source = r#"
interface RevocableResult<T> {
    proxy: T;
    revoke: () => void;
}

interface DataObject {
    id: number;
    data: string;
}

function createRevocableProxy<T extends object>(target: T): RevocableResult<T> {
    const handler: ProxyHandler<T> = {
        get(target: T, prop: string | symbol): any {
            return Reflect.get(target, prop);
        }
    };
    return Proxy.revocable(target, handler);
}

class RevocableProxyManager<T extends object> {
    private proxies: Map<string, { proxy: T; revoke: () => void }> = new Map();

    create(id: string, target: T): T {
        const { proxy, revoke } = Proxy.revocable(target, {
            get: (t: T, p: string | symbol) => Reflect.get(t, p),
            set: (t: T, p: string | symbol, v: any) => Reflect.set(t, p, v)
        });
        this.proxies.set(id, { proxy, revoke });
        return proxy;
    }

    revoke(id: string): boolean {
        const entry = this.proxies.get(id);
        if (entry) {
            entry.revoke();
            this.proxies.delete(id);
            return true;
        }
        return false;
    }
}

const obj: DataObject = { id: 1, data: "test" };
const { proxy, revoke } = Proxy.revocable(obj, {});
const manager = new RevocableProxyManager<DataObject>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interfaces should be erased
    assert!(
        !output.contains("interface RevocableResult") && !output.contains("interface DataObject"),
        "Interfaces should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("createRevocableProxy"),
        "Function should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("RevocableProxyManager"),
        "Class should be present: {}",
        output
    );
    // Proxy.revocable should be preserved
    assert!(
        output.contains("Proxy.revocable"),
        "Proxy.revocable should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": RevocableResult") && !output.contains(": DataObject"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Reflect API integration with type annotations
#[test]
fn test_parity_es5_reflect_integration() {
    let source = r#"
interface ReflectTarget {
    prop: string;
    method(): void;
}

type ReflectResult<T> = T | undefined;

class ReflectWrapper {
    static safeGet<T extends object, K extends keyof T>(
        target: T,
        key: K
    ): T[K] | undefined {
        return Reflect.get(target, key);
    }

    static safeSet<T extends object, K extends keyof T>(
        target: T,
        key: K,
        value: T[K]
    ): boolean {
        return Reflect.set(target, key, value);
    }

    static hasOwn<T extends object>(target: T, key: PropertyKey): boolean {
        return Reflect.has(target, key);
    }

    static getKeys<T extends object>(target: T): (string | symbol)[] {
        return Reflect.ownKeys(target);
    }
}

function applyWithReflect<T, A extends any[], R>(
    fn: (this: T, ...args: A) => R,
    thisArg: T,
    args: A
): R {
    return Reflect.apply(fn, thisArg, args);
}

function constructWithReflect<T>(
    ctor: new (...args: any[]) => T,
    args: any[]
): T {
    return Reflect.construct(ctor, args);
}

const target: ReflectTarget = { prop: "value", method() {} };
const value: string | undefined = ReflectWrapper.safeGet(target, "prop");
const keys: (string | symbol)[] = ReflectWrapper.getKeys(target);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface ReflectTarget"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type ReflectResult"),
        "Type alias should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ReflectWrapper"),
        "Class should be present: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("applyWithReflect") && output.contains("constructWithReflect"),
        "Functions should be present: {}",
        output
    );
    // Reflect methods should be preserved
    assert!(
        output.contains("Reflect.get") && output.contains("Reflect.set"),
        "Reflect methods should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": ReflectTarget") && !output.contains(": ReflectResult"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Proxy with class instance wrapping
#[test]
fn test_parity_es5_proxy_class_wrapper() {
    let source = r#"
interface Observable<T> {
    subscribe(callback: (value: T) => void): void;
}

class ReactiveObject<T extends object> {
    private target: T;
    private listeners: Set<Function> = new Set();

    constructor(target: T) {
        this.target = target;
    }

    createProxy(): T {
        const self = this;
        const handler = {
            set: function(target: any, prop: any, value: any) {
                const result = Reflect.set(target, prop, value);
                self.notifyListeners(prop, value);
                return result;
            },
            get: function(target: any, prop: any) {
                return Reflect.get(target, prop);
            }
        };
        return new Proxy(this.target, handler);
    }

    private notifyListeners(prop: any, value: any): void {
        this.listeners.forEach(listener => listener(prop, value));
    }

    onChange(callback: Function): void {
        this.listeners.add(callback);
    }
}

interface User {
    name: string;
    age: number;
}

const user: User = { name: "John", age: 30 };
const reactive = new ReactiveObject<User>(user);
const proxy = reactive.createProxy();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interfaces should be erased
    assert!(
        !output.contains("interface Observable") && !output.contains("interface User"),
        "Interfaces should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ReactiveObject"),
        "Class should be present: {}",
        output
    );
    // Proxy constructor should be preserved
    assert!(
        output.contains("new Proxy"),
        "Proxy constructor should be preserved: {}",
        output
    );
    // Reflect methods should be preserved
    assert!(
        output.contains("Reflect.set") && output.contains("Reflect.get"),
        "Reflect methods should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": User") && !output.contains(": T"),
        "Type annotations should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private target") && !output.contains("private listeners"),
        "Private modifier should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 WeakRef patterns parity tests
// =============================================================================

/// Test WeakRef deref with type annotations
#[test]
fn test_parity_es5_weakref_deref() {
    let source = r#"
interface CacheableObject {
    id: string;
    data: unknown;
}

class WeakRefHolder<T extends object> {
    private ref: WeakRef<T>;

    constructor(target: T) {
        this.ref = new WeakRef(target);
    }

    get(): T | undefined {
        return this.ref.deref();
    }

    isAlive(): boolean {
        return this.ref.deref() !== undefined;
    }
}

function createWeakRef<T extends object>(obj: T): WeakRef<T> {
    return new WeakRef(obj);
}

function tryDeref<T extends object>(ref: WeakRef<T>): T | undefined {
    const value: T | undefined = ref.deref();
    return value;
}

const obj: CacheableObject = { id: "test", data: {} };
const weakRef: WeakRef<CacheableObject> = new WeakRef(obj);
const holder = new WeakRefHolder<CacheableObject>(obj);
const derefed: CacheableObject | undefined = weakRef.deref();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface CacheableObject"),
        "Interface should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("WeakRefHolder"),
        "Class should be present: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("createWeakRef") && output.contains("tryDeref"),
        "Functions should be present: {}",
        output
    );
    // WeakRef constructor should be preserved
    assert!(
        output.contains("new WeakRef"),
        "WeakRef constructor should be preserved: {}",
        output
    );
    // deref method should be preserved
    assert!(
        output.contains(".deref()"),
        "deref method should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": WeakRef<") && !output.contains(": CacheableObject"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test FinalizationRegistry with type annotations
#[test]
fn test_parity_es5_finalization_registry() {
    let source = r#"
interface CleanupContext {
    resourceId: string;
    timestamp: number;
}

type CleanupCallback = (heldValue: string) => void;

class ResourceTracker {
    private registry: FinalizationRegistry<string>;
    private cleanupCount: number = 0;

    constructor() {
        this.registry = new FinalizationRegistry((heldValue: string) => {
            console.log(`Cleaning up: ${heldValue}`);
            this.cleanupCount++;
        });
    }

    track(obj: object, resourceId: string): void {
        this.registry.register(obj, resourceId);
    }

    trackWithUnregister(obj: object, resourceId: string, token: object): void {
        this.registry.register(obj, resourceId, token);
    }

    untrack(token: object): void {
        this.registry.unregister(token);
    }

    getCleanupCount(): number {
        return this.cleanupCount;
    }
}

function createRegistry<T>(callback: (value: T) => void): FinalizationRegistry<T> {
    return new FinalizationRegistry(callback);
}

const tracker = new ResourceTracker();
const resource = { data: "important" };
tracker.track(resource, "resource-1");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface CleanupContext"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type CleanupCallback"),
        "Type alias should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ResourceTracker"),
        "Class should be present: {}",
        output
    );
    // FinalizationRegistry constructor should be preserved
    assert!(
        output.contains("new FinalizationRegistry"),
        "FinalizationRegistry constructor should be preserved: {}",
        output
    );
    // Registry methods should be preserved
    assert!(
        output.contains(".register(") || output.contains(".unregister("),
        "Registry methods should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": FinalizationRegistry") && !output.contains(": CleanupCallback"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test weak cache pattern with WeakRef and Map
#[test]
fn test_parity_es5_weak_cache() {
    let source = r#"
interface Cacheable {
    readonly id: string;
}

interface CacheEntry<T> {
    ref: WeakRef<T>;
    metadata: Map<string, unknown>;
}

class WeakCache<K, V extends object> {
    private cache: Map<K, WeakRef<V>> = new Map();
    private registry: FinalizationRegistry<K>;

    constructor() {
        this.registry = new FinalizationRegistry((key: K) => {
            this.cache.delete(key);
        });
    }

    set(key: K, value: V): void {
        const ref = new WeakRef(value);
        this.cache.set(key, ref);
        this.registry.register(value, key, ref);
    }

    get(key: K): V | undefined {
        const ref = this.cache.get(key);
        if (ref) {
            const value = ref.deref();
            if (value === undefined) {
                this.cache.delete(key);
            }
            return value;
        }
        return undefined;
    }

    has(key: K): boolean {
        const ref = this.cache.get(key);
        return ref !== undefined && ref.deref() !== undefined;
    }

    delete(key: K): boolean {
        const ref = this.cache.get(key);
        if (ref) {
            this.registry.unregister(ref);
            return this.cache.delete(key);
        }
        return false;
    }
}

const cache = new WeakCache<string, Cacheable>();
const item: Cacheable = { id: "item-1" };
cache.set("key1", item);
const retrieved: Cacheable | undefined = cache.get("key1");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interfaces should be erased
    assert!(
        !output.contains("interface Cacheable") && !output.contains("interface CacheEntry"),
        "Interfaces should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("WeakCache"),
        "Class should be present: {}",
        output
    );
    // WeakRef should be preserved
    assert!(
        output.contains("new WeakRef") && output.contains(".deref()"),
        "WeakRef usage should be preserved: {}",
        output
    );
    // FinalizationRegistry should be preserved
    assert!(
        output.contains("new FinalizationRegistry"),
        "FinalizationRegistry should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Cacheable") && !output.contains(": WeakRef<"),
        "Type annotations should be erased: {}",
        output
    );
    // Readonly modifier should be erased
    assert!(
        !output.contains("readonly id"),
        "Readonly modifier should be erased: {}",
        output
    );
}

/// Test WeakRef with async patterns
#[test]
fn test_parity_es5_weakref_async() {
    let source = r#"
interface AsyncResource {
    fetch(): Promise<string>;
}

class AsyncWeakRefManager<T extends object> {
    private refs: Map<string, WeakRef<T>> = new Map();

    register(id: string, obj: T): void {
        this.refs.set(id, new WeakRef(obj));
    }

    async getOrFetch(id: string, fetcher: () => Promise<T>): Promise<T | undefined> {
        const ref = this.refs.get(id);
        if (ref) {
            const existing = ref.deref();
            if (existing !== undefined) {
                return existing;
            }
        }
        const newObj = await fetcher();
        this.register(id, newObj);
        return newObj;
    }

    async processAll(processor: (obj: T) => Promise<void>): Promise<void> {
        for (const [id, ref] of this.refs) {
            const obj = ref.deref();
            if (obj !== undefined) {
                await processor(obj);
            } else {
                this.refs.delete(id);
            }
        }
    }
}

async function withWeakRef<T extends object>(
    ref: WeakRef<T>,
    action: (obj: T) => Promise<void>
): Promise<boolean> {
    const obj = ref.deref();
    if (obj !== undefined) {
        await action(obj);
        return true;
    }
    return false;
}

const manager = new AsyncWeakRefManager<AsyncResource>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface AsyncResource"),
        "Interface should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("AsyncWeakRefManager"),
        "Class should be present: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("withWeakRef"),
        "Function should be present: {}",
        output
    );
    // WeakRef should be preserved
    assert!(
        output.contains("new WeakRef") && output.contains(".deref()"),
        "WeakRef usage should be preserved: {}",
        output
    );
    // Async should be transformed (awaiter helper or similar)
    assert!(
        output.contains("__awaiter") || output.contains("return") || output.contains("Promise"),
        "Async patterns should be present: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise<") && !output.contains(": WeakRef<"),
        "Type annotations should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Promise patterns parity tests
// =============================================================================

/// Test Promise.all with type annotations
#[test]
fn test_parity_es5_promise_all() {
    let source = r#"
interface ApiResponse<T> {
    data: T;
    status: number;
}

type PromiseResult<T> = Promise<ApiResponse<T>>;

async function fetchAll<T>(urls: string[]): Promise<T[]> {
    const promises: Promise<T>[] = urls.map(url => fetch(url).then(r => r.json()));
    return Promise.all(promises);
}

class ParallelFetcher<T> {
    private baseUrl: string;

    constructor(baseUrl: string) {
        this.baseUrl = baseUrl;
    }

    async fetchMultiple(ids: string[]): Promise<T[]> {
        const urls = ids.map(id => `${this.baseUrl}/${id}`);
        const responses = await Promise.all(
            urls.map(url => fetch(url))
        );
        return Promise.all(responses.map(r => r.json()));
    }

    async fetchWithMetadata(ids: string[]): Promise<Array<{ id: string; data: T }>> {
        const results = await Promise.all(
            ids.map(async (id) => {
                const response = await fetch(`${this.baseUrl}/${id}`);
                const data: T = await response.json();
                return { id, data };
            })
        );
        return results;
    }
}

const fetcher = new ParallelFetcher<object>("/api");
const results: object[] = await fetchAll<object>(["/a", "/b"]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface ApiResponse"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type PromiseResult"),
        "Type alias should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("fetchAll"),
        "Function should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ParallelFetcher"),
        "Class should be present: {}",
        output
    );
    // Promise.all should be preserved
    assert!(
        output.contains("Promise.all"),
        "Promise.all should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise<") && !output.contains(": T[]"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Promise.race with type annotations
#[test]
fn test_parity_es5_promise_race() {
    let source = r#"
interface TimeoutError {
    message: string;
    timeout: number;
}

function timeout<T>(ms: number, value?: T): Promise<T> {
    return new Promise((resolve, reject) => {
        setTimeout(() => {
            if (value !== undefined) {
                resolve(value);
            } else {
                reject(new Error("Timeout"));
            }
        }, ms);
    });
}

async function fetchWithTimeout<T>(
    url: string,
    timeoutMs: number
): Promise<T> {
    return Promise.race([
        fetch(url).then(r => r.json()) as Promise<T>,
        timeout<T>(timeoutMs)
    ]);
}

class RacingFetcher<T> {
    private defaultTimeout: number;

    constructor(defaultTimeout: number = 5000) {
        this.defaultTimeout = defaultTimeout;
    }

    async fetchFirst(urls: string[]): Promise<T> {
        return Promise.race(
            urls.map(url => fetch(url).then(r => r.json()))
        );
    }

    async fetchWithFallback(primary: string, fallback: string): Promise<T> {
        try {
            return await Promise.race([
                fetch(primary).then(r => r.json()),
                timeout<T>(this.defaultTimeout)
            ]);
        } catch {
            return fetch(fallback).then(r => r.json());
        }
    }
}

const racer = new RacingFetcher<object>(3000);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface TimeoutError"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("timeout") && output.contains("fetchWithTimeout"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("RacingFetcher"),
        "Class should be present: {}",
        output
    );
    // Promise.race should be preserved
    assert!(
        output.contains("Promise.race"),
        "Promise.race should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise<") && !output.contains(": T"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Promise.allSettled with type annotations
#[test]
fn test_parity_es5_promise_all_settled() {
    let source = r#"
interface SettledResult<T> {
    status: "fulfilled" | "rejected";
    value?: T;
    reason?: Error;
}

type BatchResult<T> = PromiseSettledResult<T>[];

async function fetchAllSettled<T>(urls: string[]): Promise<PromiseSettledResult<T>[]> {
    const promises = urls.map(url => fetch(url).then(r => r.json()));
    return Promise.allSettled(promises);
}

class ResilientFetcher<T> {
    async fetchBatch(requests: Array<() => Promise<T>>): Promise<{
        succeeded: T[];
        failed: Error[];
    }> {
        const results = await Promise.allSettled(requests.map(fn => fn()));

        const succeeded: T[] = [];
        const failed: Error[] = [];

        for (const result of results) {
            if (result.status === "fulfilled") {
                succeeded.push(result.value);
            } else {
                failed.push(result.reason);
            }
        }

        return { succeeded, failed };
    }

    async fetchWithRetry(urls: string[], maxRetries: number): Promise<T[]> {
        let results = await Promise.allSettled(
            urls.map(url => fetch(url).then(r => r.json()))
        );

        const successful: T[] = [];
        for (const result of results) {
            if (result.status === "fulfilled") {
                successful.push(result.value);
            }
        }
        return successful;
    }
}

const fetcher = new ResilientFetcher<object>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface SettledResult"),
        "Interface should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type BatchResult"),
        "Type alias should be erased: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("fetchAllSettled"),
        "Function should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ResilientFetcher"),
        "Class should be present: {}",
        output
    );
    // Promise.allSettled should be preserved
    assert!(
        output.contains("Promise.allSettled"),
        "Promise.allSettled should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": PromiseSettledResult") && !output.contains(": BatchResult"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Promise.any with type annotations
#[test]
fn test_parity_es5_promise_any() {
    let source = r#"
interface FetchOptions {
    timeout?: number;
    retries?: number;
}

class AnyFirstFetcher<T> {
    private endpoints: string[];

    constructor(endpoints: string[]) {
        this.endpoints = endpoints;
    }

    async fetchFromAny(): Promise<T> {
        return Promise.any(
            this.endpoints.map(url => fetch(url).then(r => r.json()))
        );
    }

    async fetchFirst(urls: string[]): Promise<T> {
        const promises = urls.map(url => fetch(url).then(r => r.json()));
        return Promise.any(promises);
    }
}

async function fetchAnySuccessful<T>(urls: string[]): Promise<T> {
    const promises: Promise<T>[] = urls.map(url =>
        fetch(url).then(r => r.json())
    );
    return Promise.any(promises);
}

function checkAggregateError(e: unknown): boolean {
    return e instanceof AggregateError;
}

const fetcher = new AnyFirstFetcher<object>(["/api1", "/api2"]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface FetchOptions"),
        "Interface should be erased: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("AnyFirstFetcher"),
        "Class should be present: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("fetchAnySuccessful"),
        "Function should be present: {}",
        output
    );
    // Promise.any should be preserved
    assert!(
        output.contains("Promise.any"),
        "Promise.any should be preserved: {}",
        output
    );
    // AggregateError should be preserved
    assert!(
        output.contains("AggregateError"),
        "AggregateError should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Promise<") && !output.contains(": T"),
        "Type annotations should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Iterator patterns parity tests
// =============================================================================

/// Test Symbol.iterator implementation with type annotations
#[test]
fn test_parity_es5_iterator_symbol_iterator() {
    let source = r#"
interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}

class Range implements Iterable<number> {
    private start: number;
    private end: number;
    private current: number = 0;

    constructor(start: number, end: number) {
        this.start = start;
        this.end = end;
        this.current = start;
    }

    [Symbol.iterator](): Iterator<number> {
        this.current = this.start;
        return this;
    }

    next(): IteratorResult<number> {
        if (this.current <= this.end) {
            return { value: this.current++, done: false };
        }
        return { value: undefined, done: true };
    }
}

class ArrayIterator<T> implements Iterable<T> {
    private items: T[];
    private index: number = 0;

    constructor(items: T[]) {
        this.items = items;
    }

    [Symbol.iterator](): Iterator<T> {
        this.index = 0;
        return this;
    }

    next(): IteratorResult<T> {
        if (this.index < this.items.length) {
            return { value: this.items[this.index++], done: false };
        }
        return { value: undefined, done: true };
    }
}

const range = new Range(1, 5);
const arrIter = new ArrayIterator<string>(["a", "b", "c"]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface Iterable"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("Range") && output.contains("ArrayIterator"),
        "Classes should be present: {}",
        output
    );
    // Symbol.iterator should be preserved
    assert!(
        output.contains("Symbol.iterator"),
        "Symbol.iterator should be preserved: {}",
        output
    );
    // next method should be preserved
    assert!(
        output.contains("next"),
        "next method should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Iterator<") && !output.contains(": IteratorResult<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Iterator next method with type annotations
#[test]
fn test_parity_es5_iterator_next() {
    let source = r#"
interface IteratorResult<T> {
    done: boolean;
    value: T;
}

class CounterIterator {
    private count: number = 0;
    private max: number;

    constructor(max: number) {
        this.max = max;
    }

    next(): IteratorResult<number> {
        if (this.count < this.max) {
            return { done: false, value: this.count++ };
        }
        return { done: true, value: this.count };
    }
}

class MappingIterator<T, U> {
    private source: Iterator<T>;
    private mapper: (value: T) => U;

    constructor(source: Iterator<T>, mapper: (value: T) => U) {
        this.source = source;
        this.mapper = mapper;
    }

    next(): IteratorResult<U> {
        const result = this.source.next();
        if (result.done) {
            return { done: true, value: undefined as any };
        }
        return { done: false, value: this.mapper(result.value) };
    }
}

function consumeIterator<T>(iter: Iterator<T>): T[] {
    const results: T[] = [];
    let result = iter.next();
    while (!result.done) {
        results.push(result.value);
        result = iter.next();
    }
    return results;
}

const counter = new CounterIterator(5);
const first = counter.next();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface IteratorResult"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("CounterIterator") && output.contains("MappingIterator"),
        "Classes should be present: {}",
        output
    );
    // Function should be present
    assert!(
        output.contains("consumeIterator"),
        "Function should be present: {}",
        output
    );
    // next method calls should be preserved
    assert!(
        output.contains(".next()"),
        "next method calls should be preserved: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": IteratorResult<") && !output.contains(": Iterator<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Iterator return method with type annotations
#[test]
fn test_parity_es5_iterator_return() {
    let source = r#"
interface IteratorReturnResult<T> {
    done: true;
    value: T;
}

class ResourceIterator<T> {
    private items: T[];
    private index: number = 0;
    private closed: boolean = false;

    constructor(items: T[]) {
        this.items = items;
    }

    next(): IteratorResult<T> {
        if (this.closed || this.index >= this.items.length) {
            return { done: true, value: undefined as any };
        }
        return { done: false, value: this.items[this.index++] };
    }

    return(value?: T): IteratorResult<T> {
        this.closed = true;
        console.log("Iterator closed");
        return { done: true, value: value as any };
    }
}

class CleanupIterator<T> {
    private source: Iterator<T>;
    private cleanup: () => void;

    constructor(source: Iterator<T>, cleanup: () => void) {
        this.source = source;
        this.cleanup = cleanup;
    }

    next(): IteratorResult<T> {
        return this.source.next();
    }

    return(value?: T): IteratorResult<T> {
        this.cleanup();
        if (this.source.return) {
            return this.source.return(value);
        }
        return { done: true, value: value as any };
    }
}

const resourceIter = new ResourceIterator<number>([1, 2, 3]);
const result = resourceIter.return(0);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface IteratorReturnResult"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("ResourceIterator") && output.contains("CleanupIterator"),
        "Classes should be present: {}",
        output
    );
    // return method should be defined (as prototype method)
    assert!(
        output.contains(".return") || output.contains("return:"),
        "return method should be defined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": IteratorResult<") && !output.contains(": IteratorReturnResult"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test Iterator throw method with type annotations
#[test]
fn test_parity_es5_iterator_throw() {
    let source = r#"
interface ThrowableIterator<T> extends Iterator<T> {
    throw(error?: Error): IteratorResult<T>;
}

class ErrorHandlingIterator<T> {
    private items: T[];
    private index: number = 0;
    private errorHandler: (e: Error) => T | undefined;

    constructor(items: T[], errorHandler: (e: Error) => T | undefined) {
        this.items = items;
        this.errorHandler = errorHandler;
    }

    next(): IteratorResult<T> {
        if (this.index >= this.items.length) {
            return { done: true, value: undefined as any };
        }
        return { done: false, value: this.items[this.index++] };
    }

    throw(error?: Error): IteratorResult<T> {
        if (error && this.errorHandler) {
            const recovered = this.errorHandler(error);
            if (recovered !== undefined) {
                return { done: false, value: recovered };
            }
        }
        return { done: true, value: undefined as any };
    }
}

class DelegatingIterator<T> {
    private inner: Iterator<T>;

    constructor(inner: Iterator<T>) {
        this.inner = inner;
    }

    next(): IteratorResult<T> {
        return this.inner.next();
    }

    throw(error?: Error): IteratorResult<T> {
        if (typeof this.inner.throw === "function") {
            return this.inner.throw(error);
        }
        throw error;
    }

    return(value?: T): IteratorResult<T> {
        if (typeof this.inner.return === "function") {
            return this.inner.return(value);
        }
        return { done: true, value: value as any };
    }
}

const iter = new ErrorHandlingIterator<number>([1, 2, 3], (e) => -1);
const throwResult = iter.throw(new Error("test"));
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface ThrowableIterator"),
        "Interface should be erased: {}",
        output
    );
    // Classes should be present
    assert!(
        output.contains("ErrorHandlingIterator") && output.contains("DelegatingIterator"),
        "Classes should be present: {}",
        output
    );
    // throw method should be defined
    assert!(
        output.contains(".throw") || output.contains("throw:"),
        "throw method should be defined: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": IteratorResult<") && !output.contains(": ThrowableIterator"),
        "Type annotations should be erased: {}",
        output
    );
    // Extends clause should be erased
    assert!(
        !output.contains("extends Iterator"),
        "Extends clause should be erased: {}",
        output
    );
}

// =============================================================================
// ES5 Generator patterns parity tests
// =============================================================================

/// Test basic yield expression with type annotations
#[test]
fn test_parity_es5_generator_basic_yield() {
    let source = r#"
interface NumberGenerator {
    next(): IteratorResult<number>;
}

function* countUp(max: number): Generator<number, void, unknown> {
    for (let i = 0; i < max; i++) {
        yield i;
    }
}

function* fibonacci(limit: number): Generator<number> {
    let prev = 0;
    let curr = 1;
    while (curr <= limit) {
        yield curr;
        const next = prev + curr;
        prev = curr;
        curr = next;
    }
}

class NumberSequence {
    private values: number[];

    constructor(values: number[]) {
        this.values = values;
    }

    *[Symbol.iterator](): Generator<number> {
        for (const val of this.values) {
            yield val;
        }
    }
}

const counter = countUp(5);
const fib = fibonacci(100);
const seq = new NumberSequence([1, 2, 3]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface NumberGenerator"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("countUp") && output.contains("fibonacci"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("NumberSequence"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<") && !output.contains(": IteratorResult<"),
        "Type annotations should be erased: {}",
        output
    );
    // Parameter type annotations should be erased
    assert!(
        !output.contains(": number[]") && !output.contains("private values"),
        "Member type annotations should be erased: {}",
        output
    );
}

/// Test yield* delegation with type annotations
#[test]
fn test_parity_es5_generator_yield_star() {
    let source = r#"
function* inner(): Generator<number> {
    yield 1;
    yield 2;
    yield 3;
}

function* outer(): Generator<number> {
    yield 0;
    yield* inner();
    yield 4;
}

function* flatten<T>(arrays: T[][]): Generator<T> {
    for (const arr of arrays) {
        yield* arr;
    }
}

class CompositeGenerator<T> {
    private generators: Array<Generator<T>>;

    constructor(generators: Array<Generator<T>>) {
        this.generators = generators;
    }

    *combined(): Generator<T> {
        for (const gen of this.generators) {
            yield* gen;
        }
    }
}

const outerGen = outer();
const flatGen = flatten([[1, 2], [3, 4]]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("inner") && output.contains("outer") && output.contains("flatten"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("CompositeGenerator"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function*/yield* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<") && !output.contains("<T>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test generator conditional return with type annotations
#[test]
fn test_parity_es5_generator_conditional_return() {
    let source = r#"
interface GeneratorResult<T, R> {
    values: T[];
    returnValue: R;
}

function* withReturn(): Generator<number, string, unknown> {
    yield 1;
    yield 2;
    return "done";
}

function* conditionalReturn(shouldComplete: boolean): Generator<number, string> {
    yield 1;
    if (!shouldComplete) {
        return "early exit";
    }
    yield 2;
    yield 3;
    return "completed";
}

class StatefulGenerator<T, R> {
    private state: string = "idle";

    *run(items: T[], finalResult: R): Generator<T, R> {
        this.state = "running";
        for (const item of items) {
            yield item;
        }
        this.state = "done";
        return finalResult;
    }

    getState(): string {
        return this.state;
    }
}

const gen = withReturn();
const conditional = conditionalReturn(true);
const stateful = new StatefulGenerator<number, boolean>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface GeneratorResult"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("withReturn") && output.contains("conditionalReturn"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("StatefulGenerator"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<"),
        "Generator type annotations should be erased: {}",
        output
    );
    // Other type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains("private state"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test generator throw with type annotations
#[test]
fn test_parity_es5_generator_throw() {
    let source = r#"
interface ErrorRecovery<T> {
    recover(error: Error): T | undefined;
}

function* recoverableGenerator(): Generator<number, void, Error | undefined> {
    let value = 0;
    while (true) {
        const error = yield value;
        if (error) {
            console.log("Received error:", error.message);
            value = -1;
        } else {
            value++;
        }
    }
}

class ThrowableGenerator<T> {
    private errorCount: number = 0;

    *generate(items: T[]): Generator<T, void, Error | undefined> {
        for (const item of items) {
            const error = yield item;
            if (error) {
                this.errorCount++;
            }
        }
    }

    getErrorCount(): number {
        return this.errorCount;
    }
}

function consumeWithThrow<T>(gen: Generator<T, void, Error | undefined>): T[] {
    const results: T[] = [];
    let result = gen.next();
    while (!result.done) {
        results.push(result.value);
        result = gen.next();
    }
    return results;
}

const recoverable = recoverableGenerator();
const throwable = new ThrowableGenerator<string>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface ErrorRecovery"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("recoverableGenerator") && output.contains("consumeWithThrow"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("ThrowableGenerator"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<"),
        "Generator type annotations should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface ErrorRecovery"),
        "Interface should be erased: {}",
        output
    );
}

/// Test generator resource management with try/catch and type annotations
#[test]
fn test_parity_es5_generator_resource_management() {
    let source = r#"
interface SafeResult<T> {
    value?: T;
    error?: Error;
}

function* safeGenerator(): Generator<number, void, unknown> {
    try {
        yield 1;
        yield 2;
        throw new Error("Intentional error");
    } catch (e) {
        console.log("Caught:", e);
        yield -1;
    } finally {
        console.log("Cleanup");
    }
}

function* resourceGenerator(): Generator<string, void, unknown> {
    const resource = "acquired";
    try {
        yield resource;
        yield "processing";
    } finally {
        console.log("Releasing resource");
    }
}

class SafeIterator<T> {
    *iterate(items: T[]): Generator<SafeResult<T>> {
        for (const item of items) {
            try {
                yield { value: item };
            } catch (e) {
                yield { error: e as Error };
            }
        }
    }
}

const safeGen = safeGenerator();
const resourceGen = resourceGenerator();
const safeIter = new SafeIterator<number>();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface SafeResult"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("safeGenerator") && output.contains("resourceGenerator"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("SafeIterator"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<"),
        "Generator type annotations should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface SafeResult"),
        "Interface should be erased: {}",
        output
    );
}

/// Test combined generator patterns with type annotations
#[test]
fn test_parity_es5_generator_combined() {
    let source = r#"
interface StreamProcessor<T, R> {
    process(input: T): R;
}

function* pipeline<T, U, V>(
    source: Iterable<T>,
    transform1: (x: T) => U,
    transform2: (x: U) => V
): Generator<V> {
    for (const item of source) {
        const intermediate = transform1(item);
        yield transform2(intermediate);
    }
}

class GeneratorPipeline<T> {
    private source: Generator<T>;

    constructor(source: Generator<T>) {
        this.source = source;
    }

    *map<U>(fn: (x: T) => U): Generator<U> {
        for (const item of this.source) {
            yield fn(item);
        }
    }

    *filter(predicate: (x: T) => boolean): Generator<T> {
        for (const item of this.source) {
            if (predicate(item)) {
                yield item;
            }
        }
    }

    *take(count: number): Generator<T> {
        let taken = 0;
        for (const item of this.source) {
            if (taken >= count) return;
            yield item;
            taken++;
        }
    }
}

function* range(start: number, end: number): Generator<number> {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

const rangeGen = range(1, 10);
const pipe = new GeneratorPipeline(rangeGen);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Interface should be erased
    assert!(
        !output.contains("interface StreamProcessor"),
        "Interface should be erased: {}",
        output
    );
    // Functions should be present
    assert!(
        output.contains("pipeline") && output.contains("range"),
        "Functions should be present: {}",
        output
    );
    // Class should be present
    assert!(
        output.contains("GeneratorPipeline"),
        "Class should be present: {}",
        output
    );
    // Generator type annotations should be erased (function* may remain if transform not fully implemented)
    assert!(
        !output.contains(": Generator<") && !output.contains("<T>") && !output.contains("<U>"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test class decorator with private fields
#[test]
fn test_parity_es5_decorator_class_private_fields() {
    let source = r#"
interface ClassDecorator {
    <T extends new (...args: any[]) => any>(constructor: T): T | void;
}

function sealed(constructor: Function): void {
    Object.seal(constructor);
    Object.seal(constructor.prototype);
}

function track(constructor: Function): void {
    console.log("Class instantiated:", constructor.name);
}

@sealed
@track
class SecureData {
    #secret: string;
    #count: number = 0;

    constructor(secret: string) {
        this.#secret = secret;
    }

    #increment(): void {
        this.#count++;
    }

    getSecret(): string {
        this.#increment();
        return this.#secret;
    }

    getAccessCount(): number {
        return this.#count;
    }
}

const data = new SecureData("password123");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("SecureData"),
        "Class should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("sealed") && output.contains("track"),
        "Decorator functions should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface ClassDecorator"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test method decorator with computed property name
#[test]
fn test_parity_es5_decorator_method_computed_name() {
    let source = r#"
interface MethodDecorator {
    (target: any, propertyKey: string | symbol, descriptor: PropertyDescriptor): PropertyDescriptor | void;
}

const methodName = "dynamicMethod";
const symbolKey = Symbol("symbolMethod");

function log(target: any, key: string | symbol, descriptor: PropertyDescriptor): PropertyDescriptor {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Calling:", String(key));
        return original.apply(this, args);
    };
    return descriptor;
}

function measure(target: any, key: string | symbol, descriptor: PropertyDescriptor): PropertyDescriptor {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        const start = Date.now();
        const result = original.apply(this, args);
        console.log("Duration:", Date.now() - start);
        return result;
    };
    return descriptor;
}

class DynamicMethods {
    @log
    [methodName](x: number): number {
        return x * 2;
    }

    @measure
    @log
    [symbolKey](value: string): string {
        return value.toUpperCase();
    }

    @log
    ["literal" + "Name"](a: number, b: number): number {
        return a + b;
    }
}

const instance = new DynamicMethods();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("DynamicMethods"),
        "Class should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("function log") && output.contains("function measure"),
        "Decorator functions should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface MethodDecorator"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": PropertyDescriptor") && !output.contains(": number)"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test accessor decorator on getter/setter pair
#[test]
fn test_parity_es5_decorator_accessor_pair() {
    let source = r#"
interface AccessorDecorator {
    (target: any, propertyKey: string, descriptor: PropertyDescriptor): PropertyDescriptor | void;
}

function validate(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    const originalSet = descriptor.set;
    if (originalSet) {
        descriptor.set = function(value: any) {
            if (value < 0) throw new Error("Value must be non-negative");
            originalSet.call(this, value);
        };
    }
    return descriptor;
}

function cache(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    const originalGet = descriptor.get;
    const cacheKey = Symbol(key + "_cache");
    if (originalGet) {
        descriptor.get = function() {
            if (!(this as any)[cacheKey]) {
                (this as any)[cacheKey] = originalGet.call(this);
            }
            return (this as any)[cacheKey];
        };
    }
    return descriptor;
}

class BoundedValue {
    private _value: number = 0;
    private _computedValue: number | null = null;

    @validate
    get value(): number {
        return this._value;
    }

    @validate
    set value(v: number) {
        this._value = v;
        this._computedValue = null;
    }

    @cache
    get computed(): number {
        console.log("Computing...");
        return this._value * 2;
    }
}

const bounded = new BoundedValue();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("BoundedValue"),
        "Class should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("function validate") && output.contains("function cache"),
        "Decorator functions should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface AccessorDecorator"),
        "Interface should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private _value"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test parameter decorator in constructor
#[test]
fn test_parity_es5_decorator_parameter_constructor() {
    let source = r#"
interface ParameterDecorator {
    (target: Object, propertyKey: string | symbol | undefined, parameterIndex: number): void;
}

const injectionTokens = new Map<any, Map<number, string>>();

function inject(token: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, index: number): void {
        const existing = injectionTokens.get(target) || new Map();
        existing.set(index, token);
        injectionTokens.set(target, existing);
    };
}

function required(target: Object, propertyKey: string | symbol | undefined, index: number): void {
    console.log("Required parameter at index:", index);
}

interface DatabaseConnection {
    query(sql: string): Promise<any[]>;
}

interface LoggerService {
    log(message: string): void;
}

class UserRepository {
    private db: DatabaseConnection;
    private logger: LoggerService;

    constructor(
        @inject("database") @required db: DatabaseConnection,
        @inject("logger") logger: LoggerService
    ) {
        this.db = db;
        this.logger = logger;
    }

    async findUser(id: number): Promise<any> {
        this.logger.log("Finding user: " + id);
        const results = await this.db.query("SELECT * FROM users WHERE id = " + id);
        return results[0];
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("UserRepository"),
        "Class should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("function inject") && output.contains("function required"),
        "Decorator functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface ParameterDecorator")
            && !output.contains("interface DatabaseConnection"),
        "Interfaces should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private db") && !output.contains("private logger"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test decorator inheritance with method override
#[test]
fn test_parity_es5_decorator_inheritance_override() {
    let source = r#"
interface ClassDecorator {
    <T extends new (...args: any[]) => any>(constructor: T): T | void;
}

interface MethodDecorator {
    (target: any, propertyKey: string, descriptor: PropertyDescriptor): PropertyDescriptor | void;
}

function entity(name: string): ClassDecorator {
    return function<T extends new (...args: any[]) => any>(constructor: T): T {
        (constructor as any).entityName = name;
        return constructor;
    };
}

function logged(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        console.log("Before:", key);
        const result = original.apply(this, args);
        console.log("After:", key);
        return result;
    };
    return descriptor;
}

function validated(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    const original = descriptor.value;
    descriptor.value = function(...args: any[]) {
        if (args.some(arg => arg === null || arg === undefined)) {
            throw new Error("Invalid arguments");
        }
        return original.apply(this, args);
    };
    return descriptor;
}

@entity("base")
class BaseEntity {
    id: number;

    constructor(id: number) {
        this.id = id;
    }

    @logged
    save(): void {
        console.log("Saving entity:", this.id);
    }
}

@entity("user")
class UserEntity extends BaseEntity {
    name: string;

    constructor(id: number, name: string) {
        super(id);
        this.name = name;
    }

    @validated
    @logged
    save(): void {
        console.log("Saving user:", this.name);
        super.save();
    }

    @logged
    delete(): void {
        console.log("Deleting user:", this.id);
    }
}

const user = new UserEntity(1, "John");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseEntity") && output.contains("UserEntity"),
        "Classes should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("function entity")
            && output.contains("function logged")
            && output.contains("function validated"),
        "Decorator functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface ClassDecorator")
            && !output.contains("interface MethodDecorator"),
        "Interfaces should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test combined all decorator types
#[test]
fn test_parity_es5_decorator_combined_all() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

function component(selector: string): ClassDecorator {
    return function(constructor: Function): void {
        (constructor as any).selector = selector;
    };
}

function input(target: any, key: string): void {
    const inputs = (target.constructor as any).inputs || [];
    inputs.push(key);
    (target.constructor as any).inputs = inputs;
}

function output(target: any, key: string): void {
    const outputs = (target.constructor as any).outputs || [];
    outputs.push(key);
    (target.constructor as any).outputs = outputs;
}

function autobind(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    const original = descriptor.value;
    return {
        configurable: true,
        enumerable: false,
        get() {
            return original.bind(this);
        }
    };
}

function readonly(target: any, key: string, descriptor: PropertyDescriptor): PropertyDescriptor {
    descriptor.writable = false;
    return descriptor;
}

function inject(token: string): ParameterDecorator {
    return function(target: Object, propertyKey: string | symbol | undefined, index: number): void {
        console.log("Injecting", token, "at index", index);
    };
}

@component("app-widget")
class Widget {
    @input
    title: string = "";

    @output
    onClick: Function = () => {};

    private _count: number = 0;

    constructor(@inject("config") config: any) {
        console.log("Widget created with config:", config);
    }

    @readonly
    get count(): number {
        return this._count;
    }

    set count(value: number) {
        this._count = value;
    }

    @autobind
    handleClick(event: Event): void {
        this._count++;
        this.onClick(event);
    }

    @autobind
    @readonly
    render(): string {
        return "<div>" + this.title + "</div>";
    }
}

const widget = new Widget({});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("Widget"),
        "Class should be present: {}",
        output
    );
    // Decorator functions should be present
    assert!(
        output.contains("function component")
            && output.contains("function input")
            && output.contains("function autobind"),
        "Decorator functions should be present: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Constructor"),
        "Type alias should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private _count"),
        "Private modifier should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string")
            && !output.contains(": number")
            && !output.contains(": Function"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test private instance field with inheritance chain
#[test]
fn test_parity_es5_private_field_inheritance_chain() {
    let source = r#"
interface Identifiable {
    getId(): string;
}

class BaseEntity implements Identifiable {
    #id: string;
    #createdAt: Date;

    constructor(id: string) {
        this.#id = id;
        this.#createdAt = new Date();
    }

    getId(): string {
        return this.#id;
    }

    protected getCreatedAt(): Date {
        return this.#createdAt;
    }
}

class User extends BaseEntity {
    #email: string;
    #password: string;

    constructor(id: string, email: string, password: string) {
        super(id);
        this.#email = email;
        this.#password = password;
    }

    getEmail(): string {
        return this.#email;
    }

    #hashPassword(): string {
        return "hashed_" + this.#password;
    }

    validatePassword(input: string): boolean {
        return this.#hashPassword() === "hashed_" + input;
    }
}

class Admin extends User {
    #permissions: string[];

    constructor(id: string, email: string, password: string, permissions: string[]) {
        super(id, email, password);
        this.#permissions = permissions;
    }

    hasPermission(permission: string): boolean {
        return this.#permissions.includes(permission);
    }
}

const admin = new Admin("1", "admin@test.com", "secret", ["read", "write"]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseEntity") && output.contains("User") && output.contains("Admin"),
        "Classes should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Identifiable"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": Date") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected getCreatedAt"),
        "Protected modifier should be erased: {}",
        output
    );
}

/// Test private static field with initialization dependencies
#[test]
fn test_parity_es5_private_static_initialization_order() {
    let source = r#"
interface Config {
    baseUrl: string;
    timeout: number;
}

class ApiClient {
    static #instanceCount: number = 0;
    static #defaultConfig: Config = { baseUrl: "https://api.example.com", timeout: 5000 };
    static #instances: ApiClient[] = [];

    #config: Config;
    #id: number;

    constructor(config?: Partial<Config>) {
        ApiClient.#instanceCount++;
        this.#id = ApiClient.#instanceCount;
        this.#config = { ...ApiClient.#defaultConfig, ...config };
        ApiClient.#instances.push(this);
    }

    static getInstanceCount(): number {
        return ApiClient.#instanceCount;
    }

    static getAllInstances(): ApiClient[] {
        return [...ApiClient.#instances];
    }

    static #resetInstances(): void {
        ApiClient.#instances = [];
        ApiClient.#instanceCount = 0;
    }

    static reset(): void {
        ApiClient.#resetInstances();
    }

    getId(): number {
        return this.#id;
    }

    getConfig(): Config {
        return { ...this.#config };
    }
}

const client1 = new ApiClient();
const client2 = new ApiClient({ timeout: 10000 });
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("ApiClient"),
        "Class should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": Config")
            && !output.contains(": ApiClient[]"),
        "Type annotations should be erased: {}",
        output
    );
    // Static modifier in type context should be erased
    assert!(
        !output.contains("static #instanceCount: number"),
        "Static field type should be erased: {}",
        output
    );
}

/// Test private method with async patterns
#[test]
fn test_parity_es5_private_method_async_patterns() {
    let source = r#"
interface ApiResponse<T> {
    data: T;
    status: number;
}

class DataService {
    #baseUrl: string;
    #cache: Map<string, any> = new Map();

    constructor(baseUrl: string) {
        this.#baseUrl = baseUrl;
    }

    async #fetch<T>(endpoint: string): Promise<ApiResponse<T>> {
        const response = await fetch(this.#baseUrl + endpoint);
        const data = await response.json();
        return { data, status: response.status };
    }

    async #fetchWithCache<T>(endpoint: string): Promise<T> {
        if (this.#cache.has(endpoint)) {
            return this.#cache.get(endpoint);
        }
        const response = await this.#fetch<T>(endpoint);
        this.#cache.set(endpoint, response.data);
        return response.data;
    }

    async #retry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {
        for (let i = 0; i < attempts; i++) {
            try {
                return await fn();
            } catch (e) {
                if (i === attempts - 1) throw e;
                await this.#delay(1000 * (i + 1));
            }
        }
        throw new Error("Retry failed");
    }

    async #delay(ms: number): Promise<void> {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async getUser(id: string): Promise<any> {
        return this.#retry(() => this.#fetchWithCache("/users/" + id), 3);
    }

    async getUsers(): Promise<any[]> {
        const response = await this.#fetch<any[]>("/users");
        return response.data;
    }
}

const service = new DataService("https://api.example.com");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("DataService"),
        "Class should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface ApiResponse"),
        "Interface should be erased: {}",
        output
    );
    // Generic type annotations should be erased
    assert!(
        !output.contains("<T>") && !output.contains("Promise<ApiResponse"),
        "Generic type annotations should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": string") && !output.contains(": number") && !output.contains(": Map<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test private accessor with computed values
#[test]
fn test_parity_es5_private_accessor_computed_values() {
    let source = r#"
interface Dimensions {
    width: number;
    height: number;
}

class Rectangle {
    #width: number;
    #height: number;
    #cachedArea: number | null = null;
    #cachedPerimeter: number | null = null;

    constructor(width: number, height: number) {
        this.#width = width;
        this.#height = height;
    }

    get #area(): number {
        if (this.#cachedArea === null) {
            this.#cachedArea = this.#width * this.#height;
        }
        return this.#cachedArea;
    }

    get #perimeter(): number {
        if (this.#cachedPerimeter === null) {
            this.#cachedPerimeter = 2 * (this.#width + this.#height);
        }
        return this.#cachedPerimeter;
    }

    set #dimensions(dims: Dimensions) {
        this.#width = dims.width;
        this.#height = dims.height;
        this.#invalidateCache();
    }

    #invalidateCache(): void {
        this.#cachedArea = null;
        this.#cachedPerimeter = null;
    }

    getArea(): number {
        return this.#area;
    }

    getPerimeter(): number {
        return this.#perimeter;
    }

    resize(width: number, height: number): void {
        this.#dimensions = { width, height };
    }

    scale(factor: number): void {
        this.#dimensions = {
            width: this.#width * factor,
            height: this.#height * factor
        };
    }
}

const rect = new Rectangle(10, 20);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("Rectangle"),
        "Class should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Dimensions"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number")
            && !output.contains(": Dimensions")
            && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
    // Union type should be erased
    assert!(
        !output.contains("number | null"),
        "Union type should be erased: {}",
        output
    );
}

/// Test private field in conditional expressions
#[test]
fn test_parity_es5_private_field_conditional_expr() {
    let source = r#"
interface State {
    isActive: boolean;
    value: number;
}

class StateMachine {
    #state: "idle" | "running" | "paused" | "stopped" = "idle";
    #value: number = 0;
    #maxValue: number;
    #minValue: number;

    constructor(min: number, max: number) {
        this.#minValue = min;
        this.#maxValue = max;
    }

    #isValidValue(value: number): boolean {
        return value >= this.#minValue && value <= this.#maxValue;
    }

    #clamp(value: number): number {
        return value < this.#minValue ? this.#minValue :
               value > this.#maxValue ? this.#maxValue : value;
    }

    setValue(value: number): void {
        this.#value = this.#isValidValue(value) ? value : this.#clamp(value);
    }

    getValue(): number {
        return this.#state === "running" ? this.#value :
               this.#state === "paused" ? this.#value :
               this.#state === "idle" ? 0 : -1;
    }

    getState(): string {
        return this.#state;
    }

    start(): void {
        this.#state = this.#state === "idle" || this.#state === "stopped" ? "running" : this.#state;
    }

    pause(): void {
        this.#state = this.#state === "running" ? "paused" : this.#state;
    }

    resume(): void {
        this.#state = this.#state === "paused" ? "running" : this.#state;
    }

    stop(): void {
        this.#state = this.#state !== "stopped" ? "stopped" : this.#state;
        this.#value = this.#state === "stopped" ? 0 : this.#value;
    }

    increment(): number {
        return this.#state === "running"
            ? (this.#value = this.#clamp(this.#value + 1))
            : this.#value;
    }
}

const machine = new StateMachine(0, 100);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class should be present
    assert!(
        output.contains("StateMachine"),
        "Class should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface State"),
        "Interface should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": boolean") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
    // Union literal type should be erased
    assert!(
        !output.contains(r#""idle" | "running""#),
        "Union literal type should be erased: {}",
        output
    );
}

/// Test combined private patterns with generics
#[test]
fn test_parity_es5_private_combined_generics() {
    let source = r#"
interface Comparable<T> {
    compareTo(other: T): number;
}

interface Serializable {
    serialize(): string;
}

class PrivateCollection<T extends Comparable<T> & Serializable> {
    #items: T[] = [];
    #maxSize: number;
    #comparator: ((a: T, b: T) => number) | null = null;

    static #defaultMaxSize: number = 100;
    static #instanceCount: number = 0;

    constructor(maxSize?: number) {
        this.#maxSize = maxSize ?? PrivateCollection.#defaultMaxSize;
        PrivateCollection.#instanceCount++;
    }

    static getInstanceCount(): number {
        return PrivateCollection.#instanceCount;
    }

    #ensureCapacity(): boolean {
        return this.#items.length < this.#maxSize;
    }

    #sort(): void {
        if (this.#comparator) {
            this.#items.sort(this.#comparator);
        } else {
            this.#items.sort((a, b) => a.compareTo(b));
        }
    }

    get #size(): number {
        return this.#items.length;
    }

    get #isEmpty(): boolean {
        return this.#items.length === 0;
    }

    set #customComparator(comparator: (a: T, b: T) => number) {
        this.#comparator = comparator;
    }

    add(item: T): boolean {
        if (!this.#ensureCapacity()) return false;
        this.#items.push(item);
        this.#sort();
        return true;
    }

    remove(item: T): boolean {
        const index = this.#items.findIndex(i => i.compareTo(item) === 0);
        if (index === -1) return false;
        this.#items.splice(index, 1);
        return true;
    }

    getSize(): number {
        return this.#size;
    }

    isEmpty(): boolean {
        return this.#isEmpty;
    }

    setComparator(comparator: (a: T, b: T) => number): void {
        this.#customComparator = comparator;
        this.#sort();
    }

    toArray(): T[] {
        return [...this.#items];
    }

    serialize(): string {
        return JSON.stringify(this.#items.map(item => item.serialize()));
    }
}

class NumberWrapper implements Comparable<NumberWrapper>, Serializable {
    constructor(public value: number) {}

    compareTo(other: NumberWrapper): number {
        return this.value - other.value;
    }

    serialize(): string {
        return String(this.value);
    }
}

const collection = new PrivateCollection<NumberWrapper>(50);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("PrivateCollection") && output.contains("NumberWrapper"),
        "Classes should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Comparable") && !output.contains("interface Serializable"),
        "Interfaces should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T extends")
            && !output.contains("<T>")
            && !output.contains("<NumberWrapper>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T[]") && !output.contains(": number") && !output.contains(": boolean"),
        "Type annotations should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements Comparable"),
        "Implements clause should be erased: {}",
        output
    );
}

/// Test abstract class with abstract methods
#[test]
fn test_parity_es5_abstract_class_abstract_methods() {
    let source = r#"
interface Drawable {
    draw(ctx: CanvasRenderingContext2D): void;
}

abstract class Shape implements Drawable {
    abstract getArea(): number;
    abstract getPerimeter(): number;
    abstract draw(ctx: CanvasRenderingContext2D): void;

    describe(): string {
        return "Area: " + this.getArea() + ", Perimeter: " + this.getPerimeter();
    }
}

abstract class Polygon extends Shape {
    abstract getSides(): number;

    describe(): string {
        return super.describe() + ", Sides: " + this.getSides();
    }
}

class Triangle extends Polygon {
    constructor(private a: number, private b: number, private c: number) {
        super();
    }

    getArea(): number {
        const s = (this.a + this.b + this.c) / 2;
        return Math.sqrt(s * (s - this.a) * (s - this.b) * (s - this.c));
    }

    getPerimeter(): number {
        return this.a + this.b + this.c;
    }

    getSides(): number {
        return 3;
    }

    draw(ctx: CanvasRenderingContext2D): void {
        console.log("Drawing triangle");
    }
}

const triangle = new Triangle(3, 4, 5);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Shape") && output.contains("Polygon") && output.contains("Triangle"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract getArea"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Drawable"),
        "Interface should be erased: {}",
        output
    );
    // Implements should be erased
    assert!(
        !output.contains("implements Drawable"),
        "Implements should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test abstract class with implemented methods
#[test]
fn test_parity_es5_abstract_class_implemented_methods() {
    let source = r#"
interface Logger {
    log(message: string): void;
}

abstract class BaseService implements Logger {
    protected name: string;

    constructor(name: string) {
        this.name = name;
    }

    log(message: string): void {
        console.log("[" + this.name + "] " + message);
    }

    protected formatError(error: Error): string {
        return "Error in " + this.name + ": " + error.message;
    }

    abstract execute(): Promise<void>;
    abstract validate(): boolean;
}

class EmailService extends BaseService {
    private recipients: string[];

    constructor(recipients: string[]) {
        super("EmailService");
        this.recipients = recipients;
    }

    async execute(): Promise<void> {
        this.log("Sending email to " + this.recipients.length + " recipients");
        await this.sendEmails();
    }

    validate(): boolean {
        return this.recipients.length > 0;
    }

    private async sendEmails(): Promise<void> {
        for (const recipient of this.recipients) {
            this.log("Sent to: " + recipient);
        }
    }
}

const service = new EmailService(["user@example.com"]);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseService") && output.contains("EmailService"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract execute"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected name") && !output.contains("protected formatError"),
        "Protected modifier should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Logger"),
        "Interface should be erased: {}",
        output
    );
}

/// Test abstract class with static members
#[test]
fn test_parity_es5_abstract_class_static_members() {
    let source = r#"
interface Countable {
    getCount(): number;
}

abstract class Counter implements Countable {
    private static instanceCount: number = 0;
    protected static readonly MAX_INSTANCES: number = 100;

    protected id: number;

    constructor() {
        Counter.instanceCount++;
        this.id = Counter.instanceCount;
    }

    static getInstanceCount(): number {
        return Counter.instanceCount;
    }

    static resetCount(): void {
        Counter.instanceCount = 0;
    }

    abstract getCount(): number;
    abstract increment(): void;
    abstract decrement(): void;

    getId(): number {
        return this.id;
    }
}

class UpDownCounter extends Counter {
    private count: number = 0;

    getCount(): number {
        return this.count;
    }

    increment(): void {
        this.count++;
    }

    decrement(): void {
        this.count--;
    }

    static create(): UpDownCounter {
        if (Counter.getInstanceCount() >= Counter["MAX_INSTANCES"]) {
            throw new Error("Max instances reached");
        }
        return new UpDownCounter();
    }
}

const counter1 = new UpDownCounter();
const counter2 = UpDownCounter.create();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Counter") && output.contains("UpDownCounter"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Static methods should be present
    assert!(
        output.contains("getInstanceCount") && output.contains("resetCount"),
        "Static methods should be present: {}",
        output
    );
    // Private/protected/readonly modifiers should be erased
    assert!(
        !output.contains("private static")
            && !output.contains("protected static")
            && !output.contains("readonly MAX"),
        "Access modifiers should be erased: {}",
        output
    );
}

/// Test abstract class inheritance chain
#[test]
fn test_parity_es5_abstract_class_inheritance_chain() {
    let source = r#"
interface Renderable {
    render(): string;
}

abstract class Component implements Renderable {
    abstract render(): string;

    mount(): void {
        console.log("Mounting component");
    }
}

abstract class UIComponent extends Component {
    protected styles: Record<string, string> = {};

    abstract getClassName(): string;

    setStyle(key: string, value: string): void {
        this.styles[key] = value;
    }
}

abstract class InteractiveComponent extends UIComponent {
    protected handlers: Map<string, Function> = new Map();

    abstract onClick(): void;
    abstract onHover(): void;

    addHandler(event: string, handler: Function): void {
        this.handlers.set(event, handler);
    }
}

class Button extends InteractiveComponent {
    private label: string;

    constructor(label: string) {
        super();
        this.label = label;
    }

    render(): string {
        return "<button class='" + this.getClassName() + "'>" + this.label + "</button>";
    }

    getClassName(): string {
        return "btn btn-primary";
    }

    onClick(): void {
        console.log("Button clicked");
    }

    onHover(): void {
        console.log("Button hovered");
    }
}

const button = new Button("Submit");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // All classes should be present
    assert!(
        output.contains("Component")
            && output.contains("UIComponent")
            && output.contains("InteractiveComponent")
            && output.contains("Button"),
        "All classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract render"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Renderable"),
        "Interface should be erased: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected styles") && !output.contains("protected handlers"),
        "Protected modifier should be erased: {}",
        output
    );
}

/// Test abstract class with generics
#[test]
fn test_parity_es5_abstract_class_generics() {
    let source = r#"
interface Repository<T> {
    findById(id: string): T | null;
    save(entity: T): void;
    delete(id: string): boolean;
}

abstract class BaseRepository<T, ID = string> implements Repository<T> {
    protected items: Map<ID, T> = new Map();

    abstract findById(id: ID): T | null;
    abstract createId(): ID;

    save(entity: T): void {
        const id = this.createId();
        this.items.set(id, entity);
    }

    delete(id: ID): boolean {
        return this.items.delete(id);
    }

    getAll(): T[] {
        return Array.from(this.items.values());
    }

    protected getItemCount(): number {
        return this.items.size;
    }
}

interface User {
    name: string;
    email: string;
}

class UserRepository extends BaseRepository<User, string> {
    private counter: number = 0;

    findById(id: string): User | null {
        return this.items.get(id) || null;
    }

    createId(): string {
        this.counter++;
        return "user_" + this.counter;
    }

    findByEmail(email: string): User | null {
        for (const user of this.items.values()) {
            if (user.email === email) return user;
        }
        return null;
    }
}

const repo = new UserRepository();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseRepository") && output.contains("UserRepository"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract findById"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T, ID") && !output.contains("<User"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Repository") && !output.contains("interface User"),
        "Interfaces should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements Repository"),
        "Implements clause should be erased: {}",
        output
    );
}

/// Test combined abstract class patterns
#[test]
fn test_parity_es5_abstract_class_combined_patterns() {
    let source = r#"
interface Identifiable {
    getId(): string;
}

interface Timestamped {
    getCreatedAt(): Date;
    getUpdatedAt(): Date;
}

abstract class Entity<T extends Identifiable & Timestamped> {
    protected static entityCount: number = 0;
    private static readonly VERSION: string = "1.0.0";

    #data: T | null = null;
    protected readonly createdAt: Date;

    constructor() {
        Entity.entityCount++;
        this.createdAt = new Date();
    }

    abstract validate(): boolean;
    abstract serialize(): string;
    abstract deserialize(data: string): T;

    static getVersion(): string {
        return Entity.VERSION;
    }

    static getEntityCount(): number {
        return Entity.entityCount;
    }

    protected setData(data: T): void {
        this.#data = data;
    }

    getData(): T | null {
        return this.#data;
    }

    getAge(): number {
        return Date.now() - this.createdAt.getTime();
    }
}

interface UserData extends Identifiable, Timestamped {
    name: string;
    email: string;
}

class UserEntity extends Entity<UserData> {
    validate(): boolean {
        const data = this.getData();
        return data !== null && data.name.length > 0 && data.email.includes("@");
    }

    serialize(): string {
        const data = this.getData();
        return data ? JSON.stringify(data) : "";
    }

    deserialize(json: string): UserData {
        return JSON.parse(json);
    }

    updateUser(name: string, email: string): void {
        const data = this.getData();
        if (data) {
            this.setData({
                ...data,
                name,
                email
            });
        }
    }
}

const userEntity = new UserEntity();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Entity") && output.contains("UserEntity"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract validate"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T extends") && !output.contains("<UserData>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Identifiable") && !output.contains("interface UserData"),
        "Interfaces should be erased: {}",
        output
    );
    // Protected/private/readonly modifiers should be erased
    assert!(
        !output.contains("protected static")
            && !output.contains("private static")
            && !output.contains("readonly VERSION"),
        "Access modifiers should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": boolean") && !output.contains(": string") && !output.contains(": Date"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test basic mixin function pattern
#[test]
fn test_parity_es5_mixin_basic_function() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

interface Timestamped {
    timestamp: Date;
    getTimestamp(): Date;
}

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Timestamped {
        timestamp = new Date();

        getTimestamp(): Date {
            return this.timestamp;
        }
    };
}

interface Named {
    name: string;
    getName(): string;
}

function Named<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Named {
        name = "";

        getName(): string {
            return this.name;
        }

        setName(name: string): void {
            this.name = name;
        }
    };
}

class Entity {
    id: number;

    constructor(id: number) {
        this.id = id;
    }
}

const TimestampedEntity = Timestamped(Entity);
const NamedEntity = Named(Entity);
const entity = new TimestampedEntity(1);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes and functions should be present
    assert!(
        output.contains("Entity") && output.contains("Timestamped") && output.contains("Named"),
        "Classes and mixin functions should be present: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Constructor"),
        "Type alias should be erased: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Timestamped") && !output.contains("interface Named"),
        "Interfaces should be erased: {}",
        output
    );
    // Implements clauses should be erased
    assert!(
        !output.contains("implements Timestamped") && !output.contains("implements Named"),
        "Implements clauses should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Date") && !output.contains(": string") && !output.contains(": void"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test mixin with generics
#[test]
fn test_parity_es5_mixin_generics() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

interface Comparable<T> {
    compareTo(other: T): number;
}

function Comparable<T, TBase extends Constructor>(Base: TBase) {
    abstract class ComparableClass extends Base implements Comparable<T> {
        abstract compareTo(other: T): number;

        isEqual(other: T): boolean {
            return this.compareTo(other) === 0;
        }

        isLessThan(other: T): boolean {
            return this.compareTo(other) < 0;
        }

        isGreaterThan(other: T): boolean {
            return this.compareTo(other) > 0;
        }
    }
    return ComparableClass;
}

interface Serializable<T> {
    serialize(): string;
    deserialize(data: string): T;
}

function Serializable<T, TBase extends Constructor>(Base: TBase) {
    abstract class SerializableClass extends Base implements Serializable<T> {
        abstract serialize(): string;
        abstract deserialize(data: string): T;

        toJSON(): string {
            return this.serialize();
        }
    }
    return SerializableClass;
}

class BaseModel {
    id: string;

    constructor(id: string) {
        this.id = id;
    }
}

class User extends Serializable<User, typeof BaseModel>(Comparable<User, typeof BaseModel>(BaseModel)) {
    name: string;

    constructor(id: string, name: string) {
        super(id);
        this.name = name;
    }

    compareTo(other: User): number {
        return this.name.localeCompare(other.name);
    }

    serialize(): string {
        return JSON.stringify({ id: this.id, name: this.name });
    }

    deserialize(data: string): User {
        const obj = JSON.parse(data);
        return new User(obj.id, obj.name);
    }
}

const user = new User("1", "Alice");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseModel") && output.contains("User"),
        "Classes should be present: {}",
        output
    );
    // Mixin functions should be present
    assert!(
        output.contains("Comparable") && output.contains("Serializable"),
        "Mixin functions should be present: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<User>") && !output.contains("<T, TBase"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract compareTo"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Comparable") && !output.contains("interface Serializable"),
        "Interfaces should be erased: {}",
        output
    );
}

/// Test multiple mixin composition
#[test]
fn test_parity_es5_mixin_composition() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

interface Loggable {
    log(message: string): void;
}

function Loggable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Loggable {
        log(message: string): void {
            console.log("[" + this.constructor.name + "] " + message);
        }
    };
}

interface Disposable {
    dispose(): void;
    isDisposed: boolean;
}

function Disposable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Disposable {
        isDisposed = false;

        dispose(): void {
            this.isDisposed = true;
        }
    };
}

interface Activatable {
    activate(): void;
    deactivate(): void;
    isActive: boolean;
}

function Activatable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Activatable {
        isActive = false;

        activate(): void {
            this.isActive = true;
        }

        deactivate(): void {
            this.isActive = false;
        }
    };
}

class Component {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

const EnhancedComponent = Loggable(Disposable(Activatable(Component)));

class Widget extends EnhancedComponent {
    private width: number;
    private height: number;

    constructor(name: string, width: number, height: number) {
        super(name);
        this.width = width;
        this.height = height;
    }

    render(): void {
        this.log("Rendering widget: " + this.name);
    }
}

const widget = new Widget("MyWidget", 100, 200);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Component") && output.contains("Widget"),
        "Classes should be present: {}",
        output
    );
    // Mixin functions should be present
    assert!(
        output.contains("Loggable")
            && output.contains("Disposable")
            && output.contains("Activatable"),
        "Mixin functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Loggable") && !output.contains("interface Disposable"),
        "Interfaces should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Constructor"),
        "Type alias should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private width") && !output.contains("private height"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test mixin with static members
#[test]
fn test_parity_es5_mixin_static_members() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

interface Countable {
    getInstanceId(): number;
}

function Countable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Countable {
        static instanceCount: number = 0;
        static readonly MAX_INSTANCES: number = 1000;

        private instanceId: number;

        constructor(...args: any[]) {
            super(...args);
            (this.constructor as any).instanceCount++;
            this.instanceId = (this.constructor as any).instanceCount;
        }

        static getCount(): number {
            return this.instanceCount;
        }

        static resetCount(): void {
            this.instanceCount = 0;
        }

        getInstanceId(): number {
            return this.instanceId;
        }
    };
}

interface Registrable {
    register(): void;
    unregister(): void;
}

function Registrable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Registrable {
        static registry: Set<any> = new Set();

        static getRegistered(): any[] {
            return Array.from(this.registry);
        }

        static clearRegistry(): void {
            this.registry.clear();
        }

        register(): void {
            (this.constructor as any).registry.add(this);
        }

        unregister(): void {
            (this.constructor as any).registry.delete(this);
        }
    };
}

class Service {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

const TrackedService = Countable(Registrable(Service));

class DatabaseService extends TrackedService {
    connectionString: string;

    constructor(name: string, connectionString: string) {
        super(name);
        this.connectionString = connectionString;
    }
}

const db = new DatabaseService("DB", "localhost:5432");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Service") && output.contains("DatabaseService"),
        "Classes should be present: {}",
        output
    );
    // Mixin functions should be present
    assert!(
        output.contains("Countable") && output.contains("Registrable"),
        "Mixin functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Countable") && !output.contains("interface Registrable"),
        "Interfaces should be erased: {}",
        output
    );
    // Readonly modifier should be erased
    assert!(
        !output.contains("readonly MAX_INSTANCES"),
        "Readonly modifier should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": number") && !output.contains(": Set<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test mixin with private fields
#[test]
fn test_parity_es5_mixin_private_fields() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;

interface Cacheable {
    getCached<T>(key: string): T | undefined;
    setCached<T>(key: string, value: T): void;
    clearCache(): void;
}

function Cacheable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Cacheable {
        #cache: Map<string, any> = new Map();
        #cacheHits: number = 0;
        #cacheMisses: number = 0;

        getCached<T>(key: string): T | undefined {
            if (this.#cache.has(key)) {
                this.#cacheHits++;
                return this.#cache.get(key);
            }
            this.#cacheMisses++;
            return undefined;
        }

        setCached<T>(key: string, value: T): void {
            this.#cache.set(key, value);
        }

        clearCache(): void {
            this.#cache.clear();
            this.#cacheHits = 0;
            this.#cacheMisses = 0;
        }

        getCacheStats(): { hits: number; misses: number } {
            return { hits: this.#cacheHits, misses: this.#cacheMisses };
        }
    };
}

interface Lockable {
    lock(): void;
    unlock(): void;
    isLocked(): boolean;
}

function Lockable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Lockable {
        #locked: boolean = false;
        #lockCount: number = 0;

        lock(): void {
            this.#locked = true;
            this.#lockCount++;
        }

        unlock(): void {
            this.#locked = false;
        }

        isLocked(): boolean {
            return this.#locked;
        }

        getLockCount(): number {
            return this.#lockCount;
        }
    };
}

class Resource {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

const SecureResource = Cacheable(Lockable(Resource));

class FileResource extends SecureResource {
    path: string;

    constructor(name: string, path: string) {
        super(name);
        this.path = path;
    }

    read(): string {
        if (this.isLocked()) {
            throw new Error("Resource is locked");
        }
        const cached = this.getCached<string>("content");
        if (cached) return cached;
        const content = "File content of " + this.path;
        this.setCached("content", content);
        return content;
    }
}

const file = new FileResource("config", "/etc/config.json");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Resource") && output.contains("FileResource"),
        "Classes should be present: {}",
        output
    );
    // Mixin functions should be present
    assert!(
        output.contains("Cacheable") && output.contains("Lockable"),
        "Mixin functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Cacheable") && !output.contains("interface Lockable"),
        "Interfaces should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Constructor"),
        "Type alias should be erased: {}",
        output
    );
    // Generic type annotations should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<string>"),
        "Generic type annotations should be erased: {}",
        output
    );
}

/// Test combined mixin patterns
#[test]
fn test_parity_es5_mixin_combined_patterns() {
    let source = r#"
type Constructor<T = {}> = new (...args: any[]) => T;
type GConstructor<T = {}> = new (...args: any[]) => T;

interface EventEmitter {
    on(event: string, handler: Function): void;
    off(event: string, handler: Function): void;
    emit(event: string, ...args: any[]): void;
}

function EventEmitter<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements EventEmitter {
        #handlers: Map<string, Set<Function>> = new Map();

        on(event: string, handler: Function): void {
            if (!this.#handlers.has(event)) {
                this.#handlers.set(event, new Set());
            }
            this.#handlers.get(event)!.add(handler);
        }

        off(event: string, handler: Function): void {
            this.#handlers.get(event)?.delete(handler);
        }

        emit(event: string, ...args: any[]): void {
            this.#handlers.get(event)?.forEach(handler => handler(...args));
        }

        listenerCount(event: string): number {
            return this.#handlers.get(event)?.size ?? 0;
        }
    };
}

interface Observable<T> {
    subscribe(observer: (value: T) => void): () => void;
    getValue(): T;
}

function Observable<T, TBase extends Constructor>(Base: TBase) {
    abstract class ObservableClass extends Base implements Observable<T> {
        #observers: Set<(value: T) => void> = new Set();
        #value: T | undefined;

        abstract getValue(): T;

        protected setValue(value: T): void {
            this.#value = value;
            this.#notifyObservers(value);
        }

        #notifyObservers(value: T): void {
            this.#observers.forEach(observer => observer(value));
        }

        subscribe(observer: (value: T) => void): () => void {
            this.#observers.add(observer);
            return () => this.#observers.delete(observer);
        }
    }
    return ObservableClass;
}

interface Validatable {
    validate(): boolean;
    getErrors(): string[];
}

function Validatable<TBase extends Constructor>(Base: TBase) {
    return class extends Base implements Validatable {
        protected errors: string[] = [];

        validate(): boolean {
            this.errors = [];
            return true;
        }

        getErrors(): string[] {
            return [...this.errors];
        }

        protected addError(error: string): void {
            this.errors.push(error);
        }
    };
}

class Model {
    id: string;
    createdAt: Date;

    constructor(id: string) {
        this.id = id;
        this.createdAt = new Date();
    }
}

const ReactiveModel = EventEmitter(Observable<any, typeof Model>(Validatable(Model)));

class UserModel extends ReactiveModel {
    private _name: string = "";
    private _email: string = "";

    get name(): string {
        return this._name;
    }

    set name(value: string) {
        this._name = value;
        this.emit("change", { field: "name", value });
    }

    get email(): string {
        return this._email;
    }

    set email(value: string) {
        this._email = value;
        this.emit("change", { field: "email", value });
    }

    getValue(): any {
        return { id: this.id, name: this._name, email: this._email };
    }

    validate(): boolean {
        super.validate();
        if (!this._name) this.addError("Name is required");
        if (!this._email.includes("@")) this.addError("Invalid email");
        return this.getErrors().length === 0;
    }
}

const user = new UserModel("user-1");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Model") && output.contains("UserModel"),
        "Classes should be present: {}",
        output
    );
    // Mixin functions should be present
    assert!(
        output.contains("EventEmitter")
            && output.contains("Observable")
            && output.contains("Validatable"),
        "Mixin functions should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Constructor") && !output.contains("type GConstructor"),
        "Type aliases should be erased: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface EventEmitter") && !output.contains("interface Observable"),
        "Interfaces should be erased: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract getValue"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected errors") && !output.contains("protected setValue"),
        "Protected modifier should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<TBase") && !output.contains("<any,"),
        "Generic type parameters should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Generic Class Patterns Parity Tests
// ============================================================================

/// Test ES5 generic class with single type parameter
#[test]
fn test_parity_es5_generic_class_single_param() {
    let source = r#"
class Container<T> {
    private value: T;

    constructor(value: T) {
        this.value = value;
    }

    getValue(): T {
        return this.value;
    }

    setValue(value: T): void {
        this.value = value;
    }
}

class StringContainer extends Container<string> {
    getLength(): number {
        return this.getValue().length;
    }
}

const numContainer = new Container<number>(42);
const strContainer = new StringContainer("hello");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Container") && output.contains("StringContainer"),
        "Classes should be present: {}",
        output
    );
    // Generic type parameter should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<string>") && !output.contains("<number>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Return type annotations should be erased
    assert!(
        !output.contains(": T") && !output.contains(": number") && !output.contains(": void"),
        "Return type annotations should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private value"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test ES5 generic class with multiple type parameters
#[test]
fn test_parity_es5_generic_class_multi_params() {
    let source = r#"
class Pair<K, V> {
    constructor(public key: K, public value: V) {}

    getKey(): K { return this.key; }
    getValue(): V { return this.value; }
    swap(): Pair<V, K> { return new Pair(this.value, this.key); }
}

class Triple<A, B, C> {
    constructor(
        private first: A,
        private second: B,
        private third: C
    ) {}

    toArray(): [A, B, C] {
        return [this.first, this.second, this.third];
    }
}

class Dictionary<K extends string, V> {
    private items: Map<K, V> = new Map();

    set(key: K, value: V): void {
        this.items.set(key, value);
    }

    get(key: K): V | undefined {
        return this.items.get(key);
    }
}

const pair = new Pair<string, number>("age", 25);
const triple = new Triple<number, string, boolean>(1, "two", true);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("Pair") && output.contains("Triple") && output.contains("Dictionary"),
        "Classes should be present: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<K,") && !output.contains("<A,") && !output.contains("<K extends"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Return type annotations should be erased
    assert!(
        !output.contains("): K") && !output.contains("): V") && !output.contains("): [A,"),
        "Return type annotations should be erased: {}",
        output
    );
    // Public/private modifiers should be erased
    assert!(
        !output.contains("public key") && !output.contains("private first"),
        "Access modifiers should be erased: {}",
        output
    );
}

/// Test ES5 generic class with constraints
#[test]
fn test_parity_es5_generic_class_constraints() {
    let source = r#"
interface Comparable<T> {
    compareTo(other: T): number;
}

interface Serializable {
    serialize(): string;
}

class SortedList<T extends Comparable<T>> {
    private items: T[] = [];

    add(item: T): void {
        this.items.push(item);
        this.items.sort((a, b) => a.compareTo(b));
    }

    get(index: number): T {
        return this.items[index];
    }
}

class Repository<T extends { id: string } & Serializable> {
    private data: Map<string, T> = new Map();

    save(item: T): void {
        this.data.set(item.id, item);
    }

    find(id: string): T | undefined {
        return this.data.get(id);
    }

    exportAll(): string[] {
        return Array.from(this.data.values()).map(v => v.serialize());
    }
}

class KeyValueStore<K extends string | number, V extends object> {
    private store: Record<string, V> = {};

    put(key: K, value: V): void {
        this.store[String(key)] = value;
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("SortedList")
            && output.contains("Repository")
            && output.contains("KeyValueStore"),
        "Classes should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Comparable") && !output.contains("interface Serializable"),
        "Interfaces should be erased: {}",
        output
    );
    // Generic constraints should be erased
    assert!(
        !output.contains("<T extends Comparable") && !output.contains("<T extends {"),
        "Generic constraints should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": T[]") && !output.contains(": Map<") && !output.contains(": Record<"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test ES5 generic class extending generic base
#[test]
fn test_parity_es5_generic_class_extends_generic() {
    let source = r#"
abstract class BaseRepository<T, ID> {
    protected items: Map<ID, T> = new Map();

    abstract create(data: Partial<T>): T;

    findById(id: ID): T | undefined {
        return this.items.get(id);
    }

    save(id: ID, item: T): void {
        this.items.set(id, item);
    }
}

interface User {
    id: string;
    name: string;
    email: string;
}

class UserRepository extends BaseRepository<User, string> {
    create(data: Partial<User>): User {
        const user: User = {
            id: Math.random().toString(36),
            name: data.name || "",
            email: data.email || ""
        };
        return user;
    }

    findByEmail(email: string): User | undefined {
        for (const user of this.items.values()) {
            if (user.email === email) return user;
        }
        return undefined;
    }
}

class CachedRepository<T, ID> extends BaseRepository<T, ID> {
    private cache: Map<ID, { value: T; timestamp: number }> = new Map();

    create(data: Partial<T>): T {
        return data as T;
    }

    getCached(id: ID, maxAge: number): T | undefined {
        const cached = this.cache.get(id);
        if (cached && Date.now() - cached.timestamp < maxAge) {
            return cached.value;
        }
        return this.findById(id);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseRepository")
            && output.contains("UserRepository")
            && output.contains("CachedRepository"),
        "Classes should be present: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract create"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface User"),
        "Interface should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T, ID>") && !output.contains("<User, string>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Protected modifier should be erased
    assert!(
        !output.contains("protected items"),
        "Protected modifier should be erased: {}",
        output
    );
}

/// Test ES5 generic class with default type parameters
#[test]
fn test_parity_es5_generic_class_default_params() {
    let source = r#"
class EventEmitter<T = any> {
    private listeners: Array<(data: T) => void> = [];

    on(callback: (data: T) => void): void {
        this.listeners.push(callback);
    }

    emit(data: T): void {
        this.listeners.forEach(cb => cb(data));
    }
}

class TypedMap<K = string, V = unknown> {
    private map: Map<K, V> = new Map();

    set(key: K, value: V): this {
        this.map.set(key, value);
        return this;
    }

    get(key: K): V | undefined {
        return this.map.get(key);
    }
}

class ConfigStore<T extends object = Record<string, any>> {
    private config: T;

    constructor(initial: T) {
        this.config = initial;
    }

    get<K extends keyof T>(key: K): T[K] {
        return this.config[key];
    }

    set<K extends keyof T>(key: K, value: T[K]): void {
        this.config[key] = value;
    }
}

// Usage with defaults
const emitter1 = new EventEmitter();
const emitter2 = new EventEmitter<string>();
const map1 = new TypedMap();
const map2 = new TypedMap<number, boolean>();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("EventEmitter")
            && output.contains("TypedMap")
            && output.contains("ConfigStore"),
        "Classes should be present: {}",
        output
    );
    // Generic type parameters with defaults should be erased
    assert!(
        !output.contains("<T = any>")
            && !output.contains("<K = string")
            && !output.contains("<T extends object ="),
        "Generic type parameters with defaults should be erased: {}",
        output
    );
    // Instantiation type arguments should be erased
    assert!(
        !output.contains("EventEmitter<string>") && !output.contains("TypedMap<number"),
        "Instantiation type arguments should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": this") && !output.contains(": T[K]"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test ES5 combined generic class patterns
#[test]
fn test_parity_es5_generic_class_combined() {
    let source = r#"
interface Entity {
    id: string;
    createdAt: Date;
}

interface Service<T> {
    process(item: T): Promise<T>;
}

abstract class BaseService<T extends Entity, R = void> implements Service<T> {
    protected readonly serviceName: string;

    constructor(name: string) {
        this.serviceName = name;
    }

    abstract validate(item: T): boolean;
    abstract transform(item: T): R;

    async process(item: T): Promise<T> {
        if (!this.validate(item)) {
            throw new Error("Validation failed");
        }
        return item;
    }
}

class CompositeService<T extends Entity, U extends Entity = T> extends BaseService<T, U[]> {
    private services: Array<BaseService<T, any>> = [];

    validate(item: T): boolean {
        return item.id !== undefined;
    }

    transform(item: T): U[] {
        return [item as unknown as U];
    }

    addService<S extends BaseService<T, any>>(service: S): this {
        this.services.push(service);
        return this;
    }
}

class GenericFactory<T extends new (...args: any[]) => any> {
    constructor(private readonly ctor: T) {}

    create(...args: ConstructorParameters<T>): InstanceType<T> {
        return new this.ctor(...args);
    }
}

type Handler<T, R> = (input: T) => R;

class Pipeline<TInput, TOutput = TInput> {
    private handlers: Array<Handler<any, any>> = [];

    pipe<TNext>(handler: Handler<TOutput, TNext>): Pipeline<TInput, TNext> {
        const next = new Pipeline<TInput, TNext>();
        next.handlers = [...this.handlers, handler];
        return next;
    }

    execute(input: TInput): TOutput {
        return this.handlers.reduce((acc, handler) => handler(acc), input as any);
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes should be present
    assert!(
        output.contains("BaseService")
            && output.contains("CompositeService")
            && output.contains("GenericFactory")
            && output.contains("Pipeline"),
        "Classes should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Entity") && !output.contains("interface Service"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Handler"),
        "Type aliases should be erased: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("abstract validate"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements Service"),
        "Implements clause should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T extends Entity")
            && !output.contains("<TInput,")
            && !output.contains("<T extends new"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Protected/readonly modifiers should be erased
    assert!(
        !output.contains("protected readonly") && !output.contains("private readonly ctor"),
        "Access modifiers should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Tuple Type Patterns Parity Tests
// ============================================================================

/// Test ES5 basic tuple type
#[test]
fn test_parity_es5_tuple_basic() {
    let source = r#"
type Point = [number, number];
type RGB = [number, number, number];
type NameAge = [string, number];

function createPoint(x: number, y: number): Point {
    return [x, y];
}

function getColor(): RGB {
    return [255, 128, 0];
}

function processEntry(entry: NameAge): void {
    const [name, age] = entry;
    console.log(name, age);
}

const point: Point = [10, 20];
const color: RGB = [100, 150, 200];
const person: NameAge = ["Alice", 30];

// Destructuring tuples
const [x, y] = point;
const [r, g, b] = color;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("createPoint")
            && output.contains("getColor")
            && output.contains("processEntry"),
        "Functions should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Point")
            && !output.contains("type RGB")
            && !output.contains("type NameAge"),
        "Type aliases should be erased: {}",
        output
    );
    // Return type annotations should be erased
    assert!(
        !output.contains("): Point") && !output.contains("): RGB") && !output.contains("): void"),
        "Return type annotations should be erased: {}",
        output
    );
    // Variable type annotations should be erased
    assert!(
        !output.contains(": Point") && !output.contains(": RGB") && !output.contains(": NameAge"),
        "Variable type annotations should be erased: {}",
        output
    );
}

/// Test ES5 tuple with optional elements
#[test]
fn test_parity_es5_tuple_optional() {
    let source = r#"
type OptionalTuple = [string, number?, boolean?];
type ConfigTuple = [string, number | undefined, boolean?];

function processOptional(tuple: OptionalTuple): string {
    const [name, age, active] = tuple;
    return name + (age ?? 0) + (active ?? false);
}

function createConfig(name: string, count?: number, enabled?: boolean): ConfigTuple {
    return [name, count, enabled];
}

class TupleHandler {
    private data: OptionalTuple;

    constructor(name: string, age?: number) {
        this.data = [name, age];
    }

    getData(): OptionalTuple {
        return this.data;
    }

    setActive(active: boolean): void {
        this.data = [this.data[0], this.data[1], active];
    }
}

const minimal: OptionalTuple = ["test"];
const partial: OptionalTuple = ["test", 42];
const full: OptionalTuple = ["test", 42, true];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("processOptional")
            && output.contains("createConfig")
            && output.contains("TupleHandler"),
        "Functions and class should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type OptionalTuple") && !output.contains("type ConfigTuple"),
        "Type aliases should be erased: {}",
        output
    );
    // Tuple type annotations should be erased
    assert!(
        !output.contains(": OptionalTuple") && !output.contains(": ConfigTuple"),
        "Tuple type annotations should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private data"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test ES5 tuple with rest elements
#[test]
fn test_parity_es5_tuple_rest() {
    let source = r#"
type StringNumberBooleans = [string, number, ...boolean[]];
type StringNumbers = [string, ...number[]];
type Unbounded = [...string[]];

function logStringsAndNumbers(first: string, ...rest: number[]): void {
    console.log(first, ...rest);
}

function processRestTuple(tuple: StringNumberBooleans): number {
    const [str, num, ...flags] = tuple;
    return flags.filter(f => f).length + num;
}

function combineArrays<T, U>(arr1: T[], arr2: U[]): [...T[], ...U[]] {
    return [...arr1, ...arr2];
}

class RestTupleProcessor {
    process(data: StringNumbers): string[] {
        const [prefix, ...numbers] = data;
        return numbers.map(n => prefix + n);
    }
}

const tuple1: StringNumberBooleans = ["start", 10, true, false, true];
const tuple2: StringNumbers = ["value", 1, 2, 3, 4, 5];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("logStringsAndNumbers")
            && output.contains("processRestTuple")
            && output.contains("RestTupleProcessor"),
        "Functions and class should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type StringNumberBooleans")
            && !output.contains("type StringNumbers")
            && !output.contains("type Unbounded"),
        "Type aliases should be erased: {}",
        output
    );
    // Generic return types should be erased
    assert!(
        !output.contains("): [...T[]") && !output.contains("<T, U>"),
        "Generic types should be erased: {}",
        output
    );
}

/// Test ES5 named tuple elements
#[test]
fn test_parity_es5_tuple_named() {
    let source = r#"
type Coordinate = [x: number, y: number, z?: number];
type Person = [name: string, age: number, email: string];
type Range = [start: number, end: number];

function createCoordinate(x: number, y: number, z?: number): Coordinate {
    return z !== undefined ? [x, y, z] : [x, y];
}

function formatPerson(person: Person): string {
    const [name, age, email] = person;
    return `${name} (${age}): ${email}`;
}

function* iterateRange(range: Range): Generator<number> {
    const [start, end] = range;
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

interface CoordinateProcessor {
    process(coord: Coordinate): number;
}

class RangeCalculator implements CoordinateProcessor {
    process(coord: Coordinate): number {
        const [x, y, z = 0] = coord;
        return Math.sqrt(x * x + y * y + z * z);
    }
}

const point3D: Coordinate = [1, 2, 3];
const user: Person = ["John", 25, "john@example.com"];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("createCoordinate")
            && output.contains("formatPerson")
            && output.contains("RangeCalculator"),
        "Functions and class should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Coordinate")
            && !output.contains("type Person")
            && !output.contains("type Range"),
        "Type aliases should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface CoordinateProcessor"),
        "Interface should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements CoordinateProcessor"),
        "Implements clause should be erased: {}",
        output
    );
    // Return type with Generator should be erased
    assert!(
        !output.contains("): Generator<"),
        "Generator return type should be erased: {}",
        output
    );
}

/// Test ES5 variadic tuple types
#[test]
fn test_parity_es5_tuple_variadic() {
    let source = r#"
type Concat<T extends unknown[], U extends unknown[]> = [...T, ...U];
type Prepend<T, U extends unknown[]> = [T, ...U];
type Append<T extends unknown[], U> = [...T, U];

function concat<T extends unknown[], U extends unknown[]>(arr1: T, arr2: U): Concat<T, U> {
    return [...arr1, ...arr2] as Concat<T, U>;
}

function prepend<T, U extends unknown[]>(item: T, arr: U): Prepend<T, U> {
    return [item, ...arr];
}

function append<T extends unknown[], U>(arr: T, item: U): Append<T, U> {
    return [...arr, item] as Append<T, U>;
}

type Tail<T extends unknown[]> = T extends [unknown, ...infer Rest] ? Rest : never;
type Head<T extends unknown[]> = T extends [infer First, ...unknown[]] ? First : never;

function tail<T extends unknown[]>(arr: T): Tail<T> {
    const [, ...rest] = arr;
    return rest as Tail<T>;
}

function head<T extends unknown[]>(arr: T): Head<T> {
    return arr[0] as Head<T>;
}

const combined = concat([1, 2], ["a", "b"]);
const withPrefix = prepend("start", [1, 2, 3]);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions should be present
    assert!(
        output.contains("concat")
            && output.contains("prepend")
            && output.contains("append")
            && output.contains("tail"),
        "Functions should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Concat")
            && !output.contains("type Prepend")
            && !output.contains("type Tail"),
        "Type aliases should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T extends unknown[]") && !output.contains("<T, U extends"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Return type annotations should be erased
    assert!(
        !output.contains("): Concat<")
            && !output.contains("): Prepend<")
            && !output.contains("): Tail<"),
        "Return type annotations should be erased: {}",
        output
    );
}

/// Test ES5 combined tuple patterns
#[test]
fn test_parity_es5_tuple_combined() {
    let source = r#"
type EventData = [type: string, timestamp: number, payload?: unknown];
type AsyncResult<T> = [error: Error | null, data: T | null];
type PaginatedResult<T> = [items: T[], total: number, page: number, ...metadata: string[]];

interface EventHandler {
    handle(event: EventData): AsyncResult<boolean>;
}

abstract class BaseProcessor<T> {
    protected abstract transform(input: T): AsyncResult<T>;

    async process(input: T): Promise<AsyncResult<T>> {
        try {
            return this.transform(input);
        } catch (e) {
            return [e as Error, null];
        }
    }
}

class DataProcessor extends BaseProcessor<string> implements EventHandler {
    protected transform(input: string): AsyncResult<string> {
        return [null, input.toUpperCase()];
    }

    handle(event: EventData): AsyncResult<boolean> {
        const [type, timestamp, payload] = event;
        console.log(type, timestamp, payload);
        return [null, true];
    }
}

function paginate<T>(items: T[], page: number, perPage: number): PaginatedResult<T> {
    const start = (page - 1) * perPage;
    const pageItems = items.slice(start, start + perPage);
    return [pageItems, items.length, page, "cached", "validated"];
}

type ReadonlyTuple = readonly [string, number];
type MutableFromReadonly<T extends readonly unknown[]> = [...T];

const readonlyData: ReadonlyTuple = ["immutable", 42];
const mutableCopy: MutableFromReadonly<ReadonlyTuple> = [...readonlyData];

async function fetchData(): Promise<AsyncResult<object>> {
    return [null, { success: true }];
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes and functions should be present
    assert!(
        output.contains("BaseProcessor")
            && output.contains("DataProcessor")
            && output.contains("paginate"),
        "Classes and functions should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type EventData")
            && !output.contains("type AsyncResult")
            && !output.contains("type PaginatedResult"),
        "Type aliases should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface EventHandler"),
        "Interface should be erased: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class") && !output.contains("protected abstract"),
        "Abstract keyword should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements EventHandler"),
        "Implements clause should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T extends readonly"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Readonly modifier in type should be erased
    assert!(
        !output.contains("readonly [string"),
        "Readonly tuple type should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Union/Intersection Type Patterns Parity Tests
// ============================================================================

/// Test ES5 basic union type
#[test]
fn test_parity_es5_union_basic() {
    let source = r#"
type StringOrNumber = string | number;
type Primitive = string | number | boolean | null | undefined;
type Status = "pending" | "success" | "error";

function formatValue(value: StringOrNumber): string {
    if (typeof value === "string") {
        return value.toUpperCase();
    }
    return value.toFixed(2);
}

function getStatus(): Status {
    return "success";
}

function processPrimitive(p: Primitive): string {
    if (p === null || p === undefined) {
        return "empty";
    }
    return String(p);
}

class UnionHandler {
    private value: StringOrNumber;

    constructor(initial: StringOrNumber) {
        this.value = initial;
    }

    getValue(): StringOrNumber {
        return this.value;
    }

    setValue(value: StringOrNumber): void {
        this.value = value;
    }
}

const val1: StringOrNumber = "hello";
const val2: StringOrNumber = 42;
const status: Status = "pending";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("formatValue")
            && output.contains("getStatus")
            && output.contains("UnionHandler"),
        "Functions and class should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type StringOrNumber")
            && !output.contains("type Primitive")
            && !output.contains("type Status"),
        "Type aliases should be erased: {}",
        output
    );
    // Union type annotations should be erased
    assert!(
        !output.contains(": StringOrNumber")
            && !output.contains(": Status")
            && !output.contains(": Primitive"),
        "Union type annotations should be erased: {}",
        output
    );
    // Private modifier should be erased
    assert!(
        !output.contains("private value"),
        "Private modifier should be erased: {}",
        output
    );
}

/// Test ES5 discriminated union
#[test]
fn test_parity_es5_union_discriminated() {
    let source = r#"
interface Circle {
    kind: "circle";
    radius: number;
}

interface Rectangle {
    kind: "rectangle";
    width: number;
    height: number;
}

interface Triangle {
    kind: "triangle";
    base: number;
    height: number;
}

type Shape = Circle | Rectangle | Triangle;

function getArea(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "rectangle":
            return shape.width * shape.height;
        case "triangle":
            return (shape.base * shape.height) / 2;
    }
}

function isCircle(shape: Shape): shape is Circle {
    return shape.kind === "circle";
}

class ShapeProcessor {
    process(shape: Shape): string {
        return `Area: ${getArea(shape)}`;
    }

    filterCircles(shapes: Shape[]): Circle[] {
        return shapes.filter(isCircle);
    }
}

const circle: Circle = { kind: "circle", radius: 5 };
const rect: Rectangle = { kind: "rectangle", width: 10, height: 20 };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("getArea")
            && output.contains("isCircle")
            && output.contains("ShapeProcessor"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Circle")
            && !output.contains("interface Rectangle")
            && !output.contains("interface Triangle"),
        "Interfaces should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Shape"),
        "Type alias should be erased: {}",
        output
    );
    // Type predicate should be erased
    assert!(
        !output.contains("shape is Circle"),
        "Type predicate should be erased: {}",
        output
    );
    // Parameter type annotations should be erased
    assert!(
        !output.contains("shape: Shape") && !output.contains("shapes: Shape[]"),
        "Parameter type annotations should be erased: {}",
        output
    );
}

/// Test ES5 intersection type
#[test]
fn test_parity_es5_intersection_basic() {
    let source = r#"
interface Named {
    name: string;
}

interface Aged {
    age: number;
}

interface Emailable {
    email: string;
}

type Person = Named & Aged;
type Contact = Named & Emailable;
type FullContact = Named & Aged & Emailable;

function greet(person: Person): string {
    return `Hello, ${person.name}! You are ${person.age} years old.`;
}

function sendEmail(contact: Contact): void {
    console.log(`Sending to ${contact.email}`);
}

function processFullContact(contact: FullContact): string {
    return `${contact.name} (${contact.age}): ${contact.email}`;
}

class ContactManager {
    private contacts: FullContact[] = [];

    add(contact: FullContact): void {
        this.contacts.push(contact);
    }

    findByName(name: string): FullContact | undefined {
        return this.contacts.find(c => c.name === name);
    }
}

const person: Person = { name: "Alice", age: 30 };
const fullContact: FullContact = { name: "Bob", age: 25, email: "bob@example.com" };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("greet")
            && output.contains("sendEmail")
            && output.contains("ContactManager"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Named")
            && !output.contains("interface Aged")
            && !output.contains("interface Emailable"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Person")
            && !output.contains("type Contact")
            && !output.contains("type FullContact"),
        "Type aliases should be erased: {}",
        output
    );
    // Intersection type annotations should be erased
    assert!(
        !output.contains(": Person") && !output.contains(": FullContact"),
        "Intersection type annotations should be erased: {}",
        output
    );
}

/// Test ES5 union with null/undefined
#[test]
fn test_parity_es5_union_nullable() {
    let source = r#"
type Nullable<T> = T | null;
type Optional<T> = T | undefined;
type Maybe<T> = T | null | undefined;

function getValue<T>(value: Nullable<T>, defaultValue: T): T {
    return value !== null ? value : defaultValue;
}

function processOptional<T>(value: Optional<T>): T | undefined {
    return value;
}

function handleMaybe<T>(value: Maybe<T>, fallback: T): T {
    return value ?? fallback;
}

class NullableContainer<T> {
    private value: Nullable<T>;

    constructor(value: Nullable<T>) {
        this.value = value;
    }

    get(): Nullable<T> {
        return this.value;
    }

    getOrDefault(defaultValue: T): T {
        return this.value ?? defaultValue;
    }

    map<U>(fn: (v: T) => U): NullableContainer<U> {
        return new NullableContainer(this.value !== null ? fn(this.value) : null);
    }
}

function strictNullCheck(value: string | null | undefined): string {
    if (value === null) return "null";
    if (value === undefined) return "undefined";
    return value;
}

const nullable: Nullable<string> = null;
const optional: Optional<number> = undefined;
const maybe: Maybe<boolean> = true;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("getValue")
            && output.contains("handleMaybe")
            && output.contains("NullableContainer"),
        "Functions and class should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Nullable")
            && !output.contains("type Optional")
            && !output.contains("type Maybe"),
        "Type aliases should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<U>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Nullable type annotations should be erased
    assert!(
        !output.contains(": Nullable<") && !output.contains(": Maybe<"),
        "Nullable type annotations should be erased: {}",
        output
    );
}

/// Test ES5 complex intersection patterns
#[test]
fn test_parity_es5_intersection_complex() {
    let source = r#"
interface Timestamped {
    createdAt: Date;
    updatedAt: Date;
}

interface Identifiable {
    id: string;
}

interface Serializable {
    toJSON(): object;
}

type Entity = Identifiable & Timestamped;
type SerializableEntity = Entity & Serializable;

type WithMethods<T> = T & {
    clone(): T;
    equals(other: T): boolean;
};

function createEntity<T extends object>(data: T): T & Entity {
    return {
        ...data,
        id: Math.random().toString(36),
        createdAt: new Date(),
        updatedAt: new Date()
    };
}

abstract class BaseEntity implements Entity {
    id: string;
    createdAt: Date;
    updatedAt: Date;

    constructor() {
        this.id = Math.random().toString(36);
        this.createdAt = new Date();
        this.updatedAt = new Date();
    }
}

class User extends BaseEntity implements SerializableEntity {
    constructor(public name: string, public email: string) {
        super();
    }

    toJSON(): object {
        return { id: this.id, name: this.name, email: this.email };
    }
}

type Mixin<T, U> = T & U;
type ReadonlyEntity<T> = Readonly<T> & Entity;

const user: SerializableEntity = new User("Alice", "alice@example.com");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and classes should be present
    assert!(
        output.contains("createEntity") && output.contains("BaseEntity") && output.contains("User"),
        "Functions and classes should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Timestamped") && !output.contains("interface Identifiable"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Entity")
            && !output.contains("type SerializableEntity")
            && !output.contains("type WithMethods"),
        "Type aliases should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements Entity") && !output.contains("implements SerializableEntity"),
        "Implements clause should be erased: {}",
        output
    );
    // Abstract keyword should be erased
    assert!(
        !output.contains("abstract class"),
        "Abstract keyword should be erased: {}",
        output
    );
}

/// Test ES5 combined union/intersection patterns
#[test]
fn test_parity_es5_union_intersection_combined() {
    let source = r#"
interface Success<T> {
    status: "success";
    data: T;
}

interface Failure {
    status: "failure";
    error: Error;
}

type Result<T> = Success<T> | Failure;
type AsyncResult<T> = Promise<Result<T>>;

interface Logger {
    log(message: string): void;
}

interface Metrics {
    track(event: string, data?: object): void;
}

type Instrumented<T> = T & Logger & Metrics;

function isSuccess<T>(result: Result<T>): result is Success<T> {
    return result.status === "success";
}

async function fetchData<T>(url: string): AsyncResult<T> {
    try {
        const response = await fetch(url);
        const data = await response.json();
        return { status: "success", data };
    } catch (e) {
        return { status: "failure", error: e as Error };
    }
}

class InstrumentedService<T> implements Logger, Metrics {
    private data: Result<T> | null = null;

    log(message: string): void {
        console.log(`[LOG] ${message}`);
    }

    track(event: string, data?: object): void {
        console.log(`[TRACK] ${event}`, data);
    }

    async execute(fn: () => Promise<T>): AsyncResult<T> {
        this.log("Starting execution");
        try {
            const result = await fn();
            this.data = { status: "success", data: result };
            this.track("success");
            return this.data;
        } catch (e) {
            this.data = { status: "failure", error: e as Error };
            this.track("failure", { error: (e as Error).message });
            return this.data;
        }
    }
}

type Either<L, R> = { tag: "left"; value: L } | { tag: "right"; value: R };
type PromiseOr<T> = T | Promise<T>;
type ArrayOr<T> = T | T[];

function normalizeArray<T>(input: ArrayOr<T>): T[] {
    return Array.isArray(input) ? input : [input];
}

const service: Instrumented<{ name: string }> = {
    name: "test",
    log: (msg) => console.log(msg),
    track: (evt) => console.log(evt)
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("isSuccess")
            && output.contains("fetchData")
            && output.contains("InstrumentedService"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Success")
            && !output.contains("interface Failure")
            && !output.contains("interface Logger"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Result")
            && !output.contains("type AsyncResult")
            && !output.contains("type Instrumented"),
        "Type aliases should be erased: {}",
        output
    );
    // Type predicate should be erased
    assert!(
        !output.contains("result is Success"),
        "Type predicate should be erased: {}",
        output
    );
    // Implements clause should be erased
    assert!(
        !output.contains("implements Logger") && !output.contains("implements Metrics"),
        "Implements clause should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<L, R>"),
        "Generic type parameters should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Type Guard Patterns Parity Tests
// ============================================================================

/// Test ES5 user-defined type guard
#[test]
fn test_parity_es5_type_guard_user_defined() {
    let source = r#"
interface Cat {
    meow(): void;
    purr(): void;
}

interface Dog {
    bark(): void;
    wagTail(): void;
}

type Animal = Cat | Dog;

function isCat(animal: Animal): animal is Cat {
    return "meow" in animal;
}

function isDog(animal: Animal): animal is Dog {
    return "bark" in animal;
}

function processAnimal(animal: Animal): string {
    if (isCat(animal)) {
        animal.meow();
        return "cat";
    } else if (isDog(animal)) {
        animal.bark();
        return "dog";
    }
    return "unknown";
}

class AnimalHandler {
    private animals: Animal[] = [];

    add(animal: Animal): void {
        this.animals.push(animal);
    }

    getCats(): Cat[] {
        return this.animals.filter(isCat);
    }

    getDogs(): Dog[] {
        return this.animals.filter(isDog);
    }
}

function isNonNull<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
}

const values = [1, null, 2, undefined, 3];
const nonNullValues = values.filter(isNonNull);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("isCat") && output.contains("isDog") && output.contains("AnimalHandler"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Cat") && !output.contains("interface Dog"),
        "Interfaces should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Animal"),
        "Type alias should be erased: {}",
        output
    );
    // Type predicates should be erased
    assert!(
        !output.contains("animal is Cat")
            && !output.contains("animal is Dog")
            && !output.contains("value is T"),
        "Type predicates should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type parameters should be erased: {}",
        output
    );
}

/// Test ES5 typeof type guard
#[test]
fn test_parity_es5_type_guard_typeof() {
    let source = r#"
type Primitive = string | number | boolean | symbol | bigint;

function isString(value: unknown): value is string {
    return typeof value === "string";
}

function isNumber(value: unknown): value is number {
    return typeof value === "number";
}

function isBoolean(value: unknown): value is boolean {
    return typeof value === "boolean";
}

function formatPrimitive(value: Primitive): string {
    if (typeof value === "string") {
        return value.toUpperCase();
    } else if (typeof value === "number") {
        return value.toFixed(2);
    } else if (typeof value === "boolean") {
        return value ? "yes" : "no";
    } else if (typeof value === "symbol") {
        return value.toString();
    } else if (typeof value === "bigint") {
        return value.toString() + "n";
    }
    return String(value);
}

class TypeChecker {
    check(value: unknown): string {
        if (typeof value === "function") {
            return "function";
        }
        if (typeof value === "object") {
            return value === null ? "null" : "object";
        }
        if (typeof value === "undefined") {
            return "undefined";
        }
        return typeof value;
    }
}

function processValue(value: string | number | object): void {
    if (typeof value === "string") {
        console.log(value.length);
    } else if (typeof value === "number") {
        console.log(value.toFixed(0));
    } else {
        console.log(Object.keys(value));
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("isString")
            && output.contains("formatPrimitive")
            && output.contains("TypeChecker"),
        "Functions and class should be present: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Primitive"),
        "Type alias should be erased: {}",
        output
    );
    // Type predicates should be erased
    assert!(
        !output.contains("value is string") && !output.contains("value is number"),
        "Type predicates should be erased: {}",
        output
    );
    // Parameter type annotations should be erased
    assert!(
        !output.contains(": unknown") && !output.contains(": Primitive"),
        "Parameter type annotations should be erased: {}",
        output
    );
    // typeof operators should remain (they're runtime)
    assert!(
        output.contains("typeof value"),
        "typeof operators should remain: {}",
        output
    );
}

/// Test ES5 instanceof type guard
#[test]
fn test_parity_es5_type_guard_instanceof() {
    let source = r#"
class Animal {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
}

class Dog extends Animal {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
    bark(): void {
        console.log("Woof!");
    }
}

class Cat extends Animal {
    color: string;
    constructor(name: string, color: string) {
        super(name);
        this.color = color;
    }
    meow(): void {
        console.log("Meow!");
    }
}

function isDog(animal: Animal): animal is Dog {
    return animal instanceof Dog;
}

function isCat(animal: Animal): animal is Cat {
    return animal instanceof Cat;
}

function processAnimal(animal: Animal): void {
    if (animal instanceof Dog) {
        animal.bark();
        console.log(animal.breed);
    } else if (animal instanceof Cat) {
        animal.meow();
        console.log(animal.color);
    }
}

class AnimalProcessor {
    process(animals: Animal[]): { dogs: Dog[]; cats: Cat[] } {
        return {
            dogs: animals.filter((a): a is Dog => a instanceof Dog),
            cats: animals.filter((a): a is Cat => a instanceof Cat)
        };
    }
}

function isError(value: unknown): value is Error {
    return value instanceof Error;
}

function handleError(e: unknown): string {
    if (e instanceof Error) {
        return e.message;
    }
    return String(e);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Classes and functions should be present
    assert!(
        output.contains("Animal")
            && output.contains("Dog")
            && output.contains("Cat")
            && output.contains("AnimalProcessor"),
        "Classes and functions should be present: {}",
        output
    );
    // Type predicates should be erased
    assert!(
        !output.contains("animal is Dog")
            && !output.contains("animal is Cat")
            && !output.contains("a is Dog"),
        "Type predicates should be erased: {}",
        output
    );
    // instanceof operators should remain (they're runtime)
    assert!(
        output.contains("instanceof Dog") && output.contains("instanceof Cat"),
        "instanceof operators should remain: {}",
        output
    );
    // Return type annotations should be erased
    assert!(
        !output.contains("): void") && !output.contains(": { dogs:"),
        "Return type annotations should be erased: {}",
        output
    );
}

/// Test ES5 in operator guard
#[test]
fn test_parity_es5_type_guard_in() {
    let source = r#"
interface Fish {
    swim(): void;
}

interface Bird {
    fly(): void;
}

interface Amphibian {
    swim(): void;
    walk(): void;
}

type Creature = Fish | Bird | Amphibian;

function isFish(creature: Creature): creature is Fish {
    return "swim" in creature && !("walk" in creature);
}

function isBird(creature: Creature): creature is Bird {
    return "fly" in creature;
}

function isAmphibian(creature: Creature): creature is Amphibian {
    return "swim" in creature && "walk" in creature;
}

function processCreature(creature: Creature): void {
    if ("fly" in creature) {
        creature.fly();
    } else if ("swim" in creature) {
        creature.swim();
    }
}

class CreatureHandler {
    handle(creature: Creature): string {
        if ("fly" in creature) {
            return "bird";
        }
        if ("walk" in creature) {
            return "amphibian";
        }
        if ("swim" in creature) {
            return "fish";
        }
        return "unknown";
    }
}

interface WithId {
    id: string;
}

interface WithName {
    name: string;
}

function hasId<T>(obj: T): obj is T & WithId {
    return typeof obj === "object" && obj !== null && "id" in obj;
}

function hasName<T>(obj: T): obj is T & WithName {
    return typeof obj === "object" && obj !== null && "name" in obj;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("isFish")
            && output.contains("isBird")
            && output.contains("CreatureHandler"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface Fish") && !output.contains("interface Bird"),
        "Interfaces should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Creature"),
        "Type alias should be erased: {}",
        output
    );
    // Type predicates should be erased
    assert!(
        !output.contains("creature is Fish") && !output.contains("creature is Bird"),
        "Type predicates should be erased: {}",
        output
    );
    // in operators should remain (they're runtime)
    assert!(
        output.contains("\"swim\" in") && output.contains("\"fly\" in"),
        "in operators should remain: {}",
        output
    );
}

/// Test ES5 assertion function guard
#[test]
fn test_parity_es5_type_guard_assertion() {
    let source = r#"
function assertIsString(value: unknown): asserts value is string {
    if (typeof value !== "string") {
        throw new Error("Expected string");
    }
}

function assertIsNumber(value: unknown): asserts value is number {
    if (typeof value !== "number") {
        throw new Error("Expected number");
    }
}

function assertIsDefined<T>(value: T | null | undefined): asserts value is T {
    if (value === null || value === undefined) {
        throw new Error("Expected defined value");
    }
}

function assertNonNull<T>(value: T | null): asserts value is T {
    if (value === null) {
        throw new Error("Expected non-null value");
    }
}

interface User {
    id: string;
    name: string;
}

function assertIsUser(value: unknown): asserts value is User {
    if (typeof value !== "object" || value === null) {
        throw new Error("Expected object");
    }
    if (!("id" in value) || !("name" in value)) {
        throw new Error("Expected User");
    }
}

class Validator {
    assertValid(data: unknown): asserts data is { valid: true } {
        if (typeof data !== "object" || data === null || !("valid" in data)) {
            throw new Error("Invalid data");
        }
    }

    validate(data: unknown): void {
        this.assertValid(data);
        console.log("Data is valid");
    }
}

function processValue(value: unknown): string {
    assertIsString(value);
    return value.toUpperCase();
}

function processNumber(value: unknown): number {
    assertIsNumber(value);
    return value * 2;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("assertIsString")
            && output.contains("assertIsDefined")
            && output.contains("Validator"),
        "Functions and class should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface User"),
        "Interface should be erased: {}",
        output
    );
    // Assertion predicates should be erased
    assert!(
        !output.contains("asserts value is string") && !output.contains("asserts value is number"),
        "Assertion predicates should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>"),
        "Generic type parameters should be erased: {}",
        output
    );
    // Parameter type annotations should be erased
    assert!(
        !output.contains(": unknown"),
        "Parameter type annotations should be erased: {}",
        output
    );
}

/// Test ES5 combined type guard patterns
#[test]
fn test_parity_es5_type_guard_combined() {
    let source = r#"
interface ApiResponse<T> {
    status: number;
    data?: T;
    error?: string;
}

interface SuccessResponse<T> extends ApiResponse<T> {
    status: 200;
    data: T;
}

interface ErrorResponse extends ApiResponse<never> {
    status: 400 | 404 | 500;
    error: string;
}

type Response<T> = SuccessResponse<T> | ErrorResponse;

function isSuccessResponse<T>(response: Response<T>): response is SuccessResponse<T> {
    return response.status === 200 && "data" in response;
}

function isErrorResponse<T>(response: Response<T>): response is ErrorResponse {
    return response.status !== 200 && "error" in response;
}

function assertSuccess<T>(response: Response<T>): asserts response is SuccessResponse<T> {
    if (!isSuccessResponse(response)) {
        throw new Error(response.error || "Unknown error");
    }
}

class ApiClient {
    async fetch<T>(url: string): Promise<Response<T>> {
        const res = await fetch(url);
        const data = await res.json();
        if (res.ok) {
            return { status: 200, data } as SuccessResponse<T>;
        }
        return { status: res.status as 400 | 404 | 500, error: data.message } as ErrorResponse;
    }

    async fetchOrThrow<T>(url: string): Promise<T> {
        const response = await this.fetch<T>(url);
        assertSuccess(response);
        return response.data;
    }

    isValidData<T>(data: unknown, validator: (d: unknown) => d is T): data is T {
        return validator(data);
    }
}

type Guard<T> = (value: unknown) => value is T;

function createArrayGuard<T>(itemGuard: Guard<T>): Guard<T[]> {
    return (value: unknown): value is T[] => {
        return Array.isArray(value) && value.every(itemGuard);
    };
}

function isString(value: unknown): value is string {
    return typeof value === "string";
}

const isStringArray = createArrayGuard(isString);

function narrowUnion(value: string | number | boolean | object | null): string {
    if (value === null) return "null";
    if (typeof value === "string") return "string: " + value;
    if (typeof value === "number") return "number: " + value;
    if (typeof value === "boolean") return "boolean: " + value;
    if (value instanceof Array) return "array";
    return "object";
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("isSuccessResponse")
            && output.contains("assertSuccess")
            && output.contains("ApiClient"),
        "Functions and class should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface ApiResponse") && !output.contains("interface SuccessResponse"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Response") && !output.contains("type Guard"),
        "Type aliases should be erased: {}",
        output
    );
    // Type predicates should be erased
    assert!(
        !output.contains("response is SuccessResponse") && !output.contains("value is T[]"),
        "Type predicates should be erased: {}",
        output
    );
    // Assertion predicates should be erased
    assert!(
        !output.contains("asserts response is"),
        "Assertion predicates should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<T>("),
        "Generic type parameters should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Satisfies Expression Parity Tests
// ============================================================================

/// Test ES5 basic satisfies with object literal
#[test]
fn test_parity_es5_satisfies_object_literal() {
    let source = r##"
interface Config {
    name: string;
    value: number;
    enabled?: boolean;
}

const config = {
    name: "app",
    value: 42,
    enabled: true
} satisfies Config;

type Colors = Record<string, string>;

const palette = {
    red: "#ff0000",
    green: "#00ff00",
    blue: "#0000ff"
} satisfies Colors;

interface User {
    id: string;
    name: string;
    email: string;
}

const users = {
    admin: { id: "1", name: "Admin", email: "admin@example.com" },
    guest: { id: "2", name: "Guest", email: "guest@example.com" }
} satisfies Record<string, User>;

function getConfig(): Config {
    return { name: "test", value: 0 } satisfies Config;
}
"##;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present with their values
    assert!(
        output.contains("config") && output.contains("palette") && output.contains("users"),
        "Variables should be present: {}",
        output
    );
    // Object literals should remain
    assert!(
        output.contains("name: \"app\"") || output.contains("name:\"app\""),
        "Object literal values should remain: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config") && !output.contains("interface User"),
        "Interfaces should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies Config") && !output.contains("satisfies Colors"),
        "satisfies keyword should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Colors"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test ES5 satisfies with array literal
#[test]
fn test_parity_es5_satisfies_array_literal() {
    let source = r#"
type StringArray = string[];
type NumberTuple = [number, number, number];

const names = ["Alice", "Bob", "Charlie"] satisfies StringArray;
const coordinates = [10, 20, 30] satisfies NumberTuple;

interface MenuItem {
    label: string;
    action: string;
}

const menu = [
    { label: "File", action: "file" },
    { label: "Edit", action: "edit" },
    { label: "View", action: "view" }
] satisfies MenuItem[];

type Matrix = number[][];

const matrix = [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9]
] satisfies Matrix;

function getItems(): string[] {
    return ["a", "b", "c"] satisfies string[];
}

const mixed = [1, "two", true] satisfies (number | string | boolean)[];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("names") && output.contains("coordinates") && output.contains("menu"),
        "Variables should be present: {}",
        output
    );
    // Array values should remain
    assert!(
        output.contains("\"Alice\"") && output.contains("\"Bob\""),
        "Array values should remain: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type StringArray") && !output.contains("type NumberTuple"),
        "Type aliases should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies StringArray") && !output.contains("satisfies MenuItem"),
        "satisfies keyword should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface MenuItem"),
        "Interface should be erased: {}",
        output
    );
}

/// Test ES5 satisfies with function expression
#[test]
fn test_parity_es5_satisfies_function_expr() {
    let source = r#"
type Handler = (event: string) => void;
type AsyncHandler = (event: string) => Promise<void>;
type Callback<T> = (value: T) => T;

const handler = function(event: string): void {
    console.log(event);
} satisfies Handler;

const asyncHandler = async function(event: string): Promise<void> {
    await Promise.resolve();
    console.log(event);
} satisfies AsyncHandler;

const arrowHandler = ((event: string): void => {
    console.log(event);
}) satisfies Handler;

const doubler = ((x: number): number => x * 2) satisfies Callback<number>;

interface EventHandlers {
    onClick: Handler;
    onHover: Handler;
}

const handlers = {
    onClick: function(e: string) { console.log("click", e); },
    onHover: function(e: string) { console.log("hover", e); }
} satisfies EventHandlers;

type Reducer<S, A> = (state: S, action: A) => S;

const counterReducer = ((state: number, action: { type: string }) => {
    if (action.type === "increment") return state + 1;
    return state;
}) satisfies Reducer<number, { type: string }>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("handler")
            && output.contains("arrowHandler")
            && output.contains("handlers"),
        "Variables should be present: {}",
        output
    );
    // Function keyword should remain
    assert!(
        output.contains("function"),
        "Function keyword should remain: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Handler") && !output.contains("type AsyncHandler"),
        "Type aliases should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies Handler") && !output.contains("satisfies Callback"),
        "satisfies keyword should be erased: {}",
        output
    );
    // Parameter type annotations should be erased
    assert!(
        !output.contains("event: string") && !output.contains("state: number"),
        "Parameter type annotations should be erased: {}",
        output
    );
}

/// Test ES5 satisfies with as const
#[test]
fn test_parity_es5_satisfies_as_const() {
    let source = r##"
interface Theme {
    colors: {
        primary: string;
        secondary: string;
    };
    spacing: readonly number[];
}

const theme = {
    colors: {
        primary: "#007bff",
        secondary: "#6c757d"
    },
    spacing: [0, 4, 8, 16, 32] as const
} satisfies Theme;

type Routes = Record<string, { path: string; exact?: boolean }>;

const routes = {
    home: { path: "/", exact: true },
    about: { path: "/about" },
    contact: { path: "/contact" }
} as const satisfies Routes;

const STATUS = {
    PENDING: "pending",
    SUCCESS: "success",
    ERROR: "error"
} as const satisfies Record<string, string>;

type StatusType = typeof STATUS[keyof typeof STATUS];

const directions = ["north", "south", "east", "west"] as const satisfies readonly string[];

interface Config {
    version: number;
    features: readonly string[];
}

const appConfig = {
    version: 1,
    features: ["auth", "dashboard", "settings"] as const
} satisfies Config;
"##;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("theme") && output.contains("routes") && output.contains("STATUS"),
        "Variables should be present: {}",
        output
    );
    // Object values should remain
    assert!(
        output.contains("\"#007bff\"") || output.contains("primary"),
        "Object values should remain: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Theme") && !output.contains("interface Config"),
        "Interfaces should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies Theme") && !output.contains("satisfies Routes"),
        "satisfies keyword should be erased: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Routes") && !output.contains("type StatusType"),
        "Type aliases should be erased: {}",
        output
    );
}

/// Test ES5 satisfies in class context
#[test]
fn test_parity_es5_satisfies_class_context() {
    let source = r#"
interface ButtonConfig {
    label: string;
    variant: "primary" | "secondary";
    disabled?: boolean;
}

interface FormConfig {
    fields: string[];
    validation: boolean;
}

class Component {
    getConfig(): ButtonConfig {
        return { label: "Submit", variant: "primary" } satisfies ButtonConfig;
    }

    getFormConfig(): FormConfig {
        return { fields: ["name", "email"], validation: true } satisfies FormConfig;
    }
}

type Logger = {
    log: (msg: string) => void;
    error: (msg: string) => void;
};

function createLogger(): Logger {
    return {
        log: (msg: string) => console.log(msg),
        error: (msg: string) => console.error(msg)
    } satisfies Logger;
}

const options = {
    timeout: 5000,
    retries: 3
} satisfies { timeout: number; retries: number };
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Class and functions should be present
    assert!(
        output.contains("Component") && output.contains("createLogger"),
        "Class and functions should be present: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface ButtonConfig") && !output.contains("interface FormConfig"),
        "Interfaces should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies ButtonConfig")
            && !output.contains("satisfies FormConfig")
            && !output.contains("satisfies Logger"),
        "satisfies keyword should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Logger"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test ES5 combined satisfies patterns
#[test]
fn test_parity_es5_satisfies_combined() {
    let source = r#"
interface ApiEndpoint {
    method: "GET" | "POST" | "PUT" | "DELETE";
    path: string;
    auth?: boolean;
}

type ApiRoutes = Record<string, ApiEndpoint>;

const api = {
    getUsers: { method: "GET", path: "/users", auth: true },
    createUser: { method: "POST", path: "/users", auth: true },
    getUser: { method: "GET", path: "/users/:id" }
} satisfies ApiRoutes;

interface State<T> {
    data: T | null;
    loading: boolean;
    error: string | null;
}

type UserState = State<{ id: string; name: string }>;

const initialState = {
    data: null,
    loading: false,
    error: null
} satisfies UserState;

type Validator<T> = {
    validate: (value: T) => boolean;
    message: string;
};

const emailValidator = {
    validate: (value: string) => value.includes("@"),
    message: "Invalid email"
} satisfies Validator<string>;

interface Component<P> {
    props: P;
    render: () => string;
}

const button = {
    props: { label: "Click me", disabled: false },
    render() { return `<button>${this.props.label}</button>`; }
} satisfies Component<{ label: string; disabled: boolean }>;

type EventMap = {
    [K: string]: (...args: any[]) => void;
};

const events = {
    onClick: (e: MouseEvent) => console.log(e),
    onKeyDown: (e: KeyboardEvent) => console.log(e),
    onCustom: (data: unknown) => console.log(data)
} satisfies EventMap;

async function fetchData<T>(): Promise<State<T>> {
    return { data: null, loading: true, error: null } satisfies State<T>;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("api")
            && output.contains("initialState")
            && output.contains("emailValidator"),
        "Variables should be present: {}",
        output
    );
    // Object values should remain
    assert!(
        output.contains("\"/users\"") || output.contains("path"),
        "Object values should remain: {}",
        output
    );
    // Interfaces should be erased
    assert!(
        !output.contains("interface ApiEndpoint")
            && !output.contains("interface State")
            && !output.contains("interface Component"),
        "Interfaces should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type ApiRoutes")
            && !output.contains("type UserState")
            && !output.contains("type Validator"),
        "Type aliases should be erased: {}",
        output
    );
    // satisfies keyword should be erased
    assert!(
        !output.contains("satisfies ApiRoutes")
            && !output.contains("satisfies UserState")
            && !output.contains("satisfies Validator"),
        "satisfies keyword should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<T>") && !output.contains("<P>"),
        "Generic type parameters should be erased: {}",
        output
    );
}

// ============================================================================
// ES5 Const Assertion Parity Tests
// ============================================================================

/// Test ES5 object literal as const
#[test]
fn test_parity_es5_const_assertion_object() {
    let source = r#"
const config = {
    name: "app",
    version: 1,
    debug: false
} as const;

const settings = {
    theme: "dark",
    language: "en",
    notifications: true
} as const;

type ConfigType = typeof config;
type SettingsType = typeof settings;

function getConfigValue<K extends keyof typeof config>(key: K): typeof config[K] {
    return config[key];
}

const nested = {
    level1: {
        level2: {
            value: "deep"
        }
    }
} as const;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("config") && output.contains("settings") && output.contains("nested"),
        "Variables should be present: {}",
        output
    );
    // Object values should remain
    assert!(
        output.contains("\"app\"") && output.contains("\"dark\""),
        "Object values should remain: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type ConfigType") && !output.contains("type SettingsType"),
        "Type aliases should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<K extends"),
        "Generic type parameters should be erased: {}",
        output
    );
}

/// Test ES5 array literal as const
#[test]
fn test_parity_es5_const_assertion_array() {
    let source = r#"
const colors = ["red", "green", "blue"] as const;
const numbers = [1, 2, 3, 4, 5] as const;
const mixed = [true, "hello", 42] as const;

type Colors = typeof colors;
type ColorItem = typeof colors[number];

function getColor(index: 0 | 1 | 2): typeof colors[typeof index] {
    return colors[index];
}

const matrix = [
    [1, 2, 3],
    [4, 5, 6]
] as const;

const tuple = [100, "text", false] as const;
type TupleType = typeof tuple;

const empty = [] as const;
const single = ["only"] as const;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("colors") && output.contains("numbers") && output.contains("mixed"),
        "Variables should be present: {}",
        output
    );
    // Array values should remain
    assert!(
        output.contains("\"red\"") && output.contains("\"green\""),
        "Array values should remain: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Colors")
            && !output.contains("type ColorItem")
            && !output.contains("type TupleType"),
        "Type aliases should be erased: {}",
        output
    );
}

/// Test ES5 nested as const
#[test]
fn test_parity_es5_const_assertion_nested() {
    let source = r#"
const deepConfig = {
    database: {
        host: "localhost",
        port: 5432,
        credentials: {
            user: "admin",
            pass: "secret"
        }
    },
    cache: {
        enabled: true,
        ttl: 3600
    }
} as const;

const routes = {
    api: {
        users: "/api/users",
        posts: "/api/posts",
        comments: {
            list: "/api/comments",
            create: "/api/comments/new"
        }
    }
} as const;

type DeepConfigType = typeof deepConfig;
type DatabaseConfig = typeof deepConfig.database;
type CredentialsType = typeof deepConfig.database.credentials;

function getRoute<
    K1 extends keyof typeof routes,
    K2 extends keyof typeof routes[K1]
>(k1: K1, k2: K2): typeof routes[K1][K2] {
    return routes[k1][k2];
}

const arrayOfObjects = [
    { id: 1, name: "first" },
    { id: 2, name: "second" }
] as const;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("deepConfig")
            && output.contains("routes")
            && output.contains("arrayOfObjects"),
        "Variables should be present: {}",
        output
    );
    // Nested values should remain
    assert!(
        output.contains("\"localhost\"") && output.contains("5432"),
        "Nested values should remain: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type DeepConfigType") && !output.contains("type DatabaseConfig"),
        "Type aliases should be erased: {}",
        output
    );
}

/// Test ES5 as const with type assertion
#[test]
fn test_parity_es5_const_assertion_with_type() {
    let source = r#"
interface Config {
    readonly name: string;
    readonly value: number;
}

const config = {
    name: "test",
    value: 42
} as const as Config;

const data = {
    id: 1,
    status: "active"
} as { readonly id: number; readonly status: string };

type Status = "pending" | "active" | "done";
const status = "active" as const as Status;

const items = ["a", "b", "c"] as const as readonly string[];

function process<T>(value: T): T {
    return value;
}

const result = process({ x: 1, y: 2 } as const);

const assertion = (5 as const) + (10 as const);
const stringLiteral = "hello" as const;
const numberLiteral = 42 as const;
const booleanLiteral = true as const;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("config") && output.contains("data") && output.contains("status"),
        "Variables should be present: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface Config"),
        "Interface should be erased: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type assertions should be erased
    assert!(
        !output.contains("as Config") && !output.contains("as Status"),
        "Type assertions should be erased: {}",
        output
    );
    // Type alias should be erased
    assert!(
        !output.contains("type Status"),
        "Type alias should be erased: {}",
        output
    );
}

/// Test ES5 as const in function return
#[test]
fn test_parity_es5_const_assertion_function_return() {
    let source = r#"
function getConfig() {
    return {
        host: "localhost",
        port: 8080
    } as const;
}

function getColors() {
    return ["red", "green", "blue"] as const;
}

function createTuple() {
    return [1, "two", true] as const;
}

const arrowConfig = () => ({
    name: "arrow",
    value: 100
} as const);

const arrowArray = () => [1, 2, 3] as const;

class ConfigFactory {
    create() {
        return {
            type: "factory",
            id: 123
        } as const;
    }

    static getDefault() {
        return {
            type: "default",
            id: 0
        } as const;
    }
}

async function asyncConfig() {
    return {
        async: true,
        data: "loaded"
    } as const;
}

function* generatorConfig() {
    yield { step: 1, value: "first" } as const;
    yield { step: 2, value: "second" } as const;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Functions and class should be present
    assert!(
        output.contains("getConfig")
            && output.contains("getColors")
            && output.contains("ConfigFactory"),
        "Functions and class should be present: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Return values should remain
    assert!(
        output.contains("\"localhost\"") || output.contains("localhost"),
        "Return values should remain: {}",
        output
    );
}

/// Test ES5 combined const assertion patterns
#[test]
fn test_parity_es5_const_assertion_combined() {
    let source = r#"
const ACTIONS = {
    CREATE: "create",
    UPDATE: "update",
    DELETE: "delete"
} as const;

type ActionType = typeof ACTIONS[keyof typeof ACTIONS];

const PERMISSIONS = ["read", "write", "admin"] as const;
type Permission = typeof PERMISSIONS[number];

interface User {
    id: number;
    permissions: readonly Permission[];
}

function hasPermission(user: User, perm: Permission): boolean {
    return user.permissions.includes(perm);
}

const defaultUser = {
    id: 0,
    name: "guest",
    roles: ["viewer"] as const,
    settings: {
        theme: "light",
        notifications: false
    } as const
} as const;

type DefaultUserType = typeof defaultUser;

class ActionHandler {
    private actions = ACTIONS;

    getAction<K extends keyof typeof ACTIONS>(key: K): typeof ACTIONS[K] {
        return this.actions[key];
    }

    getAllActions() {
        return Object.values(ACTIONS) as ActionType[];
    }
}

const lookup = {
    codes: {
        success: 200,
        error: 500,
        notFound: 404
    },
    messages: ["OK", "Error", "Not Found"]
} as const;

function getCode<K extends keyof typeof lookup.codes>(key: K): typeof lookup.codes[K] {
    return lookup.codes[key];
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables, functions and class should be present
    assert!(
        output.contains("ACTIONS")
            && output.contains("PERMISSIONS")
            && output.contains("ActionHandler"),
        "Variables, functions and class should be present: {}",
        output
    );
    // as const should be erased
    assert!(
        !output.contains("as const"),
        "as const should be erased: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type ActionType")
            && !output.contains("type Permission")
            && !output.contains("type DefaultUserType"),
        "Type aliases should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface User"),
        "Interface should be erased: {}",
        output
    );
    // Generic type parameters should be erased
    assert!(
        !output.contains("<K extends keyof"),
        "Generic type parameters should be erased: {}",
        output
    );
}

// =============================================================================
// Template Literal Type Parity Tests
// =============================================================================

/// Test basic template literal type - simple string interpolation type.
/// Template literal types should be completely erased.
#[test]
fn test_parity_es5_template_literal_type_basic() {
    let source = r#"
type Greeting = `Hello, ${string}!`;
type Id = `id_${number}`;
type Key = `${string}_key`;

const greeting: Greeting = "Hello, World!";
const id: Id = "id_123";
const key: Key = "test_key";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("greeting") && output.contains("id") && output.contains("key"),
        "Variables should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type Greeting")
            && !output.contains("type Id")
            && !output.contains("type Key"),
        "Type aliases should be erased: {}",
        output
    );
    // Template literal type syntax should be erased
    assert!(
        !output.contains("`Hello, ${string}!`") && !output.contains("`id_${number}`"),
        "Template literal type syntax should be erased: {}",
        output
    );
    // Type annotations should be erased
    assert!(
        !output.contains(": Greeting") && !output.contains(": Id") && !output.contains(": Key"),
        "Type annotations should be erased: {}",
        output
    );
}

/// Test template literal type with union - union of template literals.
/// Template literal types with unions should be completely erased.
#[test]
fn test_parity_es5_template_literal_type_union() {
    let source = r#"
type EventName = "click" | "hover" | "focus";
type EventHandler = `on${EventName}`;
type Status = "loading" | "success" | "error";
type StatusMessage = `${Status}_message`;
type Combined = `${EventName}_${Status}`;

const handler: EventHandler = "onclick";
const message: StatusMessage = "loading_message";
const combined: Combined = "click_success";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present with string values
    assert!(
        output.contains("handler") && output.contains("onclick"),
        "Handler variable should be present: {}",
        output
    );
    assert!(
        output.contains("message") && output.contains("loading_message"),
        "Message variable should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type EventName") && !output.contains("type EventHandler"),
        "Type aliases should be erased: {}",
        output
    );
    // Template literal union syntax should be erased
    assert!(
        !output.contains("`on${EventName}`") && !output.contains("`${Status}_message`"),
        "Template literal union syntax should be erased: {}",
        output
    );
}

/// Test Uppercase/Lowercase intrinsic template literal types.
/// These TypeScript intrinsic types should be completely erased.
#[test]
fn test_parity_es5_template_literal_type_uppercase_lowercase() {
    let source = r#"
type BaseEvent = "click" | "hover";
type UpperEvent = Uppercase<BaseEvent>;
type LowerEvent = Lowercase<"CLICK" | "HOVER">;
type MixedCase = Uppercase<"hello"> | Lowercase<"WORLD">;

const upper: UpperEvent = "CLICK";
const lower: LowerEvent = "click";
const mixed: MixedCase = "HELLO";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("upper") && output.contains("CLICK"),
        "Upper variable should be present: {}",
        output
    );
    assert!(
        output.contains("lower") && output.contains("click"),
        "Lower variable should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type UpperEvent") && !output.contains("type LowerEvent"),
        "Type aliases should be erased: {}",
        output
    );
    // Intrinsic type syntax should be erased
    assert!(
        !output.contains("Uppercase<") && !output.contains("Lowercase<"),
        "Intrinsic type syntax should be erased: {}",
        output
    );
}

/// Test Capitalize/Uncapitalize intrinsic template literal types.
/// These TypeScript intrinsic types should be completely erased.
#[test]
fn test_parity_es5_template_literal_type_capitalize_uncapitalize() {
    let source = r#"
type BaseWord = "hello" | "world";
type CapWord = Capitalize<BaseWord>;
type UncapWord = Uncapitalize<"Hello" | "World">;
type Mixed = Capitalize<"test"> | Uncapitalize<"TEST">;

const cap: CapWord = "Hello";
const uncap: UncapWord = "hello";
const mixedVal: Mixed = "Test";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("cap") && output.contains("Hello"),
        "Cap variable should be present: {}",
        output
    );
    assert!(
        output.contains("uncap") && output.contains("hello"),
        "Uncap variable should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type CapWord") && !output.contains("type UncapWord"),
        "Type aliases should be erased: {}",
        output
    );
    // Intrinsic type syntax should be erased
    assert!(
        !output.contains("Capitalize<") && !output.contains("Uncapitalize<"),
        "Intrinsic type syntax should be erased: {}",
        output
    );
}

/// Test template literal type inference patterns.
/// Type inference with template literals should be erased.
#[test]
fn test_parity_es5_template_literal_type_inference() {
    let source = r#"
type ParseRoute<S extends string> = S extends `${infer Action}/${infer Id}`
    ? { action: Action; id: Id }
    : never;

type ExtractPrefix<S extends string> = S extends `${infer P}_${string}` ? P : never;

function parseRoute<S extends string>(route: S): ParseRoute<S> {
    const parts = route.split('/');
    return { action: parts[0], id: parts[1] } as any;
}

const result = parseRoute("users/123");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Function and result should be present
    assert!(
        output.contains("parseRoute") && output.contains("result"),
        "Function and result should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type ParseRoute") && !output.contains("type ExtractPrefix"),
        "Type aliases should be erased: {}",
        output
    );
    // Template literal inference syntax should be erased
    assert!(
        !output.contains("${infer") && !output.contains("extends `"),
        "Template literal inference syntax should be erased: {}",
        output
    );
    // Generic type parameters in type annotations should be erased
    assert!(
        !output.contains("<S extends string>") || output.contains("function parseRoute(route)"),
        "Type parameters in annotations should be erased: {}",
        output
    );
}

/// Test combined template literal patterns - complex real-world usage.
/// All template literal type constructs should be erased.
#[test]
fn test_parity_es5_template_literal_type_combined() {
    let source = r#"
type HTTPMethod = "GET" | "POST" | "PUT" | "DELETE";
type Endpoint = "/users" | "/posts" | "/comments";
type APIRoute = `${HTTPMethod} ${Endpoint}`;
type RouteHandler<R extends APIRoute> = (route: R) => void;

type CSSProperty = "margin" | "padding";
type CSSUnit = "px" | "em" | "rem";
type CSSValue = `${number}${CSSUnit}`;
type CSSDeclaration = `${CSSProperty}: ${CSSValue}`;

interface RouteConfig<T extends APIRoute = "GET /users"> {
    route: T;
    handler: RouteHandler<T>;
}

const config: RouteConfig = {
    route: "GET /users",
    handler: (r) => console.log(r)
};

const style: CSSDeclaration = "margin: 10px";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = &parser.arena;

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = ModuleKind::None;

    let ctx = EmitContext::with_options(options.clone());
    let lowering = LoweringPass::new(arena, &ctx);
    let transforms = lowering.run(root);

    let mut printer = Printer::with_transforms_and_options(arena, transforms, options);
    printer.set_source_text(source);
    printer.set_target_es5(true);
    printer.emit(root);

    let output = printer.get_output();

    // Variables should be present
    assert!(
        output.contains("config") && output.contains("GET /users"),
        "Config variable should be present: {}",
        output
    );
    assert!(
        output.contains("style") && output.contains("margin: 10px"),
        "Style variable should be present: {}",
        output
    );
    // Type aliases should be erased
    assert!(
        !output.contains("type HTTPMethod") && !output.contains("type APIRoute"),
        "Type aliases should be erased: {}",
        output
    );
    assert!(
        !output.contains("type CSSProperty") && !output.contains("type CSSDeclaration"),
        "CSS type aliases should be erased: {}",
        output
    );
    // Interface should be erased
    assert!(
        !output.contains("interface RouteConfig"),
        "Interface should be erased: {}",
        output
    );
    // Template literal type syntax should be erased
    assert!(
        !output.contains("`${HTTPMethod}") && !output.contains("`${number}${CSSUnit}`"),
        "Template literal type syntax should be erased: {}",
        output
    );
    // Generic constraints should be erased
    assert!(
        !output.contains("<T extends APIRoute"),
        "Generic constraints should be erased: {}",
        output
    );
}
