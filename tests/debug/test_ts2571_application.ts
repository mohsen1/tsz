// Test TS2571 with generic functions and Application types

type IFuncs = {
    funcA: (a: boolean) => void;
    funcB: (b: string) => void;
};

type IDestructuring<T extends IFuncs> = {
    readonly [key in keyof T]?: (...p: any) => void
};

type Destructuring<T extends IFuncs, U extends IDestructuring<T>> =
    (funcs: T) => U;

const funcs1 = {
    funcA: (a: boolean): void => {},
    funcB: (b: string): void => {},
};

type TFuncs1 = typeof funcs1;

declare function useDestructuring<T extends IDestructuring<TFuncs1>>(
    destructuring: Destructuring<TFuncs1, T>
): T;

// This should work - f should be inferred as TFuncs1
const result = useDestructuring((f) => ({
    funcA: (...p) => f.funcA(...p),
    funcB: (...p) => f.funcB(...p),
}));
