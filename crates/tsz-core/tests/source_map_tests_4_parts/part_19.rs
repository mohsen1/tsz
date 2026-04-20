/// Test source map generation for private field read access in ES5 output.
/// Validates that reading private fields (#field) generates proper source mappings.
#[test]
fn test_source_map_private_field_read_es5() {
    let source = r#"class Counter {
    #count = 0;

    getCount() {
        return this.#count;
    }
}

const counter = new Counter();
console.log(counter.getCount());"#;

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("getCount"),
        "expected getCount method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field read"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private field write access in ES5 output.
/// Validates that writing to private fields (#field = value) generates proper source mappings.
#[test]
fn test_source_map_private_field_write_es5() {
    let source = r#"class Counter {
    #count = 0;

    increment() {
        this.#count = this.#count + 1;
    }

    reset() {
        this.#count = 0;
    }
}

const counter = new Counter();
counter.increment();
counter.reset();"#;

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("increment"),
        "expected increment method in output. output: {output}"
    );
    assert!(
        output.contains("reset"),
        "expected reset method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private field write"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private method calls in ES5 output.
/// Validates that calling private methods (#`method()`) generates proper source mappings.
#[test]
fn test_source_map_private_method_call_es5() {
    let source = r#"class Calculator {
    #validate(n: number): boolean {
        return n >= 0;
    }

    #compute(a: number, b: number): number {
        return a * b;
    }

    multiply(x: number, y: number): number {
        if (this.#validate(x) && this.#validate(y)) {
            return this.#compute(x, y);
        }
        return 0;
    }
}

const calc = new Calculator();
console.log(calc.multiply(5, 3));"#;

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
        "expected Calculator class in output. output: {output}"
    );
    assert!(
        output.contains("multiply"),
        "expected multiply method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private accessors (get/set) in ES5 output.
/// Validates that private getters and setters generate proper source mappings.
#[test]
fn test_source_map_private_accessor_es5() {
    let source = r#"class Temperature {
    #celsius = 0;

    get #fahrenheit(): number {
        return this.#celsius * 9/5 + 32;
    }

    set #fahrenheit(value: number) {
        this.#celsius = (value - 32) * 5/9;
    }

    setFahrenheit(f: number) {
        this.#fahrenheit = f;
    }

    getFahrenheit(): number {
        return this.#fahrenheit;
    }
}

const temp = new Temperature();
temp.setFahrenheit(98.6);
console.log(temp.getFahrenheit());"#;

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
        output.contains("Temperature"),
        "expected Temperature class in output. output: {output}"
    );
    assert!(
        output.contains("setFahrenheit"),
        "expected setFahrenheit method in output. output: {output}"
    );
    assert!(
        output.contains("getFahrenheit"),
        "expected getFahrenheit method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private accessor"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for private static members in ES5 output.
/// Validates that static private fields and methods generate proper source mappings.
#[test]
fn test_source_map_private_static_members_es5() {
    let source = r#"class IdGenerator {
    static #nextId = 1;

    static #generateId(): number {
        return IdGenerator.#nextId++;
    }

    static create(): number {
        return IdGenerator.#generateId();
    }

    static reset(): void {
        IdGenerator.#nextId = 1;
    }
}

console.log(IdGenerator.create());
console.log(IdGenerator.create());
IdGenerator.reset();"#;

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
        output.contains("IdGenerator"),
        "expected IdGenerator class in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create static method in output. output: {output}"
    );
    assert!(
        output.contains("reset"),
        "expected reset static method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private static members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Comprehensive test combining multiple private class feature patterns.
/// Tests private fields, methods, accessors, and static members together.
#[test]
fn test_source_map_private_features_es5_comprehensive() {
    let source = r#"class BankAccount {
    static #accountCount = 0;
    #balance = 0;
    #transactions: string[] = [];

    #log(message: string): void {
        this.#transactions.push(message);
    }

    get #formattedBalance(): string {
        return `$${this.#balance.toFixed(2)}`;
    }

    static #validateAmount(amount: number): boolean {
        return amount > 0 && isFinite(amount);
    }

    constructor(initialBalance: number) {
        if (BankAccount.#validateAmount(initialBalance)) {
            this.#balance = initialBalance;
            this.#log(`Account opened with ${this.#formattedBalance}`);
        }
        BankAccount.#accountCount++;
    }

    deposit(amount: number): boolean {
        if (BankAccount.#validateAmount(amount)) {
            this.#balance += amount;
            this.#log(`Deposited: $${amount}`);
            return true;
        }
        return false;
    }

    withdraw(amount: number): boolean {
        if (BankAccount.#validateAmount(amount) && amount <= this.#balance) {
            this.#balance -= amount;
            this.#log(`Withdrew: $${amount}`);
            return true;
        }
        return false;
    }

    getBalance(): string {
        return this.#formattedBalance;
    }

    getHistory(): string[] {
        return [...this.#transactions];
    }

    static getAccountCount(): number {
        return BankAccount.#accountCount;
    }
}

