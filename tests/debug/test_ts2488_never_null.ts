// Test TS2488 for never and null types
declare const neverVal: never;
for (const x of neverVal) {} // TS2488

const [a, b] = neverVal; // TS2488

for (const y of null) {} // TS2488

const [c, d] = null; // TS2488
