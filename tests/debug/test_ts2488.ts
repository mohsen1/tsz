// Test when TypeScript emits TS2488 specifically
const num = 123;
for (const item of num) {  // TS2488: Type 'number' must have a '[Symbol.iterator]()' method
    console.log(item);
}

interface NotIterable {
    a: number;
}
const obj: NotIterable = { a: 1 };
for (const item of obj) {  // TS2488?
    console.log(item);
}

// Test with union of non-iterables
type NumOrBool = number | boolean;
const x: NumOrBool = 5;
for (const item of x) {  // TS2488?
    console.log(item);
}

// Test with intersection
type Inter = { a: number } & { b: string };
const y: Inter = { a: 1, b: "test" };
for (const item of y) {  // TS2488?
    console.log(item);
}
