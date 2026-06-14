export type NoInfer<TValue> = [TValue][TValue extends any ? 0 : never];

export type Expand<TValue> = TValue extends infer Inner
  ? { readonly [Key in keyof Inner]: Inner[Key] }
  : never;

export type Pairwise<
  TLeft extends readonly unknown[],
  TRight extends readonly unknown[],
  TOut extends readonly [unknown, unknown][] = readonly [],
> = TLeft extends readonly [infer LHead, ...infer LTail]
  ? TRight extends readonly [infer RHead, ...infer RTail]
    ? Pairwise<LTail, RTail, readonly [...TOut, [LHead, RHead]]>
    : TOut
  : TOut;
