// Test for namespace.subsymbol member resolution
// @ts-check

// Test 1: Basic namespace member access
namespace NS1 {
    export const x = 1;
    export function foo() { return 2; }
    export class Bar { value: number = 3; }
    export enum Color { Red, Green }
}

const a = NS1.x; // Should resolve to 1
const b = NS1.foo(); // Should resolve to function call
const c = new NS1.Bar(); // Should resolve to class constructor
const d = NS1.Color.Red; // Should resolve to enum member

// Test 2: Nested namespaces
namespace Outer {
    export namespace Inner {
        export const value = 42;
        export function getDouble() { return value * 2; }
    }
}

const e = Outer.Inner.value; // Should resolve to 42
const f = Outer.Inner.getDouble(); // Should resolve to function call

// Test 3: Namespace with non-exported members (should not be accessible)
namespace NS2 {
    export const exported = 1;
    const notExported = 2; // Not accessible via NS2.notExported
}

const g = NS2.exported; // OK
// const h = NS2.notExported; // Should error: not exported

// Test 4: Declaration merging with namespace
interface Merged {
    fromInterface: string;
}
namespace Merged {
    export const fromNamespace = 42;
    export function helper() { return "helper"; }
}

const i = Merged.fromNamespace; // Should resolve to 42
const j = Merged.helper(); // Should resolve to function

// Test 5: Enum merging with namespace
enum Direction {
    Up = 1,
    Down = 2
}
namespace Direction {
    export function getName(d: Direction): string {
        return d === Direction.Up ? "Up" : "Down";
    }
    export const helperValue = 99;
}

const k = Direction.Up; // Should resolve to 1
const l = Direction.getName(Direction.Down); // Should resolve to function
const m = Direction.helperValue; // Should resolve to 99

// Test 6: Deep namespace chains
namespace A {
    export namespace B {
        export namespace C {
            export const deepValue = "deep";
            export function deepFunc() { return "func"; }
        }
    }
}

const n = A.B.C.deepValue; // Should resolve to "deep"
const o = A.B.C.deepFunc(); // Should resolve to function call

// Test 7: Namespace with class and value exports
namespace Mixed {
    export class MyClass {
        method() { return "method"; }
    }
    export const myVar = "variable";
    export function myFunc() { return "function"; }
}

const p = new Mixed.Myclaass().method(); // Should resolve
const q = Mixed.myVar; // Should resolve to "variable"
const r = Mixed.myFunc(); // Should resolve to function

// Test 8: Namespace re-opening with additional exports
namespace Reopened {
    export const first = 1;
}
namespace Reopened {
    export const second = 2;
    export function combined() { return first + second; }
}

const s = Reopened.first; // Should resolve to 1
const t = Reopened.second; // Should resolve to 2
const u = Reopened.combined(); // Should resolve to function returning 3

// Test 9: Const enum namespace merging
const enum Status {
    Pending = 0,
    Active = 1,
    Done = 2
}
namespace Status {
    export function label(s: Status): string {
        switch (s) {
            case Status.Pending: return "Pending";
            case Status.Active: return "Active";
            case Status.Done: return "Done";
        }
    }
}

const v = Status.Active; // Should resolve to 1
const w = Status.label(Status.Pending); // Should resolve to function
