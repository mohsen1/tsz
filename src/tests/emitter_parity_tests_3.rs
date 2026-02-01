//! Emitter parity tests - Part 3

use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
#[allow(unused_imports)]
use crate::emitter_parity_test_utils::assert_parity;
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;

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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
