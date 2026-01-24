// Simple typeof test cases

// Test 1: typeof on value - should capture the type
const num = 42;
type T1 = typeof num; // should be number (literal 42 in const context)

// Test 2: typeof on function
function foo() {
  return "hello";
}
type T2 = typeof foo; // should be () => string

// Test 3: typeof on class
class MyClass {
  prop: string;
  constructor(p: string) {
    this.prop = p;
  }
}
type T3 = typeof MyClass; // typeof MyClass - the constructor type

// Test 4: keyof typeof combinations
const obj = { a: 1, b: 2 };
type T4 = keyof typeof obj; // should be "a" | "b"

// Test 5: typeof assignability
const x: string = "hello";
const y: typeof x = "world"; // OK - both are string
const z: typeof x = 42; // TS2322 - number not assignable to string

// Test 6: typeof on array
const arr = [1, 2, 3];
type T5 = typeof arr; // number[]
