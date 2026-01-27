// Simple TS2488 test
const num = 123;
for (const x of num) {} // TS2488

type IndexedType = { prop: number }['prop'];
declare const idx: IndexedType;
for (const y of idx) {} // TS2488
