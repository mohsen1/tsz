#!/usr/bin/env bash
#
# External-module stub writers for the 2026-07 canary-fixture repair campaign
# (msw, valtio, ts-extras, superjson, tanstack-router, ts-morph). Sourced by
# scripts/bench/project-fixtures.sh next to project-fixture-stubs.sh; split into
# its own shard to keep both files under the 2000-line ceiling. Same contract:
# each function takes the fixture tsconfig output path and derives the fixture
# directory. Stub contents were produced by iterative pinned-tsc fixpoint runs
# (add a declaration per unresolved-name error until tsc converges) and are
# emitted verbatim; the fixtures they green were validated at tsc 6.0.3 exit 0
# (superjson, ts-extras, tanstack-router) or best-effort honest residuals
# (valtio 1, msw 3, ts-morph ~180 — see the per-config comments in
# project-fixtures.sh).

if [ -n "${_TSZ_PROJECT_FIXTURE_STUBS_CANARY_SOURCED:-}" ]; then
  return 0 2>/dev/null || true
fi
_TSZ_PROJECT_FIXTURE_STUBS_CANARY_SOURCED=1


tsz_write_superjson_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'
declare module 'copy-anything' {
  export type copy<A = any, B = any, C = any, D = any, E = any> = any;
  export const copy: any;
  const __tszDefault: any;
  export default __tszDefault;
}
TYPES
}

tsz_write_ts_extras_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'
declare module 'type-fest' {
  export type Finite<A = any, B = any, C = any, D = any, E = any> = any;
  export const Finite: any;
  export type Integer<A = any, B = any, C = any, D = any, E = any> = any;
  export const Integer: any;
  export type IsEqual<A = any, B = any, C = any, D = any, E = any> = any;
  export const IsEqual: any;
  export type IsLiteral<A = any, B = any, C = any, D = any, E = any> = any;
  export const IsLiteral: any;
  export type IsTuple<A = any, B = any, C = any, D = any, E = any> = any;
  export const IsTuple: any;
  export type Join<A = any, B = any, C = any, D = any, E = any> = any;
  export const Join: any;
  export type LastArrayElement<A = any, B = any, C = any, D = any, E = any> = any;
  export const LastArrayElement: any;
  export type NegativeInfinity<A = any, B = any, C = any, D = any, E = any> = any;
  export const NegativeInfinity: any;
  export type PositiveInfinity<A = any, B = any, C = any, D = any, E = any> = any;
  export const PositiveInfinity: any;
  export type Simplify<A = any, B = any, C = any, D = any, E = any> = any;
  export const Simplify: any;
  export type Split<A = any, B = any, C = any, D = any, E = any> = any;
  export const Split: any;
  export type UnknownRecord<A = any, B = any, C = any, D = any, E = any> = any;
  export const UnknownRecord: any;
  export type Writable<A = any, B = any, C = any, D = any, E = any> = any;
  export const Writable: any;
  const __tszDefault: any;
  export default __tszDefault;
}
TYPES
}

tsz_write_valtio_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'
declare module 'proxy-compare' {
  export type affectedToPathList<A = any, B = any, C = any, D = any, E = any> = any;
  export const affectedToPathList: any;
  export type createProxy<A = any, B = any, C = any, D = any, E = any> = any;
  export const createProxy: any;
  export type getUntracked<A = any, B = any, C = any, D = any, E = any> = any;
  export const getUntracked: any;
  export type isChanged<A = any, B = any, C = any, D = any, E = any> = any;
  export const isChanged: any;
  export type markToTrack<A = any, B = any, C = any, D = any, E = any> = any;
  export const markToTrack: any;
  const __tszDefault: any;
  export default __tszDefault;
}
TYPES
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
// react hooks are called with explicit type arguments (`useRef<T>(...)`,
// `useMemo<T>(...)`) and receive inline callbacks whose parameters must pick up
// a contextual type (otherwise noImplicitAny trips TS7006). Generic call
// signatures satisfy the type-arg calls; the index signature keeps every other
// member `any`.
declare module 'react' {
  export const useCallback: <T = any>(cb: (...args: any[]) => any, deps?: any) => any;
  export const useDebugValue: (...args: any[]) => any;
  export const useEffect: (effect: (...args: any[]) => any, deps?: any) => any;
  export const useLayoutEffect: (effect: (...args: any[]) => any, deps?: any) => any;
  export const useMemo: <T = any>(factory: (...args: any[]) => any, deps?: any) => any;
  export const useRef: <T = any>(initial?: any) => any;
  export const useSyncExternalStore: <T = any>(...args: any[]) => any;
  const _default: any;
  export default _default;
}

