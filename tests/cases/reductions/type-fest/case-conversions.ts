// Oracle: `tsc -p tests/cases/reductions/type-fest/tsconfig.json --pretty false`
// reports no diagnostics. These cases force recursive template-literal
// expansion through nested `infer`, `Lowercase`, and `Capitalize` applications.

export {};

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;

type CamelCase<S extends string> =
  S extends `${infer Head}-${infer Tail}`
    ? `${Lowercase<Head>}${Capitalize<CamelCase<Tail>>}`
    : S extends `${infer Head}_${infer Tail}`
      ? `${Lowercase<Head>}${Capitalize<CamelCase<Tail>>}`
      : Lowercase<S>;

type KebabCase<S extends string> =
  S extends `${infer First}${infer Rest}`
    ? Rest extends Uncapitalize<Rest>
      ? `${Lowercase<First>}${KebabCase<Rest>}`
      : `${Lowercase<First>}-${KebabCase<Rest>}`
    : S;

type CamelFromKebab = Assert<Equal<CamelCase<"user-profile-url">, "userProfileUrl">>;
type CamelFromSnake = Assert<Equal<CamelCase<"USER_PROFILE_URL">, "userProfileUrl">>;
type KebabFromCamel = Assert<Equal<KebabCase<"userProfileUrl">, "user-profile-url">>;
type KebabFromPascal = Assert<Equal<KebabCase<"UserProfileUrl">, "user-profile-url">>;
