// Test case for freshness/excess property checking (Unsoundness Rule #4)
// Object literals should trigger excess property errors when fresh
// Once assigned to a variable, freshness is lost

// Test 1: Fresh object literal with excess property should error
const p1: Point = { x: 1, y: 2, z: 3 }; // Error: 'z' is excess

// Test 2: Non-fresh object should NOT error
const obj = { x: 1, y: 2, z: 3 }; // Freshness removed after assignment
const p2: Point = obj; // OK: obj is not fresh

// Test 3: Fresh object literal in function call should error
function expectPoint(p: Point): void {}
expectPoint({ x: 1, y: 2, z: 3 }); // Error: 'z' is excess

// Test 4: Non-fresh object in function call should NOT error
const obj2 = { x: 1, y: 2, z: 3 };
expectPoint(obj2); // OK: obj2 is not fresh

// Test 5: Object literal with matching properties should NOT error
const p3: Point = { x: 1, y: 2 }; // OK: no excess properties

// Test 6: Variable assignment should remove freshness
let p4: Point = { x: 1, y: 2, z: 3 }; // Error: 'z' is excess
p4 = { x: 3, y: 4, z: 5 }; // Error: 'z' is excess (still fresh)

interface Point {
    x: number;
    y: number;
}
