// Node.js lib.d.ts declarations
// These provide type definitions for Node.js global APIs

// Global process object
interface ProcessEnv {
    [key: string]: string | undefined;
}

interface ProcessVersions {
    node: string;
    v8: string;
    uv: string;
    zlib: string;
    brotli: string;
    ares: string;
    modules: string;
    nghttp2: string;
    napi: string;
    llhttp: string;
    openssl: string;
}

interface Process {
    readonly arch: string;
    readonly argv: string[];
    readonly argv0: string;
    connected: boolean;
    readonly cwd: () => string;
    readonly debugPort: number;
    readonly env: ProcessEnv;
    readonly execArgv: string[];
    readonly execPath: string;
    readonly exitCode?: number | undefined;
    readonly mainModule?: NodeModule | undefined;
    readonly pid: number;
    readonly platform: NodeJS.Platform;
    readonly ppid: number;
    readonly release: {
        name: string;
        sourceUrl?: string | undefined;
        headersUrl?: string | undefined;
        libUrl?: string | undefined;
        lts?: string | undefined;
    };
    readonly stdin: ReadableStream;
    readonly stdout: WritableStream;
    readonly stderr: WritableStream;
    readonly title: string;
    readonly version: string;
    readonly versions: ProcessVersions;

    abort(): never;
    chdir(directory: string): void;
    cpuUsage(previousValue?: CpuUsage): CpuUsage;
    disconnect(): void;
    emitWarning(warning: string | Error, name?: string, ctor?: Function): void;
    exit(code?: number): never;
    getegid?(): number;
    geteuid?(): number;
    getgid?(): number;
    getgroups?(): number[];
    getuid?(): number;
    hasUncaughtExceptionCaptureCallback(): boolean;
    hrtime(time?: [number, number]): [number, number];
    kill(pid: number, signal?: string | number): boolean;
    memoryUsage(): MemoryUsage;
    nextTick(callback: Function, ...args: any[]): void;
    resourceUsage(): ResourceUsage;
    send?(message: any, sendHandle?: any, options?: { swallowErrors?: boolean }, callback?: (error: Error | null) => void): boolean;
    setegid?(id: number | string): void;
    seteuid?(id: number | string): void;
    setgid?(id: number | string): void;
    setgroups?(groups: ReadonlyArray<string | number>): void;
    setuid?(id: number | string): void;
    setUncaughtExceptionCaptureCallback(fn: ((err: Error) => void) | null): void;
    umask(mask?: number): number;
    uptime(): number;

    on(event: 'beforeExit', listener: (code: number) => void): this;
    on(event: 'disconnect', listener: () => void): this;
    on(event: 'exit', listener: (code: number) => void): this;
    on(event: 'message', listener: (message: any, sendHandle: any) => void): this;
    on(event: 'uncaughtException', listener: (error: Error) => void): this;
    on(event: 'unhandledRejection', listener: (reason: any, promise: Promise<any>) => void): this;
    on(event: 'warning', listener: (warning: Error) => void): this;
    on(event: string, listener: (...args: any[]) => void): this;

    once(event: 'beforeExit', listener: (code: number) => void): this;
    once(event: 'disconnect', listener: () => void): this;
    once(event: 'exit', listener: (code: number) => void): this;
    once(event: string, listener: (...args: any[]) => void): this;

    addListener(event: string, listener: (...args: any[]) => void): this;
    removeListener(event: string, listener: (...args: any[]) => void): this;
    removeAllListeners(event?: string): this;
    off(event: string, listener: (...args: any[]) => void): this;
    emit(event: string, ...args: any[]): boolean;
    listeners(event: string): Function[];
    listenerCount(event: string): number;
    prependListener(event: string, listener: (...args: any[]) => void): this;
    prependOnceListener(event: string, listener: (...args: any[]) => void): this;
}

interface CpuUsage {
    user: number;
    system: number;
}

interface MemoryUsage {
    rss: number;
    heapTotal: number;
    heapUsed: number;
    external: number;
    arrayBuffers: number;
}

interface ResourceUsage {
    userCPUTime: number;
    systemCPUTime: number;
    maxRSS: number;
    sharedMemorySize: number;
    unsharedDataSize: number;
    unsharedStackSize: number;
    minorPageFault: number;
    majorPageFault: number;
    swappedOut: number;
    fsRead: number;
    fsWrite: number;
    ipcSent: number;
    ipcReceived: number;
    signalsCount: number;
    voluntaryContextSwitches: number;
    involuntaryContextSwitches: number;
}

