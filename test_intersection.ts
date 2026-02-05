// Test intersection property access - should succeed
type A = { x: number };
type B = { y: string };

type I = A & B;

const i: I = { x: 1, y: "hello" };
const valX = i.x; // Should work: x exists in A
const valY = i.y; // Should work: y exists in B
