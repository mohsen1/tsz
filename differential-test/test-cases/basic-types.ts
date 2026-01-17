// Basic Type Tests - Should all pass with no errors

// Primitive types
const num: number = 42;
const str: string = "hello";
const bool: boolean = true;

// Any type
const anyVal: any = "test";
const anyToNum: number = anyVal;

// Object literal types
const point: { x: number } = { x: 1 };
const coords: { x: number; y: number } = { x: 1, y: 2 };

// Arrays
const nums: number[] = [1, 2, 3];
const strs: string[] = ["a", "b"];

// Functions
function add(a: number, b: number): number {
  return a + b;
}

// Arrow functions
const multiply = (x: number, y: number): number => x * y;

// Void return
function logIt(msg: string): void {
  console.log(msg);
}