declare var process: Process;

// Buffer
type BufferEncoding = 'ascii' | 'utf8' | 'utf-8' | 'utf16le' | 'ucs2' | 'ucs-2' | 'base64' | 'base64url' | 'latin1' | 'binary' | 'hex';

interface BufferConstructor {
    from(arrayBuffer: WithImplicitCoercion<ArrayBuffer | SharedArrayBuffer>, byteOffset?: number, length?: number): Buffer;
    from(data: Uint8Array | ReadonlyArray<number>): Buffer;
    from(data: WithImplicitCoercion<Uint8Array | ReadonlyArray<number> | string>): Buffer;
    from(str: WithImplicitCoercion<string> | { [Symbol.toPrimitive](hint: 'string'): string }, encoding?: BufferEncoding): Buffer;
    of(...items: number[]): Buffer;
    isBuffer(obj: any): obj is Buffer;
    isEncoding(encoding: string): encoding is BufferEncoding;
    byteLength(string: string | NodeJS.ArrayBufferView | ArrayBuffer | SharedArrayBuffer, encoding?: BufferEncoding): number;
    concat(list: ReadonlyArray<Uint8Array>, totalLength?: number): Buffer;
    compare(buf1: Uint8Array, buf2: Uint8Array): -1 | 0 | 1;
    alloc(size: number, fill?: string | Uint8Array | number, encoding?: BufferEncoding): Buffer;
    allocUnsafe(size: number): Buffer;
    allocUnsafeSlow(size: number): Buffer;
    readonly poolSize: number;
}

interface Buffer extends Uint8Array {
    constructor: BufferConstructor;
    write(string: string, encoding?: BufferEncoding): number;
    write(string: string, offset: number, encoding?: BufferEncoding): number;
    write(string: string, offset: number, length: number, encoding?: BufferEncoding): number;
    toString(encoding?: BufferEncoding, start?: number, end?: number): string;
    toJSON(): { type: 'Buffer'; data: number[] };
    equals(otherBuffer: Uint8Array): boolean;
    compare(target: Uint8Array, targetStart?: number, targetEnd?: number, sourceStart?: number, sourceEnd?: number): -1 | 0 | 1;
    copy(target: Uint8Array, targetStart?: number, sourceStart?: number, sourceEnd?: number): number;
    slice(start?: number, end?: number): Buffer;
    subarray(start?: number, end?: number): Buffer;
    writeBigInt64BE(value: bigint, offset?: number): number;
    writeBigInt64LE(value: bigint, offset?: number): number;
    writeBigUInt64BE(value: bigint, offset?: number): number;
    writeBigUInt64LE(value: bigint, offset?: number): number;
    writeDoubleBE(value: number, offset?: number): number;
    writeDoubleLE(value: number, offset?: number): number;
    writeFloatBE(value: number, offset?: number): number;
    writeFloatLE(value: number, offset?: number): number;
    writeInt8(value: number, offset?: number): number;
    writeInt16BE(value: number, offset?: number): number;
    writeInt16LE(value: number, offset?: number): number;
    writeInt32BE(value: number, offset?: number): number;
    writeInt32LE(value: number, offset?: number): number;
    writeUInt8(value: number, offset?: number): number;
    writeUInt16BE(value: number, offset?: number): number;
    writeUInt16LE(value: number, offset?: number): number;
    writeUInt32BE(value: number, offset?: number): number;
    writeUInt32LE(value: number, offset?: number): number;
    readBigInt64BE(offset?: number): bigint;
    readBigInt64LE(offset?: number): bigint;
    readBigUInt64BE(offset?: number): bigint;
    readBigUInt64LE(offset?: number): bigint;
    readDoubleBE(offset?: number): number;
    readDoubleLE(offset?: number): number;
    readFloatBE(offset?: number): number;
    readFloatLE(offset?: number): number;
    readInt8(offset?: number): number;
    readInt16BE(offset?: number): number;
    readInt16LE(offset?: number): number;
    readInt32BE(offset?: number): number;
    readInt32LE(offset?: number): number;
    readUInt8(offset?: number): number;
    readUInt16BE(offset?: number): number;
    readUInt16LE(offset?: number): number;
    readUInt32BE(offset?: number): number;
    readUInt32LE(offset?: number): number;
    reverse(): this;
    swap16(): Buffer;
    swap32(): Buffer;
    swap64(): Buffer;
}

declare var Buffer: BufferConstructor;

