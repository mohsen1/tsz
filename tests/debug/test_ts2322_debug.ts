// Debug test to understand type resolution
class C { foo: string = ""; }
declare var c: C;

namespace M { export var x = 1; }

interface I { foo: string; }
declare var i: I;

var x: void;

// Let's trace the types
// c should have type C
// M should have type typeof M
// i should have type I

type TestC = typeof c;  // Should be C
type TestM = typeof M;  // Should be typeof M
type TestI = typeof i;  // Should be I

// Now test assignment - where are errors missing?
x = c;  // MISSING: Should error
x = M;  // MISSING: Should error
x = i;  // WORKS: Emits error
