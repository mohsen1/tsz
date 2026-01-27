// Test for TS2571: Object is of type 'unknown'
// This happens when:
// 1. A generic function returns unknown type
// 2. Destructuring is used on that unknown value
// 3. Property access is attempted on unknown

// Test 1: Generic function returning unknown
declare function f<T>(): T;

// This should trigger TS2571 - empty pattern on unknown
const {} = f();

// This should trigger TS2571 - property destructuring on unknown
const { p1 } = f();

// This should trigger TS2571 - array destructuring on unknown
const [] = f();

const [e1, e2] = f();

// Test 2: Property access on unknown
function testPropertyAccess(obj: unknown) {
    // Should trigger TS2571
    const value = obj.property;
}

// Test 3: Private name in expression with unknown
class Foo {
    #field = 1;

    test(v: unknown) {
        // Should trigger TS2571
        const result = #field in v;
    }
}

// Test 4: Complex type with Parameters utility
// The Parameters<T[key]> pattern when T[key] is unknown
type Dispatch<A = { type: any; [extraProps: string]: any }> = {
    <T extends A>(action: T): T
};
type IFuncs = { readonly [key: string]: (...p: any) => void };
type IDestructuring<T extends IFuncs> = {
    readonly [key in keyof T]?: (...p: any) => void
};
type Destructuring<T extends IFuncs, U extends IDestructuring<T>> =
    (dispatch: Dispatch<any>, funcs: T) => U;

const funcs1 = {
    funcA: (a: boolean): void => {},
    funcB: (b: string, bb: string): void => {},
};

// This should work - f is inferred
declare function useReduxDispatch1<T extends IDestructuring<typeof funcs1>>(
    destructuring: Destructuring<typeof funcs1, T>
): T;

const {} = useReduxDispatch1(
    (d, f) => ({
        funcA: (...p) => d(f.funcA(...p)),
        funcB: (...p) => d(f.funcB(...p)),
    })
);
