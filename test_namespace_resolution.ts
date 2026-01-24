// Test namespace member resolution (TS2694)
// These tests validate that namespace members are properly resolved

// Test 1: Basic namespace with exported function
namespace NamespaceA {
    export function helper(): string {
        return "hello";
    }
}

// Should work - accessing exported function
const result1 = NamespaceA.helper();

// Test 2: Namespace with exported variable
namespace NamespaceB {
    export const value = 42;
}

// Should work - accessing exported variable
const result2 = NamespaceB.value;

// Test 3: Namespace with multiple exports
namespace NamespaceC {
    export function foo(): void {}
    export const bar = 123;
    export enum Enum {
        X,
        Y
    }
}

// Should work - accessing all members
NamespaceC.foo();
const x = NamespaceC.bar;
const y = NamespaceC.Enum.X;

// Test 4: Nested namespaces
namespace Outer {
    export namespace Inner {
        export function nestedFunc(): void {}
    }
}

// Should work - accessing nested namespace member
Outer.Inner.nestedFunc();

// Test 5: Namespace with type-only member (should error when used as value)
namespace NamespaceD {
    export interface Interface {
        x: number;
    }
}

// Should emit TS2693 - Interface only refers to a type
// const bad = NamespaceD.Interface; // Uncomment to test

// Test 6: Declaration merging
namespace NamespaceE {
    export function func1(): void {}
}

namespace NamespaceE {
    export function func2(): void {}
}

// Should work - both functions should be accessible after merging
NamespaceE.func1();
NamespaceE.func2();

// Test 7: Export from another namespace
namespace Source {
    export const sourceValue = "test";
}

namespace Destination {
    export { Source };
}

// Should work - accessing re-exported namespace
Destination.Source.sourceValue;

// Test 8: Enum as namespace member
namespace NamespaceF {
    export enum Color {
        Red,
        Green,
        Blue
    }
}

// Should work - accessing enum through namespace
const color = NamespaceF.Color.Red;

// Test 9: Namespace with class
namespace NamespaceG {
    export class Helper {
        static method(): string {
            return "helper";
        }
    }
}

// Should work - accessing class through namespace
const helper = new NamespaceG.Helper();
const result3 = NamespaceG.Helper.method();

// Test 10: Ambient namespace
declare module AmbientNamespace {
    export function ambientFunc(): void;
    export const ambientConst: number;
}

// Should work - accessing ambient members
AmbientNamespace.ambientFunc();
const ambientValue = AmbientNamespace.ambientConst;

console.log("All namespace resolution tests passed");
