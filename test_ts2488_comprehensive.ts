// Comprehensive test for TS2488 - Type is not iterable

// ==================== for-of loops ====================

// Should ERROR - primitive non-iterable types
for (const x of 5) { } // TS2488
for (const x of true) { } // TS2488
for (const x of undefined) { } // TS2488
for (const x of null) { } // TS2488
for (const x of Symbol()) { } // TS2488
for (const x of 42n) { } // TS2488

// Should ERROR - object without Symbol.iterator
for (const x of {}) { } // TS2488
for (const x of { a: 1 }) { } // TS2488

// Should ERROR - function/class without iterator
function foo() {}
for (const x of foo) { } // TS2488

class Bar {}
for (const x of new Bar()) { } // TS2488

// Should ERROR - intersection with non-iterable
type ObjWithIterator = {} & { [Symbol.iterator]: () => Iterator<any> };
for (const x of {} as ObjWithIterator) { } // Might not error depending on intersection handling

// Should NOT ERROR - built-in iterables
for (const x of "hello") { }
for (const x of [1, 2, 3]) { }
for (const x of [1, 2, 3] as const) { }

// Should NOT ERROR - custom iterable with Symbol.iterator
const customIterable = {
    *[Symbol.iterator]() {
        yield 1;
        yield 2;
    }
};
for (const x of customIterable) { }

// ==================== spread operators ====================

// Should ERROR - spread non-iterable in array literal
[...5]; // TS2488
[...true]; // TS2488
[...null]; // TS2488
[...undefined]; // TS2488
[...{}]; // TS2488
[...foo]; // TS2488

// Should NOT ERROR - spread iterable
[..."hello"];
[...[1, 2, 3]];
[...customIterable];

// Should ERROR - spread non-iterable in function call
function testFn(...args: number[]) {}
testFn(...5); // TS2488
testFn(...{}); // TS2488
testFn(...foo); // TS2488

// Should NOT ERROR - spread iterable in function call
testFn(...[1, 2, 3]);
testFn(..."hello" as any); // Type mismatch but iterable

// ==================== array destructuring ====================

// Should ERROR - destructuring non-iterable
const [a] = 5; // TS2488
const [b] = true; // TS2488
const [c] = null; // TS2488
const [d] = undefined; // TS2488
const [e] = {}; // TS2488
const [f] = foo; // TS2488

// Should NOT ERROR - destructuring iterable
const [g] = [1, 2, 3];
const [h] = "hello";
const [i] = customIterable;

// ==================== union types ====================

// Should ERROR - union where not all members are iterable
type PartialIterable = number | string[];
for (const x of {} as PartialIterable) { } // TS2488 - number is not iterable

// Should NOT ERROR - union where all members are iterable
type AllIterable = string[] | number[];
for (const x of {} as AllIterable) { }

// ==================== generics ====================

// Generic function that requires iterable
function iterate<T>(items: T) {
    for (const item of items) { } // Should error if T is not iterable
}

// Should ERROR
iterate(5); // TS2488
iterate({}); // TS2488

// Should NOT ERROR
iterate([1, 2, 3]);
iterate("hello");

// Constrained generic
function iterateConstrained<T extends Array<any>>(items: T) {
    for (const item of items) { } // Should not error
}

iterateConstrained([1, 2, 3]);

// ==================== type assertions ====================

// Type assertions can bypass checking at declaration time
for (const x of {} as any) { } // No error - any
for (const x of {} as unknown) { } // No error - unknown

// But should still error with non-any assertions
for (const x of 5 as never) { } // TS2488
for (const x of {} as object) { } // TS2488

// ==================== nested iteration ====================

// Should ERROR - nested for-of with non-iterable
for (const x of 42) {
    for (const y of x) { } // TS2488 on inner
}

// ==================== await for-of ====================

async function asyncTests() {
    // Should ERROR - non-async-iterable
    for await (const x of 5) { } // TS2504
    for await (const x of {}) { } // TS2504

    // Should NOT ERROR - async iterable
    const asyncIterable = {
        async *[Symbol.asyncIterator]() {
            yield 1;
            yield 2;
        }
    };
    for await (const x of asyncIterable) { }

    // Should NOT ERROR - regular iterable is also valid in for-await-of
    for await (const x of [1, 2, 3]) { }
}

console.log("TS2488 comprehensive test file");
