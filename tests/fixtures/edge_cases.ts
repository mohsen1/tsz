// Edge case test fixtures for tsz compiler

// === Unicode and special characters ===
const unicode = "Hello, 世界! 🌍";
const emoji = "😀🎉✨";
const escaped = "line1\nline2\ttabbed\r\nwindows";

// === Number edge cases ===
const bigint = 9007199254740993n;
const hex = 0xDEADBEEF;
const binary = 0b10101010;
const octal = 0o755;
const scientific = 1.5e10;
const negative_exp = 1e-10;

// === Template literals ===
const name = "World";
const greeting = `Hello, ${name}!`;
const nested = `outer ${`inner ${1 + 2}`}`;
const multiline = `
  Line 1
  Line 2
  ${42}
`;

// === Complex generics ===
type DeepPartial<T> = T extends object ? {
    [P in keyof T]?: DeepPartial<T[P]>;
} : T;

type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

// === Conditional types ===
type IsArray<T> = T extends unknown[] ? true : false;
type ExtractArrayType<T> = T extends (infer U)[] ? U : never;

// === Mapped types ===
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Optional<T> = { [P in keyof T]?: T[P] };
type Mutable<T> = { -readonly [P in keyof T]: T[P] };

// === Complex function signatures ===
function overloaded(x: string): string;
function overloaded(x: number): number;
function overloaded(x: string | number): string | number {
    return x;
}

// === Rest parameters and spread ===
function rest(...args: number[]): number {
    return args.reduce((a, b) => a + b, 0);
}

const spread = [...[1, 2, 3], 4, 5];
const objSpread = { a: 1, ...{ b: 2, c: 3 } };

// === Destructuring ===
const [first, second, ...remaining] = [1, 2, 3, 4, 5];
const { x, y = 10, ...others } = { x: 1, y: 2, z: 3, w: 4 };

// === Optional chaining and nullish coalescing ===
interface Deep {
    a?: {
        b?: {
            c?: number;
        };
    };
}
const deep: Deep = {};
const value = deep?.a?.b?.c ?? 0;
const assign = deep?.a?.b?.c ??= 1;

// === Class with private fields ===
class PrivateClass {
    #privateField = 42;

    get publicValue(): number {
        return this.#privateField;
    }

    #privateMethod(): void {
        console.log(this.#privateField);
    }
}

// === Decorators (experimental) ===
function logged<T extends { new(...args: any[]): {} }>(constructor: T) {
    return class extends constructor {
        constructor(...args: any[]) {
            super(...args);
            console.log("Instance created");
        }
    };
}

// === Symbols ===
const uniqueSym: unique symbol = Symbol("unique");
const sym = Symbol("description");
const obj: { [sym]: number } = { [sym]: 42 };

// === Intersection types ===
type A = { a: number };
type B = { b: string };
type AB = A & B;

// === Discriminated unions ===
type Shape =
    | { kind: "circle"; radius: number }
    | { kind: "rectangle"; width: number; height: number }
    | { kind: "triangle"; base: number; height: number };

function area(shape: Shape): number {
    switch (shape.kind) {
        case "circle":
            return Math.PI * shape.radius ** 2;
        case "rectangle":
            return shape.width * shape.height;
        case "triangle":
            return (shape.base * shape.height) / 2;
    }
}

// === Tuple types ===
type Point = [number, number];
type LabeledPoint = [x: number, y: number];
type VariadicTuple = [string, ...number[], boolean];

// === Assert functions ===
function assertNonNull<T>(value: T): asserts value is NonNullable<T> {
    if (value === null || value === undefined) {
        throw new Error("Value is null or undefined");
    }
}

// === Type predicates ===
function isString(value: unknown): value is string {
    return typeof value === "string";
}

// === Const assertions ===
const constObject = { a: 1, b: 2 } as const;
const constArray = [1, 2, 3] as const;

// === Template literal types ===
type Color = "red" | "green" | "blue";
type BorderStyle = "solid" | "dashed";
type BorderProperty = `border-${Color}-${BorderStyle}`;

// === Recursive types ===
type JsonValue =
    | string
    | number
    | boolean
    | null
    | JsonValue[]
    | { [key: string]: JsonValue };

// === Namespace ===
namespace Validation {
    export interface StringValidator {
        isValid(s: string): boolean;
    }

    export class EmailValidator implements StringValidator {
        isValid(s: string): boolean {
            return s.includes("@");
        }
    }
}

// === Module augmentation ===
declare global {
    interface Array<T> {
        customMethod(): T[];
    }
}

// === Export types ===
export type { DeepPartial, Awaited, Shape };
export { area, isString, assertNonNull };