type WithImplicitCoercion<T> = T | { valueOf(): T };

// NodeJS namespace for platform types
declare namespace NodeJS {
    type Platform = 'aix' | 'android' | 'darwin' | 'freebsd' | 'haiku' | 'linux' | 'openbsd' | 'sunos' | 'win32' | 'cygwin' | 'netbsd';

    interface ArrayBufferView {
        buffer: ArrayBuffer;
        byteLength: number;
        byteOffset: number;
    }

    interface ErrnoException extends Error {
        errno?: number | undefined;
        code?: string | undefined;
        path?: string | undefined;
        syscall?: string | undefined;
    }

    interface Timer {
        hasRef(): boolean;
        ref(): this;
        refresh(): this;
        unref(): this;
        [Symbol.toPrimitive](): number;
    }

    interface Immediate {
        hasRef(): boolean;
        ref(): this;
        unref(): this;
        _onImmediate: Function;
    }

    interface Timeout {
        hasRef(): boolean;
        ref(): this;
        refresh(): this;
        unref(): this;
        [Symbol.toPrimitive](): number;
    }
}

// Module
interface NodeModule {
    exports: any;
    require: NodeRequire;
    id: string;
    filename: string;
    loaded: boolean;
    parent: NodeModule | null;
    children: NodeModule[];
    path: string;
    paths: string[];
}

interface NodeRequire {
    (id: string): any;
    resolve: RequireResolve;
    cache: Dict<NodeModule>;
    extensions: NodeExtensions;
    main: NodeModule | undefined;
}

interface RequireResolve {
    (id: string, options?: { paths?: string[] }): string;
    paths(request: string): string[] | null;
}

interface NodeExtensions {
    '.js': (m: NodeModule, filename: string) => any;
    '.json': (m: NodeModule, filename: string) => any;
    '.node': (m: NodeModule, filename: string) => any;
    [ext: string]: (m: NodeModule, filename: string) => any;
}

interface Dict<T> {
    [key: string]: T | undefined;
}

// Global require and module
declare var require: NodeRequire;
declare var module: NodeModule;
declare var exports: any;
declare var __filename: string;
declare var __dirname: string;

// Console extensions for Node.js
interface Console {
    Console: new (stdout: WritableStream, stderr?: WritableStream, ignoreErrors?: boolean) => Console;
    profile(label?: string): void;
    profileEnd(label?: string): void;
    timeStamp(label?: string): void;
}

// Streams
interface ReadableStream {
    readable: boolean;
    read(size?: number): string | Buffer | null;
    setEncoding(encoding: BufferEncoding): this;
    pause(): this;
    resume(): this;
    isPaused(): boolean;
    pipe<T extends WritableStream>(destination: T, options?: { end?: boolean }): T;
    unpipe(destination?: WritableStream): this;
    unshift(chunk: string | Uint8Array, encoding?: BufferEncoding): void;
    wrap(oldStream: ReadableStream): this;
    destroy(error?: Error): this;

    on(event: 'close', listener: () => void): this;
    on(event: 'data', listener: (chunk: any) => void): this;
    on(event: 'end', listener: () => void): this;
    on(event: 'error', listener: (err: Error) => void): this;
    on(event: 'pause', listener: () => void): this;
    on(event: 'readable', listener: () => void): this;
    on(event: 'resume', listener: () => void): this;
    on(event: string, listener: (...args: any[]) => void): this;
}

interface WritableStream {
    writable: boolean;
    write(buffer: Uint8Array | string, cb?: (err?: Error | null) => void): boolean;
    write(str: string, encoding?: BufferEncoding, cb?: (err?: Error | null) => void): boolean;
    end(cb?: () => void): this;
    end(data: string | Uint8Array, cb?: () => void): this;
    end(str: string, encoding?: BufferEncoding, cb?: () => void): this;
    destroy(error?: Error): this;

    on(event: 'close', listener: () => void): this;
    on(event: 'drain', listener: () => void): this;
    on(event: 'error', listener: (err: Error) => void): this;
    on(event: 'finish', listener: () => void): this;
    on(event: 'pipe', listener: (src: ReadableStream) => void): this;
    on(event: 'unpipe', listener: (src: ReadableStream) => void): this;
    on(event: string, listener: (...args: any[]) => void): this;
}

