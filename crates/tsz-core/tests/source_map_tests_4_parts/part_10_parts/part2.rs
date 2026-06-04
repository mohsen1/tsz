/// Comprehensive test combining multiple mapped type patterns.
/// Tests Partial, Required, Readonly, Pick, Record, and custom mapped types together.
#[test]
fn test_source_map_mapped_type_es5_comprehensive() {
    let source = r#"// Comprehensive mapped type utility library

// Standard mapped types
type Partial<T> = { [P in keyof T]?: T[P] };
type Required<T> = { [P in keyof T]-?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Record<K extends keyof any, T> = { [P in K]: T };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

// Custom mapped types
type Mutable<T> = { -readonly [P in keyof T]: T[P] };
type Nullable<T> = { [P in keyof T]: T[P] | null };
type NonNullableProps<T> = { [P in keyof T]: NonNullable<T[P]> };

// Key remapping
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

type Setters<T> = {
    [K in keyof T as `set${Capitalize<string & K>}`]: (value: T[K]) => void;
};

// Entity interface
interface Entity {
    id: number;
    name: string;
    createdAt: Date;
    updatedAt: Date | null;
}

// Repository using mapped types
class Repository<T extends Entity> {
    private items: Map<number, T> = new Map();

    create(data: Omit<T, "id" | "createdAt" | "updatedAt">): T {
        const now = new Date();
        const id = this.items.size + 1;
        const entity = {
            ...data,
            id,
            createdAt: now,
            updatedAt: null
        } as T;
        this.items.set(id, entity);
        return entity;
    }

    update(id: number, data: Partial<Omit<T, "id" | "createdAt">>): T | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const updated = { ...entity, ...data, updatedAt: new Date() };
            this.items.set(id, updated);
            return updated;
        }
        return undefined;
    }

    findById(id: number): Readonly<T> | undefined {
        return this.items.get(id);
    }

    findAll(): ReadonlyArray<Readonly<T>> {
        return Array.from(this.items.values());
    }

    getFields<K extends keyof T>(id: number, fields: K[]): Pick<T, K> | undefined {
        const entity = this.items.get(id);
        if (entity) {
            const result = {} as Pick<T, K>;
            for (const field of fields) {
                result[field] = entity[field];
            }
            return result;
        }
        return undefined;
    }
}

// Form state using mapped types
type FormState<T> = {
    values: T;
    errors: Partial<Record<keyof T, string>>;
    touched: Partial<Record<keyof T, boolean>>;
    dirty: boolean;
};

function createFormState<T>(initial: T): FormState<T> {
    return {
        values: initial,
        errors: {},
        touched: {},
        dirty: false
    };
}

interface User extends Entity {
    email: string;
    role: "admin" | "user";
}

const userRepo = new Repository<User>();
const newUser = userRepo.create({ name: "Alice", email: "alice@example.com", role: "user" });
const formState = createFormState({ name: "", email: "" });
console.log(newUser, formState);"#;

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
        output.contains("Repository"),
        "expected Repository class in output. output: {output}"
    );
    assert!(
        output.contains("create"),
        "expected create method in output. output: {output}"
    );
    assert!(
        output.contains("update"),
        "expected update method in output. output: {output}"
    );
    assert!(
        output.contains("findById"),
        "expected findById method in output. output: {output}"
    );
    assert!(
        output.contains("getFields"),
        "expected getFields method in output. output: {output}"
    );
    assert!(
        output.contains("createFormState"),
        "expected createFormState function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive mapped types"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

/// Test source map generation for `ReturnType`<T> utility type in ES5 output.
/// Validates that `ReturnType` extraction generates proper source mappings.
#[test]
fn test_source_map_utility_type_return_type_es5() {
    let source = r#"// Custom ReturnType implementation
type MyReturnType<T extends (...args: any[]) => any> = T extends (...args: any[]) => infer R ? R : never;

// Functions to extract return types from
function getString(): string {
    return "hello";
}

function getNumber(): number {
    return 42;
}

async function getAsyncData(): Promise<{ id: number; name: string }> {
    return { id: 1, name: "test" };
}

function getCallback(): (x: number) => boolean {
    return (x) => x > 0;
}

// Using ReturnType
type StringResult = ReturnType<typeof getString>;
type NumberResult = ReturnType<typeof getNumber>;
type AsyncResult = ReturnType<typeof getAsyncData>;
type CallbackResult = MyReturnType<typeof getCallback>;

// Functions that use extracted types
function processString(value: StringResult): void {
    console.log(value.toUpperCase());
}

function processNumber(value: NumberResult): void {
    console.log(value.toFixed(2));
}

// Generic wrapper using ReturnType
function wrapResult<T extends (...args: any[]) => any>(
    fn: T
): { result: ReturnType<T>; timestamp: Date } | null {
    try {
        return { result: fn(), timestamp: new Date() };
    } catch {
        return null;
    }
}

const wrapped = wrapResult(getString);
processString("test");
processNumber(123);"#;

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
        output.contains("getString"),
        "expected getString function in output. output: {output}"
    );
    assert!(
        output.contains("wrapResult"),
        "expected wrapResult function in output. output: {output}"
    );
    assert!(
        output.contains("processString"),
        "expected processString function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ReturnType utility"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
