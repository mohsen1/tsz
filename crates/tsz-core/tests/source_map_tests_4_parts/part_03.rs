#[test]
fn test_source_map_class_expr_es5_named() {
    let source = r##"const MyClass = class NamedClass {
    static className = "NamedClass";

    constructor(public id: number) {}

    describe(): string {
        return NamedClass.className + "#" + this.id;
    }
};

const instance = new MyClass(1);
console.log(instance.describe());

const Container = class InnerClass {
    static create() {
        return new InnerClass();
    }

    value = 100;
};

const created = Container.create();
console.log(created.value);"##;

    let (parser, root) = parse_test_source(source);

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
        output.contains("MyClass"),
        "expected MyClass in output. output: {output}"
    );
    assert!(
        output.contains("describe"),
        "expected describe method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn test_source_map_arrow_es5_this_binding() {
    let source = r#"class Counter {
    count = 0;

    increment = () => {
        this.count++;
        return this.count;
    };

    decrement = () => {
        this.count--;
        return this.count;
    };

    addMultiple = (n: number) => {
        for (let i = 0; i < n; i++) {
            this.increment();
        }
        return this.count;
    };
}

class EventHandler {
    name = "Handler";

    handleClick = () => {
        console.log(this.name + " clicked");
    };

    handleHover = () => {
        console.log(this.name + " hovered");
    };
}

const counter = new Counter();
console.log(counter.increment());
console.log(counter.addMultiple(5));

const handler = new EventHandler();
handler.handleClick();
handler.handleHover();"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("EventHandler"),
        "expected EventHandler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow this binding"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_rest_params() {
    let source = r#"const sum = (...numbers: number[]) => numbers.reduce((a, b) => a + b, 0);

const concat = (...strings: string[]) => strings.join("");

const first = <T>(first: T, ...rest: T[]) => first;

const last = <T>(...items: T[]) => items[items.length - 1];

const collect = (prefix: string, ...values: any[]) => {
    return prefix + ": " + values.join(", ");
};

const spread = (...args: number[]) => {
    const [head, ...tail] = args;
    return { head, tail };
};

console.log(sum(1, 2, 3, 4, 5));
console.log(concat("a", "b", "c"));
console.log(first(1, 2, 3));
console.log(last(1, 2, 3, 4));
console.log(collect("Items", 1, 2, 3));
console.log(spread(10, 20, 30, 40));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("sum"),
        "expected sum in output. output: {output}"
    );
    assert!(
        output.contains("concat"),
        "expected concat in output. output: {output}"
    );
    assert!(
        output.contains("collect"),
        "expected collect in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow rest params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_default_params() {
    let source = r#"const greet = (name: string = "World") => "Hello, " + name;

const power = (base: number, exp: number = 2) => Math.pow(base, exp);

const configure = (host: string = "localhost", port: number = 8080) => {
    return { host, port };
};

const format = (value: number, prefix: string = "$", suffix: string = "") => {
    return prefix + value.toFixed(2) + suffix;
};

const createUser = (name: string, age: number = 0, active: boolean = true) => ({
    name,
    age,
    active
});

console.log(greet());
console.log(greet("Alice"));
console.log(power(2));
console.log(power(2, 10));
console.log(configure());
console.log(format(123.456));
console.log(createUser("Bob"));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("greet"),
        "expected greet in output. output: {output}"
    );
    assert!(
        output.contains("power"),
        "expected power in output. output: {output}"
    );
    assert!(
        output.contains("configure"),
        "expected configure in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow default params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_destructuring_params() {
    let source = r#"const getFullName = ({ first, last }: { first: string; last: string }) =>
    first + " " + last;

const getCoords = ([x, y]: [number, number]) => ({ x, y });

const processUser = ({ name, age = 0 }: { name: string; age?: number }) => {
    return name + " is " + age + " years old";
};

const extractValues = ({ a, b, ...rest }: { a: number; b: number; [key: string]: number }) => {
    return { sum: a + b, rest };
};

const handleEvent = ({ type, target: { id } }: { type: string; target: { id: string } }) => {
    console.log(type + " on " + id);
};

console.log(getFullName({ first: "John", last: "Doe" }));
console.log(getCoords([10, 20]));
console.log(processUser({ name: "Alice" }));
console.log(extractValues({ a: 1, b: 2, c: 3, d: 4 }));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("getFullName"),
        "expected getFullName in output. output: {output}"
    );
    assert!(
        output.contains("processUser"),
        "expected processUser in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow destructuring params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_class_property() {
    let source = r#"class Calculator {
    add = (a: number, b: number) => a + b;
    subtract = (a: number, b: number) => a - b;
    multiply = (a: number, b: number) => a * b;
    divide = (a: number, b: number) => b !== 0 ? a / b : 0;
}

class Formatter {
    prefix: string;
    suffix: string;

    constructor(prefix: string = "", suffix: string = "") {
        this.prefix = prefix;
        this.suffix = suffix;
    }

    format = (value: any) => this.prefix + String(value) + this.suffix;
    formatNumber = (n: number) => this.format(n.toFixed(2));
}

class Logger {
    static log = (message: string) => console.log("[LOG] " + message);
    static warn = (message: string) => console.warn("[WARN] " + message);
    static error = (message: string) => console.error("[ERROR] " + message);
}

const calc = new Calculator();
console.log(calc.add(1, 2));
console.log(calc.multiply(3, 4));

const fmt = new Formatter("$", " USD");
console.log(fmt.formatNumber(123.456));

