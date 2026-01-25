// Test for Rule #44: Module Augmentation / Interface Merging
// Single-file case - multiple interface declarations with same name should merge

// Test 1: Basic interface merging
interface Point {
    x: number;
}

interface Point {
    y: number;
}

// Point should have both x and y
const p1: Point = { x: 1, y: 2 };

// Test 2: Method overloads in merged interfaces
interface Calculator {
    add(x: number, y: number): number;
}

interface Calculator {
    add(x: string, y: string): string;
}

// Calculator should have both overloads
const calc: Calculator = {
    add(x: any, y: any): any {
        return x + y;
    }
};

// Test 3: Conflicting properties - first wins or error
interface Container {
    value: string;
}

interface Container {
    value: number;  // Should cause conflict
}

// This should either use first value or error
const c1: Container = { value: "test" };
const c2: Container = { value: 42 };

// Test 4: Properties with different types become union
interface Flexible {
    data: string;
}

interface Flexible {
    data: number;
}

// TypeScript: properties become union type string | number
const flex: Flexible = { data: "test" };  // OK
const flex2: Flexible = { data: 42 };    // OK (if union type)
