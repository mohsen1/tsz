// Test file for excess property checking
// This file tests that object literals are only checked for excess properties when "fresh"

type Point = { x: number; y: number };

// Test 1: Fresh object literal - SHOULD error
const p1: Point = { x: 1, y: 2, z: 3 }; // Error: 'z' is excess

// Test 2: Non-fresh object - should NOT error
const obj = { x: 1, y: 2, z: 3 };
const p2: Point = obj; // No error - obj is not fresh

// Test 3: Fresh object literal in function call - SHOULD error
function foo(p: Point) {}
foo({ x: 1, y: 2, z: 3 }); // Error: 'z' is excess

// Test 4: Non-fresh object in function call - should NOT error
const obj2 = { x: 1, y: 2, z: 3 };
foo(obj2); // No error - obj2 is not fresh

// Test 5: Correct object literal - should NOT error
const p3: Point = { x: 1, y: 2 }; // No error
