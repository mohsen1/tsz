// Comprehensive TS2488 test cases
// Testing various type constructs that should emit TS2488

// 1. Basic non-iterable types (should already work)
const num = 123;
for (const x of num) {} // TS2488

// 2. Object without Symbol.iterator
const obj = { a: 1, b: 2 };
for (const x of obj) {} // TS2488

// 3. Optional Symbol.iterator (should emit TS2488)
declare var iterableWithOptionalIterator: {
    [Symbol.iterator]?(): Iterator<string>
};
for (const v of iterableWithOptionalIterator) {} // TS2488

// 4. Iterator without Symbol.iterator (has next() but not [Symbol.iterator]())
class MyStringIterator {
    next() {
        return { value: "", done: false };
    }
}
for (const v of new MyStringIterator) {} // TS2488

// 5. Spread of non-iterable
const spreadNum = [...123]; // TS2488
const spreadObj = [...{ a: 1 }]; // TS2488

// 6. Array destructuring of non-iterable
const [a, b] = { 0: "", 1: true }; // TS2488
const [c, d] = 456; // TS2488

// 7. Type parameter with non-iterable constraint
function testTypeParam<T extends number>(x: T) {
    for (const item of x) {} // TS2488
}

// 8. Indexed access type
type Obj = { prop: number };
type PropType = Obj['prop'];
declare const indexed: PropType;
for (const item of indexed) {} // TS2488

// 9. Conditional type
type ConditionalType = number extends string ? string[] : number;
declare const conditional: ConditionalType;
for (const item of conditional) {} // TS2488

// 10. Mapped type
type MappedType = { [K in 'a' | 'b']: number };
declare const mapped: MappedType;
for (const item of mapped) {} // TS2488

// 11. Union of non-iterables (all members must be iterable)
type NumOrBool = number | boolean;
declare const union: NumOrBool;
for (const item of union) {} // TS2488

// 12. Intersection with non-iterables
type Inter = { a: number } & { b: string };
declare const inter: Inter;
for (const item of inter) {} // TS2488

// 13. Function type
declare const fn: () => string;
for (const item of fn) {} // TS2488

// 14. Never type
declare const neverVal: never;
for (const item of neverVal) {} // TS2488
const [e, f] = neverVal; // TS2488

// 15. Null and undefined
for (const item of null) {} // TS2488
const [g, h] = null; // TS2488

// 16. Class instance without iterator
class MyClass {
    value = 42;
}
for (const item of new MyClass) {} // TS2488

// 17. Generic function type
type GenericFn<T> = (x: T) => T;
declare const genericFn: GenericFn<number>;
for (const item of genericFn) {} // TS2488
