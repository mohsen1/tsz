// Oracle: `tsc -p tests/cases/reductions/type-fest/tsconfig.json --pretty false`
// reports no diagnostics. The path parser and indexed-access reducer must keep
// each recursive property hop precise.

export {};

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;

type PathSegments<Path extends string> =
  Path extends `${infer Head}.${infer Tail}` ? [Head, ...PathSegments<Tail>] : [Path];

type GetAt<T, Parts extends readonly string[]> =
  Parts extends [infer Head extends string, ...infer Tail extends string[]]
    ? Head extends keyof T
      ? Tail extends []
        ? T[Head]
        : GetAt<T[Head], Tail>
      : never
    : T;

type Get<T, Path extends string> = GetAt<T, PathSegments<Path>>;

type Model = {
  user: {
    profile: {
      name: string;
      flags: {
        active: boolean;
      };
    };
  };
  settings: {
    theme: "dark" | "light";
  };
};

type Name = Assert<Equal<Get<Model, "user.profile.name">, string>>;
type Active = Assert<Equal<Get<Model, "user.profile.flags.active">, boolean>>;
type Theme = Assert<Equal<Get<Model, "settings.theme">, "dark" | "light">>;
type Missing = Assert<Equal<Get<Model, "user.profile.missing">, never>>;
