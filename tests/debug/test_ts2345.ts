// Test TS2345: Argument of type X is not assignable to parameter of type Y

// Case 1: Simple type mismatch
function takesNumber(x: number) {}
takesNumber("hello"); // Should emit TS2345

// Case 2: Optional parameters
function optionalParam(x?: number) {}
optionalParam("hello"); // Should emit TS2345
optionalParam(undefined); // Should NOT emit TS2345

// Case 3: Rest parameters
function restNumbers(...args: number[]) {}
restNumbers(1, 2, 3); // Should NOT emit TS2345
restNumbers(1, "two", 3); // Should emit TS2345

// Case 4: Generic functions
function identity<T>(x: T): T { return x; }
identity(123); // Should NOT emit TS2345

// Case 5: Overloaded functions
function overloaded(x: number): number;
function overloaded(x: string): string;
function overloaded(x: any): any { return x; }
overloaded(123); // Should NOT emit TS2345
overloaded("hello"); // Should NOT emit TS2345
overloaded(true); // Should emit TS2345 (no matching overload)

// Case 6: Function parameters with union types
function takesUnion(x: number | string) {}
takesUnion(123); // Should NOT emit TS2345
takesUnion("hello"); // Should NOT emit TS2345
takesUnion(true); // Should emit TS2345

// Case 7: Optional vs undefined
function optionalVsUndefined(x: number | undefined) {}
optionalVsUndefined(undefined); // Should NOT emit TS2345
optionalVsUndefined(123); // Should NOT emit TS2345

// Case 8: Callback parameter contravariance
function takesCallback(cb: (x: number) => void) {}
takesCallback((x: number) => {}); // Should NOT emit TS2345
takesCallback((x: string) => {}); // Should emit TS2345 (parameter types contravariant)
