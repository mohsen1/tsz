// Try to trigger actual TS2488
// TS2488 is for "must have a '[Symbol.iterator]()' method that returns an iterator"

// Test with downlevelIteration
const set = new Set([1, 2, 3]);
for (const item of set) {  // Might need downlevelIteration
    console.log(item);
}

// Test with Symbol.iterator explicitly
const obj = {
    [Symbol.iterator]: "not a function" as any
};
for (const item of obj) {  // TS2488?
    console.log(item);
}

// Test spread operator
const notIterable = 123;
const arr = [...notIterable];  // TS2488?
