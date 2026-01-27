// Minimal test case for TS2507

// This should NOT error - type parameter with constructor constraint
function test<T extends new () => any>(ctor: T) {
    class C extends ctor { }  // Should be valid
}

// This should error - plain object
var obj: {};
class C2 extends obj { }  // TS2507: Type '{}' is not a constructor function type
