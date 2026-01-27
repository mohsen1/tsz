// Minimal test for TS2571 investigation
// Testing contextual typing for callback parameters

type IFuncs = {
    funcA: (a: boolean) => void;
    funcB: (b: string) => void;
};

type Callback = (funcs: IFuncs) => any;

declare function useCallback(cb: Callback): void;

// This should work - f should be inferred as IFuncs, not unknown
useCallback((f) => {
    // Accessing f.funcA should NOT trigger TS2571
    // because f is IFuncs, not unknown
    const x = f.funcA;
});
