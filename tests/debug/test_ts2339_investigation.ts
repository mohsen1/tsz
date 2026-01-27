// Test cases for TS2339 "Property does not exist" investigation

// Test 1: Union property access
type A = { x: number };
type B = { y: string };
type U = A | B;

function testUnion(u: U) {
    // Both properties should be accessible via union
    if ('x' in u) {
        console.log(u.x);  // Should work - x is number
    }
    if ('y' in u) {
        console.log(u.y);  // Should work - y is string
    }
}

// Test 2: Intersection property access
type C = { [key: string]: number };
type D = { x: string };
type I = C & D;

function testIntersection(i: I) {
    console.log(i.x);  // Should be string from D
    console.log(i.y);  // Should be number from C's index signature
}

// Test 3: Optional chaining
type Obj = { a?: { b?: string } };

function testOptionalChaining(o: Obj) {
    console.log(o.a?.b);  // Should work with optional chaining
    console.log(o.a?.b?.toUpperCase());  // Chained optionals
}

// Test 4: Type alias with index signature
type StringMap = { [key: string]: number };

function testIndexAlias(map: StringMap) {
    console.log(map.anyProperty);  // Should work via index signature
    console.log(map[123]);  // Should error - numeric index not defined
}

// Test 5: Array/tuple access
type Tuple = [string, number];

function testTuple(t: Tuple) {
    console.log(t[0]);  // string
    console.log(t[1]);  // number
    console.log(t[2]);  // Should error - out of bounds
}

// Test 6: Generic type property access
function generic<T extends { x: number }>(obj: T) {
    console.log(obj.x);  // Should work
}

// Test 7: typeof property access
const obj = { a: 1, b: 2 };
type ObjType = typeof obj;

function testTypeof(o: ObjType) {
    console.log(o.a);
    console.log(o.b);
    console.log(o.c);  // Should error - property doesn't exist
}

// Test 8: Class with dynamic properties
class DynamicClass {
    [key: string]: string;

    constructor() {
        this.a = "hello";
    }
}

function testDynamic(d: DynamicClass) {
    console.log(d.a);  // Should work via index signature
    console.log(d.b);  // Should work (returns string) even though not explicitly defined
}

console.log("TS2339 investigation tests loaded");
