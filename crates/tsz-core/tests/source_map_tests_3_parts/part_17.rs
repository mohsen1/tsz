#[test]
fn test_source_map_generator_es5_state_machine() {
    let source = r#"type State = 'idle' | 'loading' | 'success' | 'error';

function* stateMachine(): Generator<State, void, string> {
    let input: string;

    while (true) {
        yield 'idle';
        input = yield 'loading';

        if (input === 'success') {
            yield 'success';
        } else if (input === 'error') {
            yield 'error';
        }
    }
}

const machine = stateMachine();
console.log(machine.next());
console.log(machine.next());
console.log(machine.next('success'));"#;

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
        output.contains("stateMachine"),
        "expected output to contain stateMachine. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for state machine generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_finally() {
    let source = r#"function* generatorWithFinally(): Generator<number> {
    try {
        yield 1;
        yield 2;
        yield 3;
    } finally {
        console.log('Generator cleanup');
    }
}

function* nestedTryFinally(): Generator<string> {
    try {
        try {
            yield 'inner-1';
            yield 'inner-2';
        } finally {
            yield 'inner-finally';
        }
        yield 'outer-1';
    } finally {
        yield 'outer-finally';
    }
}

const gen1 = generatorWithFinally();
const gen2 = nestedTryFinally();
console.log([...gen1], [...gen2]);"#;

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
        output.contains("generatorWithFinally") || output.contains("nestedTryFinally"),
        "expected output to contain generator functions. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for finally generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_composition() {
    let source = r#"function* numbers(n: number): Generator<number> {
    for (let i = 1; i <= n; i++) {
        yield i;
    }
}

function* letters(s: string): Generator<string> {
    for (const c of s) {
        yield c;
    }
}

function* combined(): Generator<number | string> {
    yield* numbers(3);
    yield '---';
    yield* letters('abc');
    yield '---';
    yield* numbers(2);
}

function* flatten<T>(iterables: Iterable<T>[]): Generator<T> {
    for (const iterable of iterables) {
        yield* iterable;
    }
}

console.log([...combined()]);
console.log([...flatten([[1, 2], [3, 4], [5]])]);"#;

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
        output.contains("combined") || output.contains("flatten"),
        "expected output to contain composition generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for composition generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_iterator_protocol() {
    let source = r#"class Range {
    constructor(private start: number, private end: number) {}

    *[Symbol.iterator](): Generator<number> {
        for (let i = this.start; i <= this.end; i++) {
            yield i;
        }
    }
}

class KeyValuePairs<K, V> {
    private pairs: [K, V][] = [];

    add(key: K, value: V): void {
        this.pairs.push([key, value]);
    }

    *keys(): Generator<K> {
        for (const [key] of this.pairs) {
            yield key;
        }
    }

    *values(): Generator<V> {
        for (const [, value] of this.pairs) {
            yield value;
        }
    }

    *entries(): Generator<[K, V]> {
        yield* this.pairs;
    }
}

const range = new Range(1, 5);
console.log([...range]);"#;

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
        output.contains("Range") || output.contains("KeyValuePairs"),
        "expected output to contain iterator protocol classes. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for iterator protocol"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_default_params() {
    let source = r#"function* range(
    start: number = 0,
    end: number = 10,
    step: number = 1
): Generator<number> {
    for (let i = start; i < end; i += step) {
        yield i;
    }
}

function* repeat<T>(
    value: T,
    times: number = Infinity
): Generator<T> {
    for (let i = 0; i < times; i++) {
        yield value;
    }
}

function* take<T>(
    iterable: Iterable<T>,
    count: number = 5
): Generator<T> {
    let i = 0;
    for (const item of iterable) {
        if (i++ >= count) break;
        yield item;
    }
}

console.log([...range()]);
console.log([...range(5, 10, 2)]);
console.log([...take(repeat('x'), 3)]);"#;

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
        output.contains("range") || output.contains("repeat"),
        "expected output to contain generators with default params. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default params generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_object_yielding() {
    let source = r#"interface Person {
    id: number;
    name: string;
    age: number;
}

function* personGenerator(): Generator<Person> {
    yield { id: 1, name: 'Alice', age: 30 };
    yield { id: 2, name: 'Bob', age: 25 };
    yield { id: 3, name: 'Charlie', age: 35 };
}

function* objectTransformer<T, U>(
    source: Iterable<T>,
    transform: (item: T) => U
): Generator<U> {
    for (const item of source) {
        yield transform(item);
    }
}

const people = personGenerator();
const names = objectTransformer(personGenerator(), p => p.name);
console.log([...people], [...names]);"#;

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
        output.contains("personGenerator") || output.contains("objectTransformer"),
        "expected output to contain object yielding generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object yielding generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_recursion() {
    let source = r#"interface TreeNode<T> {
    value: T;
    children?: TreeNode<T>[];
}