// Timers
declare function setTimeout<TArgs extends any[]>(callback: (...args: TArgs) => void, ms?: number, ...args: TArgs): NodeJS.Timeout;
declare function clearTimeout(timeoutId: NodeJS.Timeout | undefined): void;
declare function setInterval<TArgs extends any[]>(callback: (...args: TArgs) => void, ms?: number, ...args: TArgs): NodeJS.Timer;
declare function clearInterval(intervalId: NodeJS.Timer | undefined): void;
declare function setImmediate<TArgs extends any[]>(callback: (...args: TArgs) => void, ...args: TArgs): NodeJS.Immediate;
declare function clearImmediate(immediateId: NodeJS.Immediate | undefined): void;

// URL (Node.js version)
interface URL {
    hash: string;
    host: string;
    hostname: string;
    href: string;
    readonly origin: string;
    password: string;
    pathname: string;
    port: string;
    protocol: string;
    search: string;
    readonly searchParams: URLSearchParams;
    username: string;
    toString(): string;
    toJSON(): string;
}

interface URLSearchParams {
    append(name: string, value: string): void;
    delete(name: string): void;
    entries(): IterableIterator<[string, string]>;
    forEach(callback: (value: string, name: string, searchParams: URLSearchParams) => void): void;
    get(name: string): string | null;
    getAll(name: string): string[];
    has(name: string): boolean;
    keys(): IterableIterator<string>;
    set(name: string, value: string): void;
    sort(): void;
    toString(): string;
    values(): IterableIterator<string>;
    [Symbol.iterator](): IterableIterator<[string, string]>;
}

declare var URL: {
    prototype: URL;
    new(input: string, base?: string | URL): URL;
    createObjectURL(blob: Blob): string;
    revokeObjectURL(url: string): void;
};

declare var URLSearchParams: {
    prototype: URLSearchParams;
    new(init?: string[][] | Record<string, string> | string | URLSearchParams): URLSearchParams;
};

// TextEncoder/TextDecoder
interface TextEncoder {
    readonly encoding: string;
    encode(input?: string): Uint8Array;
    encodeInto(src: string, dest: Uint8Array): { read: number; written: number };
}

declare var TextEncoder: {
    prototype: TextEncoder;
    new(): TextEncoder;
};

interface TextDecoder {
    readonly encoding: string;
    readonly fatal: boolean;
    readonly ignoreBOM: boolean;
    decode(input?: BufferSource, options?: { stream?: boolean }): string;
}

declare var TextDecoder: {
    prototype: TextDecoder;
    new(label?: string, options?: { fatal?: boolean; ignoreBOM?: boolean }): TextDecoder;
};

// AbortController (Node.js version)
interface AbortController {
    readonly signal: AbortSignal;
    abort(reason?: any): void;
}

declare var AbortController: {
    prototype: AbortController;
    new(): AbortController;
};

interface AbortSignal {
    readonly aborted: boolean;
    readonly reason: any;
    throwIfAborted(): void;
    addEventListener(type: 'abort', listener: (this: AbortSignal, ev: Event) => any, options?: boolean | AddEventListenerOptions): void;
    removeEventListener(type: 'abort', listener: (this: AbortSignal, ev: Event) => any, options?: boolean | EventListenerOptions): void;
}

interface AddEventListenerOptions {
    once?: boolean;
    passive?: boolean;
    signal?: AbortSignal;
    capture?: boolean;
}

interface EventListenerOptions {
    capture?: boolean;
}

declare var AbortSignal: {
    prototype: AbortSignal;
    abort(reason?: any): AbortSignal;
    timeout(milliseconds: number): AbortSignal;
    any(signals: AbortSignal[]): AbortSignal;
};

// Event (basic Node.js compatible)
interface Event {
    readonly type: string;
    readonly target: EventTarget | null;
    readonly currentTarget: EventTarget | null;
    readonly eventPhase: number;
    readonly bubbles: boolean;
    readonly cancelable: boolean;
    readonly defaultPrevented: boolean;
    readonly composed: boolean;
    readonly timeStamp: number;
    readonly isTrusted: boolean;
    composedPath(): EventTarget[];
    preventDefault(): void;
    stopImmediatePropagation(): void;
    stopPropagation(): void;
}

interface EventTarget {
    addEventListener(type: string, listener: EventListenerOrEventListenerObject | null, options?: boolean | AddEventListenerOptions): void;
    dispatchEvent(event: Event): boolean;
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject | null, options?: boolean | EventListenerOptions): void;
}

type EventListenerOrEventListenerObject = EventListener | EventListenerObject;

interface EventListener {
    (evt: Event): void;
}

interface EventListenerObject {
    handleEvent(object: Event): void;
}

