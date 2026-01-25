// Test file to investigate TS2304 false positives

// Test 1: Global symbols (console, Array, etc.)
const x1 = console.log("hello");  // Should work
const x2 = new Array(5);  // Should work
const x3 = Promise.resolve(42);  // Should work

// Test 2: Type parameters in generic function
function generic<T>(value: T): T {
    const y: T = value;  // Should work - T is a type parameter
    return value;
}
const result = generic(42);

// Test 3: Function hoisting
hoistedFunction();  // Should work - function hoisting
function hoistedFunction() {
    return 42;
}

// Test 4: Var hoisting
console.log(hoistedVar);  // Should work - var hoisting
var hoistedVar = 42;

// Test 5: Namespace members
namespace MyNamespace {
    export function helper() {
        return "helper";
    }
}
const ns = MyNamespace;  // Should work
const result2 = MyNamespace.helper();  // Should work

// Test 6: Ambient declarations
declare global {
    const AMBIENT_GLOBAL: number;
}
const x4 = AMBIENT_GLOBAL;  // Should work

// Test 7: Class static members
class MyClass {
    static staticMethod() {
        return "static";
    }
}
MyClass.staticMethod();  // Should work

// Test 8: Type alias with type parameter
type Pair<T> = [T, T];
const pair: Pair<number> = [42, 42];  // Should work
