export type Primitive = string | number | boolean | bigint | symbol | null | undefined;
export type Brand<T, B extends string> = T & { readonly __brand: B };
export type DeepReadonly<T> = T extends (...args: any[]) => any
  ? T
  : T extends Primitive
    ? T
    : T extends Array<infer U>
      ? ReadonlyArray<DeepReadonly<U>>
      : T extends object
        ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
        : T;
export type KeyPaths<T> = T extends Date | Primitive
  ? never
  : T extends Array<infer U>
    ? KeyPaths<U> extends never ? `[]` : `[]` | `[${number}]${KeyPaths<U> extends never ? '' : `.${KeyPaths<U>}`}`
    : { [K in keyof T & string]: T[K] extends Primitive
        ? K
        : T[K] extends Array<infer U>
          ? `${K}[]` | `${K}[${number}]${KeyPaths<U> extends never ? '' : `.${KeyPaths<U>}`}`
          : `${K}` | `${K}.${KeyPaths<T[K]>}`
      }[keyof T & string];
export type PathValue<T, P extends string> =
  P extends `${infer H}.${infer R}`
    ? H extends keyof T
      ? PathValue<T[H], R>
      : unknown
    : P extends keyof T
      ? T[P]
      : unknown;