// `@redux-devtools/extension` augments the global `Window` with the devtools
// connector; valtio reads `window.__REDUX_DEVTOOLS_EXTENSION__` directly, so the
// property must exist on `Window` rather than tripping TS2339.
declare module '@redux-devtools/extension';

interface Window {
  __REDUX_DEVTOOLS_EXTENSION__?: {
    connect(options: { [key: string]: any }): any;
    [key: string]: any;
  };
}
TYPES
}

tsz_write_msw_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'
declare module '@mswjs/interceptors/ClientRequest' {
  export type ClientRequestInterceptor<A = any, B = any, C = any, D = any, E = any> = any;
  export const ClientRequestInterceptor: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module '@mswjs/interceptors/XMLHttpRequest' {
  export type XMLHttpRequestInterceptor<A = any, B = any, C = any, D = any, E = any> = any;
  export const XMLHttpRequestInterceptor: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module '@mswjs/interceptors/fetch' {
  export type FetchInterceptor<A = any, B = any, C = any, D = any, E = any> = any;
  export const FetchInterceptor: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'cookie' {
  export type parse<A = any, B = any, C = any, D = any, E = any> = any;
  export const parse: any;
  export type serialize<A = any, B = any, C = any, D = any, E = any> = any;
  export const serialize: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'is-node-process' {
  export type isNodeProcess<A = any, B = any, C = any, D = any, E = any> = any;
  export const isNodeProcess: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'path-to-regexp' {
  export type match<A = any, B = any, C = any, D = any, E = any> = any;
  export const match: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'statuses' {
  export type message<A = any, B = any, C = any, D = any, E = any> = any;
  export const message: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'tough-cookie' {
  export type Cookie<A = any, B = any, C = any, D = any, E = any> = any;
  export const Cookie: any;
  export type CookieJar<A = any, B = any, C = any, D = any, E = any> = any;
  export const CookieJar: any;
  export type MemoryCookieStore<A = any, B = any, C = any, D = any, E = any> = any;
  export const MemoryCookieStore: any;
  export type MemoryCookieStoreIndex<A = any, B = any, C = any, D = any, E = any> = any;
  export const MemoryCookieStoreIndex: any;
  export type SerializedCookie<A = any, B = any, C = any, D = any, E = any> = any;
  export const SerializedCookie: any;
  const __tszDefault: any;
  export default __tszDefault;
}
declare module 'type-fest' {
  export type PartialDeep<A = any, B = any, C = any, D = any, E = any> = any;
  export const PartialDeep: any;
  const __tszDefault: any;
  export default __tszDefault;
}
TYPES
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
// msw builds for a dual Node.js + browser environment; its real tsconfig pulls
// in @types/node. The clone installs nothing and the bench baseline pins
// `types: []` with a DOM-only lib, so the Node-flavored globals msw relies on
// (the `require` fallback, the `NodeJS` namespace, and `setTimeout` returning a
// ref-counted handle rather than a `number`) are missing and tsc emits spurious
// TS2591/TS2503/TS2339/TS2345. Provide the minimal Node ambient surface so
// msw's own source type-checks like it does against @types/node.
declare function require(id: string): any;

declare namespace NodeJS {
  // msw's `hasRefCounted` narrows to `T & NodeJS.RefCounted` before calling
  // `.unref()` on a timer/BroadcastChannel handle.
  interface RefCounted {
    ref(): this;
    unref(): this;
    hasRef(): boolean;
  }
  interface Timeout extends RefCounted {
    [Symbol.toPrimitive](): number;
  }
  type Immediate = RefCounted;
  type Timer = Timeout;
  interface ProcessEnv {
    [key: string]: string | undefined;
  }
}

// With @types/node, `setTimeout` returns an object handle (not the DOM `number`),
// which msw's `hasRefCounted(timeoutId)` guard (constrained `T extends object`)
// depends on. The generic `@types/node` signature is preferred over lib.dom's
// non-generic `TimerHandler` overload during resolution, so this return type
// wins even when the callback is a pre-typed function (e.g. a Promise executor's
// `resolve`).
declare function setTimeout<TArgs extends any[]>(
  callback: (...args: TArgs) => void,
  ms?: number,
  ...args: TArgs
): NodeJS.Timeout;

// ---------------------------------------------------------------------------
// External runtime dependencies used with real shapes (not resolvable as bare
// `any`, or `any` would break a real relation such as HttpResponse's Response
// inheritance chain). Each remaining external is stubbed generically in
// tsz-bench-external-modules.d.ts.
// ---------------------------------------------------------------------------

declare module '@mswjs/interceptors' {
  // msw's HttpResponse `extends FetchResponse`; FetchResponse itself extends the
  // global Response, so HttpResponse must remain a structural Response. Stubbing
  // FetchResponse as bare `any` severs that chain and cascades into TS2345/TS2352
  // wherever an HttpResponse flows into a Response position.
  export class FetchResponse extends Response {
    constructor(body?: any, init?: any);
    static error(): any;
    static json(body?: any, init?: any): any;
    static isResponse(value: any): value is Response;
    static isResponseWithBody(status: number): boolean;
    static isConfigurableStatusCode(status: number): boolean;
    static isRedirectResponse(status: number): boolean;
    static parseRawHeaders(rawHeaders: any): Headers;
    [key: string]: any;
  }
  export class Interceptor<E = any> {
    constructor(...args: any[]);
    [key: string]: any;
  }
  export class BatchInterceptor<E = any, F = any> {
    constructor(...args: any[]);
    [key: string]: any;
  }
  export class RequestController {
    constructor(...args: any[]);
    respondWith(response: any): void;
    [key: string]: any;
  }
  export type HttpRequestEventMap = any;
  export function createRequestId(): string;
  export function getCleanUrl(url: any, isAbsolute?: boolean): string;
  export function resolveWebSocketUrl(url: any): any;
}

// The WebSocket interceptor subpath drives msw's WebSocket handling: connection
// objects are destructured and their `addEventListener` callbacks must receive a
// contextual `event` type (otherwise noImplicitAny trips TS7006 on every
// listener). Model the connection surface explicitly; other members stay `any`.
declare module '@mswjs/interceptors/WebSocket' {
  // The minimal connection protocol (no raw `socket`) is what msw's
  // `WebSocketRemoteClientConnection` implements and what `WebSocketHandler`
  // consumes; `removeEventListener` is optional since the remote client omits it.
  interface WebSocketConnectionProtocol {
    id: string;
    url: URL;
    addEventListener(type: any, listener: (event: any) => any, options?: any): void;
    removeEventListener?(type: any, listener: (event: any) => any, options?: any): void;
    send(data: any): void;
    close(code?: any, reason?: any): void;
    [key: string]: any;
  }
  export type WebSocketClientConnectionProtocol = WebSocketConnectionProtocol;
  export type WebSocketServerConnectionProtocol = WebSocketConnectionProtocol;
  // The full connection additionally exposes the underlying `socket` that the
  // logger attaches listeners to.
  interface WebSocketConnectionFull extends WebSocketConnectionProtocol {
    socket: {
      addEventListener(type: any, listener: (event: any) => any, options?: any): void;
      removeEventListener(type: any, listener: (event: any) => any, options?: any): void;
      send(data: any): void;
      close(code?: any, reason?: any): void;
      readyState: number;
      [key: string]: any;
    };
  }
  export type WebSocketClientConnection = WebSocketConnectionFull;
  export type WebSocketServerConnection = WebSocketConnectionFull;
  export interface WebSocketConnectionData {
    client: WebSocketConnectionFull;
    server: WebSocketConnectionFull;
    // Required (not optional): msw's WebSocketHandlerConnection declares
    // `info: WebSocketConnectionData['info']` as a required property and builds
    // it by spreading a connection, so an optional `info` here is not assignable.
    info: any;
    [key: string]: any;
  }
  export type WebSocketData = any;
  export interface WebSocketClientEventMap {
    [key: string]: any;
  }
  export interface WebSocketEventMap {
    [key: string]: any;
  }
  export class WebSocketInterceptor {
    constructor(...args: any[]);
    on(event: any, listener: (...args: any[]) => any): this;
    once(event: any, listener: (...args: any[]) => any): this;
    off(event: any, listener?: (...args: any[]) => any): this;
    apply(): void;
    dispose(): void;
    [key: string]: any;
  }
}

declare module '@open-draft/deferred-promise' {
  // Constructed with an optional executor and awaited; the class extends Promise
  // so `.then`/`.catch`/`.finally` resolve and the executor callback parameters
  // pick up a contextual type (avoiding TS7031/TS7006).
  export class DeferredPromise<T = any> extends Promise<T> {
    constructor(
      executor?: (
        resolve: (value?: T | PromiseLike<T>) => void,
        reject: (reason?: any) => void,
      ) => void,
    );
    resolve(value?: T | PromiseLike<T>): void;
    reject(reason?: any): void;
    state: 'pending' | 'fulfilled' | 'rejected';
    rejectionReason: any;
    [key: string]: any;
  }
  export type DeferredPromiseExecutor<T = any> = any;
}

declare module 'graphql' {
  // Parsed documents are iterated (`node.definitions.find(...)`), so `parse` must
  // return a real DocumentNode shape rather than bare `any` (which collapses the
  // callback parameter to implicit-any, tripping TS7006, and narrows dependent
  // unions to `never`).
  export interface DocumentNode {
    kind: any;
    definitions: readonly any[];
    [key: string]: any;
  }
  export interface OperationDefinitionNode {
    kind: any;
    operation: any;
    name?: any;
    [key: string]: any;
  }
  export type OperationTypeNode = 'query' | 'mutation' | 'subscription';
  export function parse(source: any, options?: any): DocumentNode;
  export function print(ast: any): string;
  export class GraphQLError extends Error {
    constructor(message: string, ...args: any[]);
    [key: string]: any;
  }
  const _default: any;
  export default _default;
}

declare module 'until-async' {
  // Called with explicit type arguments (`until<Error, Tuple>(cb)`) and its result
  // destructured as `[error, data]`.
  export function until<Err = any, Data = any>(
    callback: () => Promise<Data> | Data,
  ): Promise<[Err | null, Data]>;
}

declare module 'headers-polyfill' {
  // `stringToHeaders` must return a real `Headers` so downstream `.get(...)`
  // narrows to `string | null` (a bare `any` collapses callbacks over the parsed
  // result into implicit-any parameters, tripping TS7006).
  export function stringToHeaders(str: string): Headers;
  export function headersToString(headers: Headers): string;
  export function headersToObject(headers: Headers): Record<string, any>;
  export function objectToHeaders(obj: any): Headers;
  export function reduceHeadersObject<T = any>(headers: any, reducer: any, initial: T): T;
  export function flattenHeadersObject(obj: any): Record<string, string>;
  export const Headers: typeof globalThis.Headers;
}

declare module 'outvariant' {
  export function invariant(
    predicate: any,
    message: string,
    ...positionals: any[]
  ): asserts predicate;
  export namespace invariant {
    function as(
      ErrorConstructor: any,
      predicate: any,
      message: string,
      ...positionals: any[]
    ): asserts predicate;
  }
  export function format(message: string, ...positionals: any[]): string;
  export class InvariantError extends Error {}
}

// strict-event-emitter and rettime both export an `Emitter` used as a generic
// type, constructed, called with typed listeners, and dereferenced as a
// namespace (`Emitter.Listener<...>`). A class+namespace merge with generic
// methods covers value, type, generic-call, and namespace-member positions.
declare module 'strict-event-emitter' {
  export class Emitter<E = any> {
    on<T = any>(type: any, listener: (...args: any[]) => any): this;
    once<T = any>(type: any, listener: (...args: any[]) => any): this;
    off<T = any>(type: any, listener?: (...args: any[]) => any): this;
    emit<T = any>(type: any, ...args: any[]): boolean;
    addListener<T = any>(type: any, listener: (...args: any[]) => any): this;
    removeListener<T = any>(type: any, listener: (...args: any[]) => any): this;
    removeAllListeners<T = any>(type?: any): this;
    listeners<T = any>(type: any): Array<(...args: any[]) => any>;
    listenerCount<T = any>(type: any): number;
    [key: string]: any;
  }
  export namespace Emitter {
    type Listener<A = any, B = any> = (...args: any[]) => any;
    type EventMap = any;
  }
  export type EventMap = any;
  export class MemoryLeakError extends Error {}
}

declare module 'rettime' {
  export class Emitter<E = any> {
    on<T = any>(type: any, listener: (...args: any[]) => any, options?: any): any;
    once<T = any>(type: any, listener: (...args: any[]) => any, options?: any): any;
    off<T = any>(type: any, listener?: (...args: any[]) => any): any;
    emit<T = any>(type: any, ...args: any[]): any;
    emitAsPromise<T = any>(type: any, ...args: any[]): Promise<any>;
    addListener<T = any>(type: any, listener: (...args: any[]) => any, options?: any): any;
    removeListener<T = any>(type: any, listener?: (...args: any[]) => any): any;
    removeAllListeners<T = any>(type?: any): any;
    listeners<T = any>(type?: any): Array<(...args: any[]) => any>;
    listenerCount<T = any>(type?: any): number;
    [key: string]: any;
  }
  export namespace Emitter {
    type Listener<A = any, B = any> = (...args: any[]) => any;
    type Event<A = any, B = any, C = any> = any;
    type EventMap = any;
  }
  export class TypedEvent<A = any, B = any, C = any, D = any, E = any> {
    // msw's frame subclasses call `super(...([type, {}] as any))`, spreading an
    // `any`-typed array into the constructor; a rest parameter accepts it.
    constructor(...args: any[]);
    type: any;
    data: any;
    [key: string]: any;
  }
  export type TypedListenerOptions<A = any, B = any, C = any> = any;
  export type DefaultEventMap = any;
}

// msw's Node adapter uses `AsyncLocalStorage` from `node:async_hooks` as both a
// generic type and a constructor; @types/node supplies it in the real build.
declare module 'node:async_hooks' {
  export class AsyncLocalStorage<T = any> {
    getStore(): T | undefined;
    run<R>(store: T, callback: (...args: any[]) => R, ...args: any[]): R;
    enterWith(store: T): void;
    disable(): void;
    [key: string]: any;
  }
  export class AsyncResource {
    constructor(type: string, options?: any);
    [key: string]: any;
  }
}
TYPES
}

tsz_write_tanstack_router_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'

TYPES
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
// tanstack router-core reads `process.env.NODE_ENV` for dev/prod branching; its
// real build resolves `process` via @types/node, absent in the clone-only
// fixture. Provide an ambient any `process` (mirroring @types/node's global).
declare var process: any;

// seroval's plugin API: `createPlugin({ test, parse, serialize, deserialize })`
// registers custom serializers. The config callbacks must carry typed parameters
// (a bare `any` config collapses `serialize(node, ctx, data)` params to
// implicit-any, tripping TS7006). Model the config surface; payloads stay `any`.
declare module 'seroval' {
  export type SerovalNode = any;
  export type PluginData = any;
  export interface PluginInfo {
    [key: string]: any;
  }
  export interface Plugin<T = any, N = any> {
    [key: string]: any;
  }
  interface SerovalPluginParse {
    sync?(value: any, ctx: any, data?: any): any;
    async?(value: any, ctx: any, data?: any): any;
    stream?(value: any, ctx: any, data?: any): any;
    parse?(value: any, ctx: any, data?: any): any;
  }
  interface CreatePluginArgs<T = any, N = any> {
    tag: any;
    extends?: any;
    test(value: unknown): boolean;
    parse: SerovalPluginParse | ((value: any, ctx: any, data?: any) => any);
    serialize(node: any, ctx: any, data?: any): any;
    deserialize?: ((node: any, ctx: any, data?: any) => any) | undefined;
    [key: string]: any;
  }
  export function createPlugin<T = any, N = any>(def: CreatePluginArgs<T, N>): Plugin<T, N>;
  export function createStream<T = any>(): any;
  export function crossSerializeStream(
    source: any,
    options?: {
      onSerialize?: (data: any, initial: any) => any;
      onDone?: (...args: any[]) => any;
      [key: string]: any;
    },
  ): any;
  export function getCrossReferenceHeader(...args: any[]): any;
  const _default: any;
  export default _default;
}

declare module 'seroval-plugins/web' {
  export const ReadableStreamPlugin: any;
  const _default: any;
  export default _default;
}

// router-core's SSR path imports Node stream/http2 APIs. `node:stream/web`'s
// WHATWG streams mirror the DOM lib globals (present here), so alias them for
// real typing; `node:stream` / `node:http2` are Node-only, stubbed permissively.
declare module 'node:stream/web' {
  export const ReadableStream: typeof globalThis.ReadableStream;
  export type ReadableStream<R = any> = globalThis.ReadableStream<R>;
  export const WritableStream: typeof globalThis.WritableStream;
  export type WritableStream<W = any> = globalThis.WritableStream<W>;
  export const TransformStream: typeof globalThis.TransformStream;
  export type TransformStream<I = any, O = any> = globalThis.TransformStream<I, O>;
}
declare module 'node:stream' {
  export class Readable {
    [key: string]: any;
    static from(iterable: any, options?: any): Readable;
    static toWeb(stream: any, options?: any): any;
    static fromWeb(stream: any, options?: any): any;
  }
  export class Writable {
    [key: string]: any;
  }
  const _default: any;
  export default _default;
}
declare module 'node:http2' {
  export type OutgoingHttpHeaders = Record<string, any>;
  const _default: any;
  export default _default;
}

declare module 'cookie-es' {
  // `splitSetCookieString` returns a string[] that router-core iterates; a bare
  // `any` collapses the `.forEach((cookie) => ...)` callback to implicit-any.
  export function splitSetCookieString(cookiesString: string | string[]): string[];
  export function parse(str: string, options?: any): Record<string, string>;
  export function serialize(name: string, value: string, options?: any): string;
  export function parseSetCookie(setCookieValue: string, options?: any): any;
  const _default: any;
  export default _default;
}
TYPES
}

tsz_write_ts_morph_canary_stubs() {
  local output="$1"
  local fixture_dir
  fixture_dir="$(dirname "$output")"
  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<'TYPES'
declare module 'conditional-type-checks' {
  export type AssertTrue<A = any, B = any, C = any, D = any, E = any> = any;
  export const AssertTrue: any;
  export type IsExact<A = any, B = any, C = any, D = any, E = any> = any;
  export const IsExact: any;
  const __tszDefault: any;
  export default __tszDefault;
}
TYPES
  cat > "$fixture_dir/tsz-bench-globals.d.ts" <<'TYPES'
// ts-morph re-exports `code-block-writer`'s default export as the value AND type
// `CodeBlockWriter` (`writer: CodeBlockWriter`). A bare `any` default export is a
// value only, so every type-position use trips TS2749. A default-exported class
// is usable in both positions; the index signature keeps all methods `any` while
// preserving fluent chaining.
declare module 'code-block-writer' {
  export default class CodeBlockWriter {
    constructor(opts?: any);
    [key: string]: any;
  }
  export type Options = any;
}
TYPES
}
