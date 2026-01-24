// Test typeof operator behavior
// This file tests various typeof scenarios to identify TS2322 issues

// Test 1: typeof on value - should capture the type
const num = 42;
type T1 = typeof num; // should be number (literal 42 in const context)

let num2 = 42;
type T2 = typeof num2; // should be number

// Test 2: typeof on function
function foo() {
  return "hello";
}
type T3 = typeof foo; // should be () => string

// Test 3: typeof on class
class MyClass {
  prop: string;
  constructor(p: string) {
    this.prop = p;
  }
  method() {
    return this.prop;
  }
}
type T4 = typeof MyClass; // should typeof MyClass - the constructor type
const instance: MyClass = new MyClass("test"); // OK
const ctor: typeof MyClass = MyClass; // OK

// Test 4: typeof on wrong symbol (should error)
type T5 = typeof NonExistent; // Should error - TS2304 or TS2318

// Test 5: typeof in type positions
type T6 = typeof Math.random; // Should be () => number

// Test 6: keyof typeof
const obj = { a: 1, b: 2 };
type T7 = keyof typeof obj; // should be "a" | "b"

// Test 7: typeof on imported types
import { Test } from './test_missing_ts2322_calls';
type T8 = typeof Test;

// Test 8: typeof assignability - should error when wrong
let x: string;
x = 42; // TS2322

type T9 = typeof x; // string
let y: T9;
y = 42; // TS2322

// Test 9: typeof on undefined vs null
let undef: undefined;
type T10 = typeof undef; // undefined

let nul: null = null;
type T11 = typeof nul; // null

// Test 10: typeof on class instance vs class itself
class A {
  method() {}
}
type T12 = typeof A; // constructor type
type T13 = InstanceType<typeof A>; // instance type
const a: T13 = new A(); // OK
const a2: T12 = A; // OK

// Test 11: typeof with generics
function identity<T>(x: T): T {
  return x;
}
type T14 = typeof identity; // <T>(x: T) => T

// Test 12: typeof on object literal
const obj2 = { name: "test", age: 42 };
type T15 = typeof obj2; // { name: string; age: number; }

// Test 13: typeof on array
const arr = [1, 2, 3];
type T16 = typeof arr; // number[]

// Test 14: typeof on interface (should error - interfaces are types, not values)
// interface I {
//   prop: string;
// }
// type T17 = typeof I; // Should error - TS2318 (type-only value)

// Test 15: typeof on namespace
namespace NS {
  export const value = 42;
}
type T18 = typeof NS; // Should work

// Test 16: typeof on enum
enum E {
  A,
  B
}
type T19 = typeof E; // Should work

// Test 17: typeof in conditional types
type T20 = typeof 42 extends number ? "yes" : "no"; // "yes"

// Test 18: typeof on complex expressions
const add = (a: number, b: number) => a + b;
type T21 = typeof add; // (a: number, b: number) => number

// Test 19: typeof with access modifiers
class B {
  private priv = "private";
  protected prot = "protected";
  public pub = "public";
  readonly ro = "readonly";
}
type T22 = typeof B;

// Test 20: typeof typeof (double typeof) - should error
// type T23 = typeof typeof num; // Should error

// Test 21: typeof in mapped types
const obj3 = { x: 1, y: 2 };
type T24 = { [K in keyof typeof obj3]: typeof obj3[K] }; // Should be { x: number; y: number; }

// Test 22: typeof with accessibility issues
class C {
  private method() {}
}
type T25 = typeof C;
const c: C = new C(); // OK
// c.method(); // Should error - private

// Test 23: typeof on generic class
class GenericClass<T> {
  value: T;
  constructor(v: T) {
    this.value = v;
  }
}
type T26 = typeof GenericClass; // typeof GenericClass
const gc: typeof GenericClass<number> = GenericClass; // Should work
const gc2: GenericClass<number> = new GenericClass(42); // OK

// Test 24: typeof on static members
class D {
  static staticProp = "static";
  instanceProp = "instance";
}
type T27 = typeof D;
D.staticProp; // OK

// Test 25: typeof errors - wrong types
type T28 = typeof Math.random; // () => number
const wrong: T28 = 42 as any; // no error with any
const wrong2: T28 = (() => "hello") as any; // any bypasses

// Test 26: typeof on const assertion
const obj4 = { x: 1 } as const;
type T29 = typeof obj4; // { readonly x: 1; }

// Test 27: keyof typeof combinations
const config = {
  apiUrl: "https://api.example.com",
  timeout: 5000,
};
type ConfigKey = keyof typeof config; // "apiUrl" | "timeout"
function getConfig(key: ConfigKey) {
  return config[key];
}
