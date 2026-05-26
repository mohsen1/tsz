// Oracle: `tsc -p tests/cases/reductions/type-fest/tsconfig.json --pretty false`
// reports no diagnostics. The assertions exercise `+?` / `-?` mapped modifiers
// so optionality is determined structurally, not by property spelling.

export {};

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;

type RequiredKeys<T> = {
  [K in keyof T]-?: {} extends Pick<T, K> ? never : K
}[keyof T];

type OptionalKeys<T> = {
  [K in keyof T]-?: {} extends Pick<T, K> ? K : never
}[keyof T];

type SetRequired<T, K extends keyof T> = Omit<T, K> & {
  [P in K]-?: T[P]
};

type SetOptional<T, K extends keyof T> = Omit<T, K> & {
  [P in K]+?: T[P]
};

type Source = {
  required: string;
  optional?: number;
  readonly readonlyRequired: boolean;
  readonly optionalReadonly?: Date;
  valueOrUndefined: string | undefined;
};

type SourceRequired = Assert<
  Equal<RequiredKeys<Source>, "required" | "readonlyRequired" | "valueOrUndefined">
>;
type SourceOptional = Assert<
  Equal<OptionalKeys<Source>, "optional" | "optionalReadonly">
>;

type Promoted = SetRequired<Source, "optional" | "optionalReadonly">;
type PromotedRequired = Assert<
  Equal<RequiredKeys<Promoted>, keyof Source>
>;
type Demoted = SetOptional<Source, "required" | "valueOrUndefined">;
type DemotedOptional = Assert<
  Equal<OptionalKeys<Demoted>, "required" | "optional" | "optionalReadonly" | "valueOrUndefined">
>;
