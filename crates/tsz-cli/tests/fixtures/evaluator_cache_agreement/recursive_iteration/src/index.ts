type Assert<T extends true> = T;
type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;

type Step = [
  value: number,
  sign: "-" | "0" | "+",
  prev: keyof StepMap,
  next: keyof StepMap,
  opposite: keyof StepMap,
];

type StepMap = {
  "__": [number, "-" | "0" | "+", "__", "__", "__"];
  "-1": [-1, "-", "__", "0", "1"];
  "0": [0, "0", "-1", "1", "0"];
  "1": [1, "+", "0", "__", "-1"];
};

type StepOf<N extends number> =
  `${N}` extends keyof StepMap ? StepMap[`${N}`] : StepMap["__"];
type Prev<I extends Step> = StepMap[I[2]];
type Next<I extends Step> = StepMap[I[3]];
type Pos<I extends Step> = I[0];
type IsNegative<I extends Step> = { "-": true; "+": false; "0": false }[I[1]];
type IsPositive<I extends Step> = { "-": false; "+": true; "0": false }[I[1]];
type Cast<A, B> = A extends B ? A : B;

type SubPositive<A extends Step, B extends Step> = {
  0: SubPositive<Prev<A>, Prev<B>>;
  1: A;
  2: StepMap["__"];
}[Pos<B> extends 0 ? 1 : number extends Pos<B> ? 2 : 0] extends infer R
  ? Cast<R, Step>
  : never;

type SubNegative<A extends Step, B extends Step> = {
  0: SubNegative<Next<A>, Next<B>>;
  1: A;
  2: StepMap["__"];
}[Pos<B> extends 0 ? 1 : number extends Pos<B> ? 2 : 0] extends infer R
  ? Cast<R, Step>
  : never;

type Difference<A extends Step, B extends Step> = {
  false: SubPositive<A, B>;
  true: SubNegative<A, B>;
}[`${IsNegative<B>}`];

type Greater<A extends number, B extends number> =
  A extends unknown
    ? B extends unknown
      ? IsPositive<Difference<StepOf<A>, StepOf<B>>>
      : never
    : never;

type _greater = Assert<Equal<Greater<1, 0>, true>>;
type _notGreater = Assert<Equal<Greater<0, 1>, false>>;
type _distributed = Assert<Equal<Greater<1 | 0, 0>, boolean>>;

export {};
