export type Sigil<T, B extends string> = T & { readonly __sigil: B };

const limitBounds = {
  fallback: 50,
  min: 1,
  max: 5000,
} as const satisfies Record<'fallback' | 'min' | 'max', number>;

export type ReadonlyDeep<T> =
  T extends (...args: any[]) => any ? T :
  T extends Array<infer U> ? ReadonlyArray<ReadonlyDeep<U>> :
  T extends object ? { readonly [K in keyof T]: ReadonlyDeep<T[K]> } :
  T;

export type OptionalKeys<T> = {
  [K in keyof T]-?: undefined extends T[K] ? K : never
}[keyof T];

export type RequiredKeys<T> = Exclude<keyof T, OptionalKeys<T>>;

export type DeepPartial<T> = T extends Function
  ? T
  : T extends Array<infer U>
    ? Array<DeepPartial<U>>
    : T extends object
      ? { [K in keyof T]?: DeepPartial<T[K]> }
      : T;

export type Replace<T, K extends keyof T, V> = Omit<T, K> & { [P in K]: V };

export type Expand<T> = T extends infer O ? { [K in keyof O]: O[K] } : never;

export interface WithId {
  id: Sigil<string, "EntityId">;
}

export interface WithTrace {
  traceId: Sigil<string, "TraceId">;
  spanId: Sigil<string, "SpanId">;
  requestId: Sigil<string, "RequestId">;
}

export type NodeId = Sigil<string, "NodeId">;
export type EdgeId = Sigil<string, "EdgeId">;

export interface Edge<T extends NodeId = NodeId, W = unknown> {
  from: T;
  to: T;
  weight: number;
  payload?: W;
}

export interface Graph<N extends NodeId = NodeId, E = unknown> {
  nodes: readonly N[];
  edges: readonly Edge<N, E>[];
}

export type MaybePromise<T> = T | Promise<T>;

export type ResultState<T, E> = { ok: true; value: T } | { ok: false; error: E };

export type TupleOf<T, N extends number, A extends T[] = []> =
  A['length'] extends N ? A : TupleOf<T, N, [...A, T]>;

export type NonEmpty<T extends readonly unknown[]> = T extends [infer H, ...infer R] ? [H, ...R] : never;

export type Merge<A, B> = Omit<A, keyof B> & B;

export type Flatten<T> = T extends readonly [infer A, ...infer R]
  ? A | Flatten<R>
  : never;

export type RecursivePath<T> =
  T extends object
    ? {
        [K in keyof T]: K extends string
          ? T[K] extends object
            ? K | `${K}.${RecursivePath<T[K]>}`
            : K
          : never
      }[keyof T]
    : never;

export interface PageArgs {
  limit: number;
  cursor?: string;
}

export interface PageResult<T> {
  items: T[];
  nextCursor?: string;
  hasMore: boolean;
}

export type CursorPage<T> = {
  items: T[];
  cursor: string;
  total: number;
};

export function withSigil<T extends string, B extends string>(value: T, _sigil: B): Sigil<T, B> {
  return value as Sigil<T, B>;
}

export function asReadonly<T>(value: T): ReadonlyDeep<T> {
  return value as ReadonlyDeep<T>;
}

export type EnsureArray<T> = T extends readonly any[] ? T : T[];

export function normalizeLimit(limit?: number): number {
  if (typeof limit !== 'number' || !Number.isFinite(limit)) return limitBounds.fallback;
  if (limit <= 0) return limitBounds.min;
  if (limit > limitBounds.max) return limitBounds.max;
  return Math.floor(limit);
}

export function isNil(value: unknown): value is null | undefined {
  return value === null || value === undefined;
}

export type PromiseResult<T> = Promise<ResultState<T, Error>>;

export function tuple<const T extends readonly unknown[]>(...values: T): T {
  return values;
}

export async function toResult<T>(work: () => Promise<T>): Promise<ResultState<T, Error>> {
  try {
    return { ok: true, value: await work() };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error : new Error(String(error)) };
  }
}
