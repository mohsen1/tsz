// Test missing TS2322 errors
var x: void;

// Class instance - MISSING ERROR
class C { foo: string = ""; }
declare var c: C;
x = c;  // Should error: Type 'C' is not assignable to type 'void'

// Namespace - MISSING ERROR
namespace M { export var x = 1; }
x = M;  // Should error: Type 'typeof M' is not assignable to type 'void'

// Interface instance - WORKS
interface I { foo: string; }
declare var i: I;
x = i;  // Should error: Type 'I' is not assignable to type 'void' - WORKS

// For comparison
x = 1;  // Should error - WORKS
x = "";  // Should error - WORKS
