#[test]
fn test_source_map_class_expr_es5_in_return() {
    let source = r#"function createCounter(initial: number) {
    return class {
        private count: number = initial;

        increment(): number {
            return ++this.count;
        }

        decrement(): number {
            return --this.count;
        }

        getCount(): number {
            return this.count;
        }
    };
}

const Counter = createCounter(10);
const counter = new Counter();
console.log(counter.increment());
console.log(counter.decrement());

function makeLogger(prefix: string) {
    return class Logger {
        log(message: string): void {
            console.log(prefix + ": " + message);
        }
    };
}

const PrefixedLogger = makeLogger("[INFO]");
const logger = new PrefixedLogger();
logger.log("Hello");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("createCounter"),
        "expected createCounter in output. output: {output}"
    );
    assert!(
        output.contains("increment"),
        "expected increment method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_extends() {
    let source = r#"class Base {
    constructor(public name: string) {}

    greet(): string {
        return "Hello, " + this.name;
    }
}

const Extended = class extends Base {
    constructor(name: string, public age: number) {
        super(name);
    }

    greet(): string {
        return super.greet() + ", age " + this.age;
    }
};

const instance = new Extended("Alice", 30);
console.log(instance.greet());

function createDerived(base: typeof Base) {
    return class extends base {
        extra = "additional";

        getExtra(): string {
            return this.extra;
        }
    };
}