const account = new BankAccount(100);
account.deposit(50);
account.withdraw(25);
console.log(account.getBalance());
console.log(account.getHistory());
console.log(BankAccount.getAccountCount());"#;

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
        output.contains("BankAccount"),
        "expected BankAccount class in output. output: {output}"
    );
    assert!(
        output.contains("deposit"),
        "expected deposit method in output. output: {output}"
    );
    assert!(
        output.contains("withdraw"),
        "expected withdraw method in output. output: {output}"
    );
    assert!(
        output.contains("getBalance"),
        "expected getBalance method in output. output: {output}"
    );
    assert!(
        output.contains("getHistory"),
        "expected getHistory method in output. output: {output}"
    );
    assert!(
        output.contains("getAccountCount"),
        "expected getAccountCount static method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive private features"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// ES5 SOURCE MAP TESTS: TYPE PARAMETER CONSTRAINTS
// =============================================================================

/// Test source map generation for generic function with type parameter constraint in ES5 output.
/// Validates that generic functions with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_function_es5() {
    let source = r#"interface HasLength {
    length: number;
}

function getLength<T extends HasLength>(item: T): number {
    return item.length;
}

function first<T extends any[]>(arr: T): T[0] {
    return arr[0];
}

const strLen = getLength("hello");
const arrLen = getLength([1, 2, 3]);
const firstItem = first([1, 2, 3]);"#;

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
        output.contains("getLength"),
        "expected getLength function in output. output: {output}"
    );
    assert!(
        output.contains("first"),
        "expected first function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic function with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for generic class with type parameter constraint in ES5 output.
/// Validates that generic classes with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_class_es5() {
    let source = r#"interface Comparable<T> {
    compareTo(other: T): number;
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

    getAll(): T[] {
        return [...this.items];
    }
}

class NumberWrapper implements Comparable<NumberWrapper> {
    constructor(public value: number) {}

    compareTo(other: NumberWrapper): number {
        return this.value - other.value;
    }
}

const list = new SortedList<NumberWrapper>();
list.add(new NumberWrapper(5));
list.add(new NumberWrapper(2));"#;

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
        output.contains("SortedList"),
        "expected SortedList class in output. output: {output}"
    );
    assert!(
        output.contains("NumberWrapper"),
        "expected NumberWrapper class in output. output: {output}"
    );
    assert!(
        output.contains("compareTo"),
        "expected compareTo method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic class with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for generic interface with type parameter constraint in ES5 output.
/// Validates that generic interfaces with extends constraints generate proper source mappings.
#[test]
fn test_source_map_type_constraint_generic_interface_es5() {
    let source = r#"interface Entity {
    id: number;
    createdAt: Date;
}

interface Repository<T extends Entity> {
    findById(id: number): T | undefined;
    findAll(): T[];
    save(entity: T): T;
    delete(id: number): boolean;
}

interface User extends Entity {
    name: string;
    email: string;
}

class UserRepository implements Repository<User> {
    private users: User[] = [];

    findById(id: number): User | undefined {
        return this.users.find(u => u.id === id);
    }

    findAll(): User[] {
        return [...this.users];
    }

    save(user: User): User {
        this.users.push(user);
        return user;
    }

    delete(id: number): boolean {
        const index = this.users.findIndex(u => u.id === id);
        if (index >= 0) {
            this.users.splice(index, 1);
            return true;
        }
        return false;
    }
}

const repo = new UserRepository();"#;

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
        output.contains("UserRepository"),
        "expected UserRepository class in output. output: {output}"
    );
    assert!(
        output.contains("findById"),
        "expected findById method in output. output: {output}"
    );
    assert!(
        output.contains("findAll"),
        "expected findAll method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic interface with constraint"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for multiple type parameters with constraints in ES5 output.
/// Validates that functions with multiple constrained type parameters generate proper source mappings.
#[test]
fn test_source_map_type_constraint_multiple_params_es5() {
    let source = r#"interface Serializable {
    serialize(): string;
}

interface Deserializable<T> {
    deserialize(data: string): T;
}

function transform<
    TInput extends Serializable,
    TOutput,
    TTransformer extends { transform(input: TInput): TOutput }
>(input: TInput, transformer: TTransformer): TOutput {
    return transformer.transform(input);
}

function merge<T extends object, U extends object>(first: T, second: U): T & U {
    return { ...first, ...second };
}

function pick<T extends object, K extends keyof T>(obj: T, keys: K[]): Pick<T, K> {
    const result = {} as Pick<T, K>;
    for (const key of keys) {
        result[key] = obj[key];
    }
    return result;
}

const merged = merge({ a: 1 }, { b: 2 });
const picked = pick({ x: 1, y: 2, z: 3 }, ["x", "z"]);"#;

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
        output.contains("transform"),
        "expected transform function in output. output: {output}"
    );
    assert!(
        output.contains("merge"),
        "expected merge function in output. output: {output}"
    );
    assert!(
        output.contains("pick"),
        "expected pick function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple type parameter constraints"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

