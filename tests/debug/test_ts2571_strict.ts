// @strictNullChecks: true

declare function f<T>(): T;
const {} = f();       // error (only in strictNullChecks)
