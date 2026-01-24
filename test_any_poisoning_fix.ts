// Test to verify "Any poisoning" is fixed
// When global symbols like console, Array, Promise are resolved,
// they should have their actual types, not just "any"

// Test 1: console.log should have proper type checking
console.log("hello");  // Should work
console.log(123);      // Should work

// Before fix: These would all pass because console is "any"
// After fix: Type errors should be caught
const x: string = console.log(123);  // Should error: Type 'void' is not assignable to type 'string'
const y: number = console.log("test");  // Should error: Type 'void' is not assignable to type 'number'

// Test 2: Array should have proper type checking
const arr: Array<number> = [1, 2, 3];  // Should work
const arr2: Array<string> = [1, 2, 3];  // Should error: Type 'number' is not assignable to type 'string'

// Before fix: This would pass because Array is "any"
// After fix: Should error
const arr3: string = new Array<number>();  // Should error: Type 'number[]' is not assignable to type 'string'

// Test 3: Promise should have proper type checking
Promise.resolve(42);  // Should work
Promise.resolve("test");  // Should work

// Before fix: This would pass because Promise is "any"
// After fix: Should error with type mismatch
const p: Promise<string> = Promise.resolve(123);  // Should error: Type 'Promise<number>' is not assignable to type 'Promise<string>'

// Test 4: Math functions should have proper types
Math.abs(-5);  // Should work
const m: string = Math.abs(-5);  // Should error: Type 'number' is not assignable to type 'string'

// Test 5: Object constructor should have proper type
Object.create(null);  // Should work
const obj: number = Object.create(null);  // Should error: Type 'object' is not assignable to type 'number'

// Test 6: String constructor
String("hello");  // Should work
const s: number = String("hello");  // Should error: Type 'string' is not assignable to type 'number'

// Test 7: JSON methods
JSON.parse('{"key": "value"}');  // Should work
const json: number = JSON.parse('{"key": "value"}');  // Should error: Type 'any' is not assignable to type 'number' (with noImplicitAny)

// Test 8: Undefined global should emit TS2304
console.log(undefinedGlobal);  // Should error: Cannot find name 'undefinedGlobal'

console.log("All tests completed - check for proper error emission");
