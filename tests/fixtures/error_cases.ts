// Error case test fixtures - these should produce type errors

// === Type mismatch errors ===
const num: number = "string";  // TS2322

function takesNumber(x: number): void {}
takesNumber("string");  // TS2345

// === Property errors ===
interface Point { x: number; y: number; }
const p: Point = { x: 1 };  // TS2741 - missing property y

const q: Point = { x: 1, y: 2, z: 3 };  // TS2353 - excess property

// === Null/undefined errors ===
const maybeNum: number | null = null;
const definiteNum: number = maybeNum;  // TS2322 - needs null check

function strictFunction(x: string): void {}
strictFunction(undefined);  // TS2345

// === Function return type errors ===
function shouldReturnNumber(): number {
    return "string";  // TS2322
}

function missingReturn(): number {
    // TS2355 - not all paths return a value
    if (Math.random() > 0.5) {
        return 1;
    }
}

// === Class errors ===
class Base {
    protected value: number = 0;
}

class Derived extends Base {
    getValue(): number {
        return this.value;
    }
}

const d = new Derived();
d.value;  // TS2445 - protected member

// === Generic constraint errors ===
function requiresLength<T extends { length: number }>(x: T): number {
    return x.length;
}
requiresLength(42);  // TS2345 - number has no length

// === Index signature errors ===
interface StringMap { [key: string]: string; }
const map: StringMap = { key: 123 };  // TS2322

// === Readonly errors ===
interface ReadonlyPoint { readonly x: number; readonly y: number; }
const rp: ReadonlyPoint = { x: 1, y: 2 };
rp.x = 3;  // TS2540

// === Tuple errors ===
const tuple: [number, string] = [1, 2];  // TS2322

// === Enum errors ===
enum Color { Red, Green, Blue }
const color: Color = 999;  // Might not error in all configs

// === Module errors ===
// import { nonExistent } from './somewhere';  // TS2305

// === Never type errors ===
function neverReturns(): never {
    return;  // TS2534
}

// === Exhaustiveness check ===
type Status = 'pending' | 'approved' | 'rejected';
function handleStatus(status: Status): string {
    switch (status) {
        case 'pending':
            return 'Waiting';
        case 'approved':
            return 'Done';
        // Missing 'rejected' case - should warn
    }
    // TS2366 - not all paths return
}

// === Async errors ===
async function asyncFunc(): number {  // TS1064 - should be Promise<number>
    return 42;
}

// === Decorator errors (when not enabled) ===
// @decorator  // TS1206 - decorators experimental
// class DecoratedClass {}

// === Conflicting declarations ===
let conflicting = 1;
let conflicting = 2;  // TS2451 - duplicate declaration

// === Type only imports misuse ===
// import type { SomeClass } from './module';
// new SomeClass();  // TS1361 - cannot use type-only import as value

// === Incorrect this context ===
class Counter {
    count = 0;
    increment() {
        this.count++;
    }
}
const counter = new Counter();
const inc = counter.increment;
inc();  // TS2683 - this is undefined at runtime (strict mode)

// === Unused variables (with strict settings) ===
const unused = 42;  // TS6133 - declared but never used
