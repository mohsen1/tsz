// Test cases to find missing TS2322 type assignability errors

// Test 1: Simple assignment - should work
let x: string = "hello";
let y: string = 42; // Should error: number not assignable to string

// Test 2: Property assignment
interface Person {
  name: string;
  age: number;
}
const p: Person = {
  name: "Alice",
  age: 30,
};
p.age = "30"; // Should error: string not assignable to number

// Test 3: Array element assignment
const arr: string[] = ["a", "b"];
arr[0] = 42; // Should error: number not assignable to string

// Test 4: Variable initialization with type annotation
const z: number = "string"; // Should error: string not assignable to number

// Test 5: Function return
function foo(): string {
  return 42; // Should error: number not assignable to string
}

// Test 6: Function argument
function bar(x: string) {
  console.log(x);
}
bar(42); // Should error: number not assignable to string

// Test 7: Destructuring
const { a, b }: { a: string; b: number } = { a: 1, b: "x" }; // Should error

// Test 8: Spread in object
const obj1 = { x: 1, y: 2 };
const obj2: { x: number; y: number } = { ...obj1, z: "string" }; // Should error for z

// Test 9: Null assignment in strict mode
let str: string = null; // Should error in strictNullChecks

// Test 10: Undefined assignment
let num: number = undefined; // Should error in strictNullChecks

console.log("TS2322 test complete");
