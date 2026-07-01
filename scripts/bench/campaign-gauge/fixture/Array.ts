import { Kind, URItoKind } from "./HKT";

declare module "./HKT" {
  interface URItoKind<A> {
    readonly MyArray: ReadonlyArray<A>;
  }
}

export type MyArr = Kind<"MyArray", number>;

// Consume the augmented member so a regression in cross-file augmentation
// visibility surfaces as a diagnostic-count delta, not a silent no-op.
export const first = (xs: MyArr): number | undefined => xs[0];
