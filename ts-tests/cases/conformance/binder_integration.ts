/// <reference no-default-lib="true"/>
/// <reference lib="es2015.full" />

// Binder Integration Test
// Tests full binder pipeline: lib loading -> binding -> symbol resolution
// Verifies all global symbols resolve correctly (TS2304 errors < 50)

// Test core global types - these should all resolve without TS2304
const obj: Object = new Object();
const func: Function = function() {};
const arr: Array<number> = [1, 2, 3];

// Test global functions
const num = Number("42");
const str = String(42);
const bool = Boolean(1);

// Test Math object
const pi = Math.PI;
const max = Math.max(1, 2, 3);

// Test Date
const now = new Date();
const timestamp = Date.now();

// Test JSON
const json = JSON.stringify({ foo: "bar" });
const parsed = JSON.parse(json);

// Test Error types
const err = new Error("test");
const typeErr = new TypeError("type error");
const rangeErr = new RangeError("range error");

// Test Promise
const promise = new Promise<number>((resolve, reject) => {
  resolve(42);
});

// Test console - should resolve without TS2304
console.log("hello world");
console.error("error");
console.warn("warning");

// Test Symbol
const sym = Symbol("test");

// Test Map and Set (ES2015)
const map = new Map<string, number>();
map.set("key", 42);
const set = new Set<number>();
set.add(1);

// Test RegExp
const regex = /test/g;
const regexMatches = regex.test("test");

// Test Array methods
arr.forEach(x => console.log(x));
const mapped = arr.map(x => x * 2);
const filtered = arr.filter(x => x > 1);

// Test Object methods
Object.keys({ foo: 1 });
Object.values({ foo: 1 });
Object.entries({ foo: 1 });

// Test typeof checks
if (typeof obj === "object") {
  console.log("is object");
}

// Test instanceof
if (err instanceof Error) {
  console.log("is error");
}

// Test template strings
const template = `value: ${num}`;

// Test destructuring
const [first, second] = [1, 2];
const { foo, bar } = { foo: 1, bar: 2 };

// Test spread operator
const newArr = [...arr];
const newObj = { ...{ foo: 1 }, bar: 2 };

// Test async/await (uses Promise)
async function asyncFunc() {
  const result = await promise;
  return result;
}

// Test class (extends Object, uses global types)
class MyClass {
  constructor(public value: number) {}

  toString(): string {
    return String(this.value);
  }
}

// Test interface (uses global types)
interface IMyInterface {
  toJSON(): string;
}

// Test type aliases
type MyType = string | number;

// Test enums
enum Color {
  Red,
  Green,
  Blue,
}

// Test generic functions
function identity<T>(value: T): T {
  return value;
}

// Test optional parameters
function foo(a: number, b?: string): void {
  console.log(a, b);
}

// Test rest parameters
function bar(...args: number[]): void {
  args.forEach(x => console.log(x));
}

// Test default parameters
function baz(a: number = 42): void {
  console.log(a);
}

// Test symbol iterator (uses global Symbol)
const iterable = {
  [Symbol.iterator]() {
    let step = 0;
    return {
      next() {
        return {
          value: step++,
          done: step > 3,
        };
      },
    };
  },
};

// Test Proxy and Reflect (ES2015)
const target = { foo: 1 };
const proxy = new Proxy(target, {
  get(target, prop) {
    return Reflect.get(target, prop);
  },
});

// Test ArrayBuffer and typed arrays
const buffer = new ArrayBuffer(8);
const int8 = new Int8Array(buffer);
const uint8 = new Uint8Array(buffer);
const int16 = new Int16Array(buffer);
const uint16 = new Uint16Array(buffer);

// Test WeakMap and WeakSet
const weakMap = new WeakMap<object, number>();
const weakSet = new WeakSet<object>();

// Test global constants NaN, Infinity, undefined
const nan = NaN;
const inf = Infinity;
const undef = undefined;

// Test isNaN and isFinite
const isnanResult = isNaN(nan);
const isfiniteResult = isFinite(inf);

// Test encodeURI/decodeURI
const uri = encodeURI("http://example.com");
const decoded = decodeURI(uri);

// Test setTimeout/clearTimeout (Node/browser globals)
// @ts-ignore - setTimeout may not be available in all environments
setTimeout(() => {}, 100);

// Test Array.from and Array.of
const fromArray = Array.from([1, 2, 3]);
const ofArray = Array.of(1, 2, 3);

// Test Object.assign
const assigned = Object.assign({}, { foo: 1 }, { bar: 2 });

// Test Object.getOwnPropertyDescriptor
const descriptor = Object.getOwnPropertyDescriptor({ foo: 1 }, "foo");

// Test Object.defineProperty
const defined: { foo?: number } = {};
Object.defineProperty(defined, "foo", {
  value: 42,
  writable: true,
  enumerable: true,
  configurable: true,
});

// Test Function prototype methods
const bound = func.bind(null);
const applied = func.apply(null, []);
const called = func.call(null);

// Test String methods
const text = "hello world";
const upper = text.toUpperCase();
const lower = text.toLowerCase();
const trimmed = text.trim();
const split = text.split(" ");
const substring = text.substring(0, 5);

// Test Number methods
const epsilon = Number.EPSILON;
const maxSafe = Number.MAX_SAFE_INTEGER;
const minSafe = Number.MIN_SAFE_INTEGER;
const isIntegerResult = Number.isInteger(42);
const isSafeIntegerResult = Number.isSafeInteger(42);

// Test Math functions
const abs = Math.abs(-5);
const ceil = Math.ceil(4.2);
const floor = Math.floor(4.9);
const round = Math.round(4.5);
const sqrt = Math.sqrt(16);
const pow = Math.pow(2, 3);
const random = Math.random();

// Test assertion types (type, interface with const)
type Foo = { foo: string };
interface Bar {
  bar: number;
}
type BarInterface = Bar;

// Test conditional types (uses built-in types)
type Conditional<T> = T extends string ? "string" : "other";

// Test infer (uses built-in types)
type InferType<T> = T extends Promise<infer U> ? U : T;

// Test keyof with built-in types
type Keys = keyof Object;

// Test typeof with built-in types
type TypeOfObject = typeof Object;

// Test readonly array and tuple
type ReadonlyArray = ReadonlyArray<number>;
type Tuple = readonly [string, number];

// Test never and unknown types
const neverValue: never = (() => {
  throw new Error("never");
})();
const unknownValue: unknown = "unknown";

// Test any (built-in top type)
const anyValue: any = "any";

// Test void (built-in bottom type for functions)
function returnsVoid(): void {
  return;
}

// Test null and undefined types
const nullValue: null = null;
const undefinedValue: undefined = undefined;

// Test non-null assertion operator
const maybeString: string | null = null;
const definitelyString = maybeString!;

// Test optional chaining
const maybeObj: { foo?: { bar?: string } } | undefined = undefined;
const chained = maybeObj?.foo?.bar;

// Test nullish coalescing
const defaulted = maybeObj ?? { foo: { bar: "default" } };

// Test numeric separators
const million = 1_000_000;

// Test bigint
const bigIntValue = 9007199254740991n;
const bigIntAdd = bigIntValue + 1n;

// Test import.meta (ES2020)
// @ts-ignore - import.meta is always available but may not be typed
const metaUrl = import.meta.url;

// Test globalThis
const globalThisValue = globalThis;

// Final verification - all global symbols should be resolved
// If this file compiles with < 50 TS2304 errors, the binder integration is working
