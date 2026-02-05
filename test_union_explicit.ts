// Explicit union test
type A = { x: number };
type B = { y: string };

type U = A | B;

const u: U = { x: 1 } as U;
const val = u.x;
