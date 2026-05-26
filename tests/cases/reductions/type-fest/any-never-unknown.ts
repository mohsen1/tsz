// Oracle: `tsc -p tests/cases/reductions/type-fest/tsconfig.json --pretty false`
// reports no diagnostics. If `IsAny`, `IsNever`, or `IsUnknown` collapses to
// `boolean`, one of the `Assert` aliases below fails.

export {};

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;

type IsAny<T> = 0 extends (1 & T) ? true : false;
type IsNever<T> = [T] extends [never] ? true : false;
type IsUnknown<T> =
  IsAny<T> extends true ? false
    : unknown extends T
      ? [keyof T] extends [never] ? true : false
      : false;

type AnyIsAny = Assert<Equal<IsAny<any>, true>>;
type UnknownIsNotAny = Assert<Equal<IsAny<unknown>, false>>;
type NeverIsNever = Assert<Equal<IsNever<never>, true>>;
type UnionIsNotNever = Assert<Equal<IsNever<string | never>, false>>;
type UnknownIsUnknown = Assert<Equal<IsUnknown<unknown>, true>>;
type AnyIsNotUnknown = Assert<Equal<IsUnknown<any>, false>>;
type ObjectIsNotUnknown = Assert<Equal<IsUnknown<{ value: string }>, false>>;