// queueMicrotask
declare function queueMicrotask(callback: () => void): void;

// structuredClone
declare function structuredClone<T>(value: T, options?: { transfer?: Transferable[] }): T;

// atob/btoa
declare function atob(data: string): string;
declare function btoa(data: string): string;

// fetch (available in Node 18+)
declare function fetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response>;

// Performance (Node.js version)
interface Performance {
    now(): number;
    timeOrigin: number;
    mark(name: string, options?: PerformanceMarkOptions): PerformanceMark;
    measure(name: string, startOrMeasureOptions?: string | PerformanceMeasureOptions, endMark?: string): PerformanceMeasure;
    clearMarks(name?: string): void;
    clearMeasures(name?: string): void;
    getEntries(): PerformanceEntryList;
    getEntriesByName(name: string, type?: string): PerformanceEntryList;
    getEntriesByType(type: string): PerformanceEntryList;
    toJSON(): any;
}

interface PerformanceEntry {
    readonly duration: number;
    readonly entryType: string;
    readonly name: string;
    readonly startTime: number;
    toJSON(): any;
}

interface PerformanceMark extends PerformanceEntry {
    readonly detail: any;
}

interface PerformanceMeasure extends PerformanceEntry {
    readonly detail: any;
}

interface PerformanceMarkOptions {
    detail?: any;
    startTime?: number;
}

interface PerformanceMeasureOptions {
    detail?: any;
    start?: string | number;
    duration?: number;
    end?: string | number;
}

type PerformanceEntryList = PerformanceEntry[];

declare var performance: Performance;

// Crypto (Node.js version)
interface Crypto {
    readonly subtle: SubtleCrypto;
    getRandomValues<T extends ArrayBufferView | null>(array: T): T;
    randomUUID(): string;
}

interface SubtleCrypto {
    decrypt(algorithm: AlgorithmIdentifier, key: CryptoKey, data: BufferSource): Promise<ArrayBuffer>;
    digest(algorithm: AlgorithmIdentifier, data: BufferSource): Promise<ArrayBuffer>;
    encrypt(algorithm: AlgorithmIdentifier, key: CryptoKey, data: BufferSource): Promise<ArrayBuffer>;
    sign(algorithm: AlgorithmIdentifier, key: CryptoKey, data: BufferSource): Promise<ArrayBuffer>;
    verify(algorithm: AlgorithmIdentifier, key: CryptoKey, signature: BufferSource, data: BufferSource): Promise<boolean>;
}

type AlgorithmIdentifier = string | Algorithm;

interface Algorithm {
    name: string;
}

interface CryptoKey {
    readonly algorithm: KeyAlgorithm;
    readonly extractable: boolean;
    readonly type: KeyType;
    readonly usages: KeyUsage[];
}

interface KeyAlgorithm {
    name: string;
}

type KeyType = "private" | "public" | "secret";
type KeyUsage = "decrypt" | "deriveBits" | "deriveKey" | "encrypt" | "sign" | "unwrapKey" | "verify" | "wrapKey";

type BufferSource = ArrayBufferView | ArrayBuffer;
type Transferable = ArrayBuffer;

declare var crypto: Crypto;

// Blob and File (Node.js 18+ versions)
interface Blob {
    readonly size: number;
    readonly type: string;
    arrayBuffer(): Promise<ArrayBuffer>;
    slice(start?: number, end?: number, contentType?: string): Blob;
    stream(): ReadableStream;
    text(): Promise<string>;
}

declare var Blob: {
    prototype: Blob;
    new(blobParts?: BlobPart[], options?: BlobPropertyBag): Blob;
};

type BlobPart = BufferSource | Blob | string;

interface BlobPropertyBag {
    endings?: "native" | "transparent";
    type?: string;
}

interface File extends Blob {
    readonly lastModified: number;
    readonly name: string;
    readonly webkitRelativePath: string;
}

declare var File: {
    prototype: File;
    new(fileBits: BlobPart[], fileName: string, options?: FilePropertyBag): File;
};

interface FilePropertyBag extends BlobPropertyBag {
    lastModified?: number;
}

// FormData (Node.js 18+ versions)
interface FormData {
    append(name: string, value: string | Blob, fileName?: string): void;
    delete(name: string): void;
    get(name: string): FormDataEntryValue | null;
    getAll(name: string): FormDataEntryValue[];
    has(name: string): boolean;
    set(name: string, value: string | Blob, fileName?: string): void;
    entries(): IterableIterator<[string, FormDataEntryValue]>;
    keys(): IterableIterator<string>;
    values(): IterableIterator<FormDataEntryValue>;
    forEach(callbackfn: (value: FormDataEntryValue, key: string, parent: FormData) => void, thisArg?: any): void;
}