function* traverseTree<T>(node: TreeNode<T>): Generator<T> {
    yield node.value;
    if (node.children) {
        for (const child of node.children) {
            yield* traverseTree(child);
        }
    }
}

function* fibonacci(): Generator<number> {
    let [a, b] = [0, 1];
    while (true) {
        yield a;
        [a, b] = [b, a + b];
    }
}

function* permutations<T>(items: T[]): Generator<T[]> {
    if (items.length <= 1) {
        yield items;
    } else {
        for (let i = 0; i < items.length; i++) {
            const rest = [...items.slice(0, i), ...items.slice(i + 1)];
            for (const perm of permutations(rest)) {
                yield [items[i], ...perm];
            }
        }
    }
}

const tree: TreeNode<number> = {
    value: 1,
    children: [{ value: 2 }, { value: 3, children: [{ value: 4 }] }]
};
console.log([...traverseTree(tree)]);"#;

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
        output.contains("traverseTree") || output.contains("fibonacci"),
        "expected output to contain recursive generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for recursive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_lazy_evaluation() {
    let source = r#"function* lazyMap<T, U>(
    source: Iterable<T>,
    fn: (item: T) => U
): Generator<U> {
    for (const item of source) {
        yield fn(item);
    }
}

function* lazyFilter<T>(
    source: Iterable<T>,
    predicate: (item: T) => boolean
): Generator<T> {
    for (const item of source) {
        if (predicate(item)) {
            yield item;
        }
    }
}

function* lazyTakeWhile<T>(
    source: Iterable<T>,
    predicate: (item: T) => boolean
): Generator<T> {
    for (const item of source) {
        if (!predicate(item)) break;
        yield item;
    }
}

function pipe<T>(...generators: ((input: Iterable<T>) => Generator<T>)[]): (input: Iterable<T>) => Generator<T> {
    return function*(input: Iterable<T>): Generator<T> {
        let result: Iterable<T> = input;
        for (const gen of generators) {
            result = gen(result);
        }
        yield* result;
    };
}

const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
const result = lazyFilter(
    lazyMap(numbers, x => x * 2),
    x => x > 5
);
console.log([...result]);"#;

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
        output.contains("lazyMap") || output.contains("lazyFilter"),
        "expected output to contain lazy evaluation generators. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for lazy evaluation generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_es5_comprehensive() {
    let source = r#"// Utility generators
function* range(start: number, end: number): Generator<number> {
    for (let i = start; i < end; i++) yield i;
}

function* map<T, U>(iter: Iterable<T>, fn: (x: T) => U): Generator<U> {
    for (const x of iter) yield fn(x);
}

function* filter<T>(iter: Iterable<T>, pred: (x: T) => boolean): Generator<T> {
    for (const x of iter) if (pred(x)) yield x;
}

// Class with generator methods
class DataStream<T> {
    private data: T[] = [];

    push(...items: T[]): void {
        this.data.push(...items);
    }

    *[Symbol.iterator](): Generator<T> {
        yield* this.data;
    }

    *reversed(): Generator<T> {
        for (let i = this.data.length - 1; i >= 0; i--) {
            yield this.data[i];
        }
    }

    *chunks(size: number): Generator<T[]> {
        for (let i = 0; i < this.data.length; i += size) {
            yield this.data.slice(i, i + size);
        }
    }

    *zip<U>(other: Iterable<U>): Generator<[T, U]> {
        const otherIter = other[Symbol.iterator]();
        for (const item of this.data) {
            const otherResult = otherIter.next();
            if (otherResult.done) break;
            yield [item, otherResult.value];
        }
    }
}

// Async generator for completeness
async function* asyncNumbers(): AsyncGenerator<number> {
    for (let i = 0; i < 5; i++) {
        await new Promise(r => setTimeout(r, 10));
        yield i;
    }
}

// Usage
const stream = new DataStream<number>();
stream.push(1, 2, 3, 4, 5, 6);

const evenDoubled = filter(
    map(stream, x => x * 2),
    x => x % 4 === 0
);

console.log([...evenDoubled]);
console.log([...stream.chunks(2)]);
console.log([...stream.zip(['a', 'b', 'c'])]);"#;

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
        output.contains("DataStream"),
        "expected output to contain DataStream class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive generator"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Inheritance ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_inheritance_es5_extends_clause() {
    let source = r#"class Animal {
    name: string;

    constructor(name: string) {
        this.name = name;
    }

    speak(): void {
        console.log(`${this.name} makes a sound`);
    }
}

class Dog extends Animal {
    breed: string;

    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
}

const dog = new Dog("Buddy", "Labrador");"#;

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
        output.contains("Animal"),
        "expected output to contain Animal class. output: {output}"
    );
    assert!(
        output.contains("Dog"),
        "expected output to contain Dog class. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for extends clause"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

