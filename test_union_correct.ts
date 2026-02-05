// Test union property access - should succeed (property exists in all members)
type A = { x: number };
type B = { x: string };

type U = A | B;

const u: U = { x: 1 } as U;
const val = u.x; // Should work: x exists in both A and B
