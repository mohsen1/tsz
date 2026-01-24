// Test symbol resolution issues

// Test 1: Static class members should be accessible via class name
class MyClass {
  static staticProp = "static";
  instanceProp = "instance";
}

// Should work: accessing static member via class
console.log(MyClass.staticProp);

// Should error: accessing instance member via class
console.log(MyClass.instanceProp); // Should error

// Should work: accessing instance member via instance
const instance = new MyClass();
console.log(instance.instanceProp);

// Test 2: Type parameters in scope
function generic<T>(value: T): T {
  // T should be accessible as a type
  type LocalType = T;
  return value;
}

// Test 3: Qualified name resolution
namespace Outer {
  export namespace Inner {
    export const value = 42;
  }
}

// Should work
const x = Outer.Inner.value;

// Test 4: Using undefined variable (should error TS2304)
console.log(undefinedVar); // TS2304

// Test 5: Interface used as value (should error TS18050)
interface MyInterface {
  prop: string;
}
const y = new MyInterface(); // TS18050

// Test 6: typeof on undefined symbol (should error TS2304)
type T1 = typeof UndefinedSymbol; // TS2304

// Test 7: keyof typeof combination
const obj = { a: 1, b: 2 };
type Keys = keyof typeof obj; // "a" | "b"

// Test 8: Re-exports
// export { foo } from './bar';
// foo should be accessible

// Test 9: typeof on class
class TestClass {
  method() {}
}
type T2 = typeof TestClass; // constructor type

// Test 10: Namespace member access via qualified name
namespace TestNS {
  export const exported = "exported";
  const notExported = "not exported";
}

// Should work
console.log(TestNS.exported);

// Should error
console.log(TestNS.notExported); // TS2694 or TS2339
