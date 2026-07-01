type Assert<T extends true> = T;
type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;

type Extends<A, B> = [A] extends [never] ? false : A extends B ? true : false;
type Contains<A, B> = Extends<A, B>;
type Equals<A, B> = Equal<A, B>;
type Mode = "default" | "contains" | "extends" | "equals";

type Test<A, B, M extends Mode = "default"> = {
  default: Extends<A, B>;
  contains: Contains<A, B>;
  extends: Extends<A, B>;
  equals: Equals<A, B>;
}[M];

type Select<U, M, ModeName extends Mode = "default"> =
  U extends unknown
    ? { true: U & M; false: never }[`${Test<U, M, ModeName>}`]
    : never;

type Values<T> = T[keyof T];
type Paths<T, P extends readonly PropertyKey[] = []> = {
  [K in keyof T]:
    | readonly [...P, K]
    | (T[K] extends object ? Paths<T[K], readonly [...P, K]> : never);
}[keyof T];

type Resolve<T, P extends readonly PropertyKey[]> =
  P extends readonly []
    ? T
    : P extends readonly [infer K, ...infer R]
      ? K extends keyof T
        ? Resolve<T[K], Extract<R, readonly PropertyKey[]>>
        : never
      : never;

type LeafPath<T> = Select<Paths<T>, readonly ["alpha", ...PropertyKey[]], "extends">;

type Model = {
  alpha: {
    first: string;
    second: number;
  };
  beta: {
    first: boolean;
  };
};

type AlphaPaths = LeafPath<Model>;
type AlphaValues = Resolve<Model, AlphaPaths>;
type AllValues = Values<{ a: AlphaValues; b: Select<Values<Model>, { first: boolean }, "extends"> }>;

type _paths = Assert<Equal<
  AlphaPaths,
  readonly ["alpha"] | readonly ["alpha", "first"] | readonly ["alpha", "second"]
>>;
type _values = Assert<Equal<
  AllValues,
  Model["alpha"] | string | number | { first: boolean }
>>;

export {};
