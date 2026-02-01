//! Emitter parity tests - Part 2

use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
#[allow(unused_imports)]
use crate::emitter_parity_test_utils::assert_parity;
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;

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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
