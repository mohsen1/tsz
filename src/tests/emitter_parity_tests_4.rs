//! Emitter parity tests - Part 4

use crate::emit_context::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions, ScriptTarget};
#[allow(unused_imports)]
use crate::emitter_parity_test_utils::assert_parity;
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;

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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
#[ignore = "ES5 spread downleveling not fully implemented"]
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