const Derived = createDerived(Base);
const derived = new Derived("Bob");
console.log(derived.greet());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Base"),
        "expected Base class in output. output: {output}"
    );
    assert!(
        output.contains("Extended"),
        "expected Extended in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with extends"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_static() {
    let source = r#"const Singleton = class {
    private static instance: any;
    private value: number;

    private constructor(value: number) {
        this.value = value;
    }

    static getInstance(): any {
        if (!Singleton.instance) {
            Singleton.instance = new Singleton(42);
        }
        return Singleton.instance;
    }

    getValue(): number {
        return this.value;
    }
};

const Registry = class {
    static items: Map<string, any> = new Map();

    static register(key: string, value: any): void {
        Registry.items.set(key, value);
    }

    static get(key: string): any {
        return Registry.items.get(key);
    }
};

Registry.register("test", { data: 123 });
console.log(Registry.get("test"));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Singleton"),
        "expected Singleton in output. output: {output}"
    );
    assert!(
        output.contains("Registry"),
        "expected Registry in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with static"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_variable() {
    let source = r#"let DynamicClass = class {
    value = 1;
};

DynamicClass = class {
    value = 2;
    double() { return this.value * 2; }
};

const instance = new DynamicClass();
console.log(instance.double());

var ReassignableClass = class First {
    type = "first";
};

ReassignableClass = class Second {
    type = "second";
    getType() { return this.type; }
};

const obj = new ReassignableClass();
console.log(obj.getType());"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("DynamicClass"),
        "expected DynamicClass in output. output: {output}"
    );
    assert!(
        output.contains("ReassignableClass"),
        "expected ReassignableClass in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in variable"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_array() {
    let source = r#"const classes = [
    class Alpha {
        name = "Alpha";
        greet() { return "I am " + this.name; }
    },
    class Beta {
        name = "Beta";
        greet() { return "I am " + this.name; }
    },
    class Gamma {
        name = "Gamma";
        greet() { return "I am " + this.name; }
    }
];

for (const Cls of classes) {
    const instance = new Cls();
    console.log(instance.greet());
}

const handlers = [
    class { handle(x: number) { return x * 2; } },
    class { handle(x: number) { return x + 10; } },
    class { handle(x: number) { return x - 5; } }
];

const value = 10;
handlers.forEach(Handler => {
    const h = new Handler();
    console.log(h.handle(value));
});"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("classes"),
        "expected classes array in output. output: {output}"
    );
    assert!(
        output.contains("handlers"),
        "expected handlers array in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in array"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_in_object() {
    let source = r#"const components = {
    Button: class {
        label: string;
        constructor(label: string) { this.label = label; }
        render() { return "<button>" + this.label + "</button>"; }
    },
    Input: class {
        placeholder: string;
        constructor(placeholder: string) { this.placeholder = placeholder; }
        render() { return "<input placeholder=\"" + this.placeholder + "\">"; }
    },
    Label: class {
        text: string;
        constructor(text: string) { this.text = text; }
        render() { return "<label>" + this.text + "</label>"; }
    }
};

const btn = new components.Button("Click me");
console.log(btn.render());

const registry = {
    handlers: {
        click: class { execute() { console.log("click"); } },
        hover: class { execute() { console.log("hover"); } }
    }
};

const clickHandler = new registry.handlers.click();
clickHandler.execute();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("components"),
        "expected components object in output. output: {output}"
    );
    assert!(
        output.contains("registry"),
        "expected registry object in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression in object"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_with_methods() {
    let source = r#"const Calculator = class {
    private result: number = 0;

    constructor(initial: number = 0) {
        this.result = initial;
    }

    add(value: number): this {
        this.result += value;
        return this;
    }

    subtract(value: number): this {
        this.result -= value;
        return this;
    }

    multiply(value: number): this {
        this.result *= value;
        return this;
    }

    divide(value: number): this {
        if (value !== 0) {
            this.result /= value;
        }
        return this;
    }

    getResult(): number {
        return this.result;
    }

    reset(): this {
        this.result = 0;
        return this;
    }
};

const calc = new Calculator(10);
const result = calc.add(5).multiply(2).subtract(10).getResult();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Calculator"),
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("add"),
        "expected add method in output. output: {output}"
    );
    assert!(
        output.contains("multiply"),
        "expected multiply method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expression with methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_comprehensive() {
    let source = r#"// Comprehensive class expression patterns for ES5 transform testing

// Anonymous class expression
const Anonymous = class {
    value = 42;
    getValue() { return this.value; }
};

// Named class expression
const Named = class InnerName {
    static type = "Named";
    describe() { return InnerName.type; }
};

// Class expression with extends
class BaseClass {
    name: string;
    constructor(name: string) { this.name = name; }
}

const Derived = class extends BaseClass {
    extra = "derived";
    constructor(name: string) {
        super(name);
    }
    getInfo() { return this.name + " - " + this.extra; }
};

// Class expression with static members
const WithStatic = class {
    static count = 0;
    id: number;

    constructor() {
        WithStatic.count++;
        this.id = WithStatic.count;
    }

    static getCount() { return WithStatic.count; }
};

// Factory function returning class expression
function createClass<T>(defaultValue: T) {
    return class {
        value: T = defaultValue;
        getValue(): T { return this.value; }
        setValue(v: T): void { this.value = v; }
    };
}

const StringClass = createClass("hello");
const NumberClass = createClass(123);

// Class expressions in array
const classArray = [
    class { type = "A"; },
    class { type = "B"; },
    class { type = "C"; }
];

// Class expressions in object
const classMap = {
    first: class First { id = 1; },
    second: class Second { id = 2; },
    third: class Third { id = 3; }
};

// IIFE with class expression
const IIFEClass = (function() {
    const privateData = "secret";
    return class {
        getPrivate() { return privateData; }
    };
})();

// Class expression with accessors
const WithAccessors = class {
    private _value: number = 0;

    get value(): number { return this._value; }
    set value(v: number) { this._value = v; }

    get doubled(): number { return this._value * 2; }
};

// Usage
const anon = new Anonymous();
console.log(anon.getValue());

const named = new Named();
console.log(named.describe());

const derived = new Derived("test");
console.log(derived.getInfo());

new WithStatic();
new WithStatic();
console.log(WithStatic.getCount());

const strInstance = new StringClass();
console.log(strInstance.getValue());

const numInstance = new NumberClass();
console.log(numInstance.getValue());

for (const Cls of classArray) {
    console.log(new Cls().type);
}

console.log(new classMap.first().id);

const iifeInstance = new IIFEClass();
console.log(iifeInstance.getPrivate());

const accessorInstance = new WithAccessors();
accessorInstance.value = 21;
console.log(accessorInstance.doubled);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("Anonymous"),
        "expected Anonymous in output. output: {output}"
    );
    assert!(
        output.contains("Named"),
        "expected Named in output. output: {output}"
    );
    assert!(
        output.contains("Derived"),
        "expected Derived in output. output: {output}"
    );
    assert!(
        output.contains("WithStatic"),
        "expected WithStatic in output. output: {output}"
    );
    assert!(
        output.contains("createClass"),
        "expected createClass function in output. output: {output}"
    );
    assert!(
        output.contains("WithAccessors"),
        "expected WithAccessors in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Arrow Function Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_arrow_es5_expression_body() {
    let source = r#"const add = (a: number, b: number) => a + b;
const square = (x: number) => x * x;
const identity = <T>(x: T) => x;

const double = (n: number) => n * 2;
const negate = (n: number) => -n;
const toString = (x: any) => String(x);

console.log(add(1, 2));
console.log(square(5));
console.log(double(10));
console.log(negate(5));
console.log(toString(42));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("add"),
        "expected add in output. output: {output}"
    );
    assert!(
        output.contains("square"),
        "expected square in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow expression body"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_block_body() {
    let source = r#"const processData = (data: number[]) => {
    const filtered = data.filter(x => x > 0);
    const doubled = filtered.map(x => x * 2);
    return doubled.reduce((a, b) => a + b, 0);
};

const validateInput = (input: string) => {
    if (!input) {
        throw new Error("Invalid input");
    }
    const trimmed = input.trim();
    if (trimmed.length === 0) {
        return null;
    }
    return trimmed.toUpperCase();
};

const factorial = (n: number): number => {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
};

console.log(processData([1, -2, 3, -4, 5]));
console.log(validateInput("  hello  "));
console.log(factorial(5));"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("processData"),
        "expected processData in output. output: {output}"
    );
    assert!(
        output.contains("validateInput"),
        "expected validateInput in output. output: {output}"
    );
    assert!(
        output.contains("factorial"),
        "expected factorial in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow block body"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

