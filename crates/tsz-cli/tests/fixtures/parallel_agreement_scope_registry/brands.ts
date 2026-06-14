export type Brand<TValue, TMarker extends string = string> = TValue & {
  readonly __brand: TMarker;
};

export type BrandInfo<TValue> = TValue extends Brand<infer Raw, infer Marker>
  ? { readonly value: Raw; readonly marker: Marker }
  : never;

export const asBrand = <TValue extends string, TMarker extends string>(
  value: TValue,
  _marker: TMarker,
): Brand<TValue, TMarker> => value as Brand<TValue, TMarker>;