Logger.log("Test message");"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("Formatter"),
        "expected Formatter in output. output: {output}"
    );
    assert!(
        output.contains("Logger"),
        "expected Logger in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow class property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_higher_order() {
    let source = r#"const createAdder = (n: number) => (x: number) => x + n;

const createMultiplier = (factor: number) => (x: number) => x * factor;

const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

const pipe = <T>(...fns: ((x: T) => T)[]) => (x: T) => fns.reduce((v, f) => f(v), x);

const curry = (fn: (a: number, b: number) => number) => (a: number) => (b: number) => fn(a, b);

const memoize = <T, R>(fn: (arg: T) => R) => {
    const cache = new Map<T, R>();
    return (arg: T) => {
        if (cache.has(arg)) return cache.get(arg)!;
        const result = fn(arg);
        cache.set(arg, result);
        return result;
    };
};

const add5 = createAdder(5);
const double = createMultiplier(2);
const addThenDouble = compose(double, add5);

console.log(add5(10));
console.log(double(7));
console.log(addThenDouble(3));

const increment = (x: number) => x + 1;
const triple = (x: number) => x * 3;
const pipeline = pipe(increment, double, triple);
console.log(pipeline(2));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("createAdder"),
        "expected createAdder in output. output: {output}"
    );
    assert!(
        output.contains("compose"),
        "expected compose in output. output: {output}"
    );
    assert!(
        output.contains("memoize"),
        "expected memoize in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for higher order arrows"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_callbacks() {
    let source = r#"const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const sum = numbers.reduce((acc, n) => acc + n, 0);
const found = numbers.find(n => n > 5);
const allPositive = numbers.every(n => n > 0);
const hasNegative = numbers.some(n => n < 0);

const sorted = [...numbers].sort((a, b) => b - a);

const users = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 25 },
    { name: "Charlie", age: 35 }
];

const names = users.map(u => u.name);
const adults = users.filter(u => u.age >= 18);
const totalAge = users.reduce((sum, u) => sum + u.age, 0);
const youngest = users.reduce((min, u) => u.age < min.age ? u : min);

setTimeout(() => console.log("delayed"), 0);
Promise.resolve(42).then(n => n * 2).then(n => console.log(n));

console.log(doubled, evens, sum, found);
console.log(names, adults, totalAge, youngest);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("doubled"),
        "expected doubled in output. output: {output}"
    );
    assert!(
        output.contains("users"),
        "expected users in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow callbacks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_comprehensive() {
    let source = r#"// Comprehensive arrow function patterns for ES5 transform testing

// Expression body
const add = (a: number, b: number) => a + b;
const identity = <T>(x: T) => x;

// Block body
const process = (data: number[]) => {
    const result = data.map(x => x * 2);
    return result.filter(x => x > 5);
};

// Default parameters
const greet = (name: string = "World") => "Hello, " + name;
const configure = (host: string = "localhost", port: number = 8080) => ({ host, port });

// Rest parameters
const sum = (...nums: number[]) => nums.reduce((a, b) => a + b, 0);
const collect = (first: string, ...rest: string[]) => [first, ...rest];

// Destructuring parameters
const getPoint = ({ x, y }: { x: number; y: number }) => x + y;
const getFirst = ([a, b]: [number, number]) => a;

// This binding in class
class Timer {
    seconds = 0;

    tick = () => {
        this.seconds++;
        return this.seconds;
    };

    reset = () => {
        this.seconds = 0;
    };
}

// Higher-order functions
const createMultiplier = (factor: number) => (x: number) => x * factor;
const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

// Callbacks
const numbers = [1, 2, 3, 4, 5];
const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const total = numbers.reduce((acc, n) => acc + n, 0);

// Async arrows
const fetchData = async (url: string) => {
    const response = await fetch(url);
    return response.json();
};

// Arrow IIFE
const result = ((x: number) => x * x)(5);

// Nested arrows
const outer = (a: number) => (b: number) => (c: number) => a + b + c;

// Arrow returning object
const createUser = (name: string, age: number) => ({ name, age, active: true });

// Generic arrow
const mapArray = <T, U>(arr: T[], fn: (x: T) => U) => arr.map(fn);

// Usage
console.log(add(1, 2));
console.log(process([1, 2, 3, 4, 5, 6]));
console.log(greet());
console.log(sum(1, 2, 3, 4, 5));
console.log(getPoint({ x: 10, y: 20 }));

const timer = new Timer();
console.log(timer.tick());
timer.reset();

const triple = createMultiplier(3);
console.log(triple(4));

console.log(doubled, evens, total);
console.log(result);
console.log(outer(1)(2)(3));
console.log(createUser("Alice", 30));
console.log(mapArray([1, 2, 3], x => x * 2));"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        output.contains("Timer"),
        "expected Timer class in output. output: {output}"
    );
    assert!(
        output.contains("createMultiplier"),
        "expected createMultiplier in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive arrow functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 Template Literal Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_template_literal_es5_basic() {
    let source = r#"const greeting = `Hello, World!`;
const simple = `Just a string`;
const empty = ``;
const withNewline = `Line 1
Line 2`;
console.log(greeting, simple, empty, withNewline);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("greeting"),
        "expected greeting in output. output: {output}"
    );
    assert!(
        output.contains("Hello"),
        "expected Hello in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic template literals"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_expression() {
    let source = r#"const name = "Alice";
const age = 30;
const message = `Hello, ${name}!`;
const info = `${name} is ${age} years old`;
const calc = `Result: ${2 + 3 * 4}`;
const nested = `Outer ${`Inner ${name}`}`;
console.log(message, info, calc, nested);"#;

    let (parser, root) = parse_test_source(source);

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
        output.contains("name"),
        "expected name in output. output: {output}"
    );
    assert!(
        output.contains("message"),
        "expected message in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for template expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