declare var FormData: {
    prototype: FormData;
    new(): FormData;
};

type FormDataEntryValue = File | string;

// Headers
interface Headers {
    append(name: string, value: string): void;
    delete(name: string): void;
    get(name: string): string | null;
    has(name: string): boolean;
    set(name: string, value: string): void;
    entries(): IterableIterator<[string, string]>;
    keys(): IterableIterator<string>;
    values(): IterableIterator<string>;
    forEach(callbackfn: (value: string, key: string, parent: Headers) => void, thisArg?: any): void;
}

declare var Headers: {
    prototype: Headers;
    new(init?: HeadersInit): Headers;
};

type HeadersInit = Headers | string[][] | Record<string, string>;

// Request and Response
interface Request {
    readonly body: ReadableStream | null;
    readonly bodyUsed: boolean;
    readonly cache: RequestCache;
    readonly credentials: RequestCredentials;
    readonly destination: RequestDestination;
    readonly headers: Headers;
    readonly integrity: string;
    readonly keepalive: boolean;
    readonly method: string;
    readonly mode: RequestMode;
    readonly redirect: RequestRedirect;
    readonly referrer: string;
    readonly referrerPolicy: ReferrerPolicy;
    readonly signal: AbortSignal;
    readonly url: string;
    clone(): Request;
    arrayBuffer(): Promise<ArrayBuffer>;
    blob(): Promise<Blob>;
    formData(): Promise<FormData>;
    json(): Promise<any>;
    text(): Promise<string>;
}

declare var Request: {
    prototype: Request;
    new(input: RequestInfo | URL, init?: RequestInit): Request;
};

type RequestInfo = Request | string;
type RequestCache = "default" | "force-cache" | "no-cache" | "no-store" | "only-if-cached" | "reload";
type RequestCredentials = "include" | "omit" | "same-origin";
type RequestDestination = "" | "audio" | "document" | "embed" | "font" | "frame" | "iframe" | "image" | "manifest" | "object" | "report" | "script" | "style" | "track" | "video" | "worker" | "xslt";
type RequestMode = "cors" | "navigate" | "no-cors" | "same-origin";
type RequestRedirect = "error" | "follow" | "manual";
type ReferrerPolicy = "" | "no-referrer" | "no-referrer-when-downgrade" | "origin" | "origin-when-cross-origin" | "same-origin" | "strict-origin" | "strict-origin-when-cross-origin" | "unsafe-url";

interface RequestInit {
    body?: BodyInit | null;
    cache?: RequestCache;
    credentials?: RequestCredentials;
    headers?: HeadersInit;
    integrity?: string;
    keepalive?: boolean;
    method?: string;
    mode?: RequestMode;
    redirect?: RequestRedirect;
    referrer?: string;
    referrerPolicy?: ReferrerPolicy;
    signal?: AbortSignal | null;
}

type BodyInit = ReadableStream | Blob | BufferSource | FormData | URLSearchParams | string;

interface Response {
    readonly body: ReadableStream | null;
    readonly bodyUsed: boolean;
    readonly headers: Headers;
    readonly ok: boolean;
    readonly redirected: boolean;
    readonly status: number;
    readonly statusText: string;
    readonly type: ResponseType;
    readonly url: string;
    clone(): Response;
    arrayBuffer(): Promise<ArrayBuffer>;
    blob(): Promise<Blob>;
    formData(): Promise<FormData>;
    json(): Promise<any>;
    text(): Promise<string>;
}

declare var Response: {
    prototype: Response;
    new(body?: BodyInit | null, init?: ResponseInit): Response;
    error(): Response;
    json(data: any, init?: ResponseInit): Response;
    redirect(url: string | URL, status?: number): Response;
};

type ResponseType = "basic" | "cors" | "default" | "error" | "opaque" | "opaqueredirect";

interface ResponseInit {
    headers?: HeadersInit;
    status?: number;
    statusText?: string;
}

// Placeholder for SharedArrayBuffer
interface SharedArrayBuffer {
    readonly byteLength: number;
    slice(begin: number, end?: number): SharedArrayBuffer;
}

declare var SharedArrayBuffer: {
    prototype: SharedArrayBuffer;
    new(byteLength: number): SharedArrayBuffer;
};
