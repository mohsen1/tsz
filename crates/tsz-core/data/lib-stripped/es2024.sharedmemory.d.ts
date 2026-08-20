/// <reference lib="es2020.bigint" />
interface Atomics {
    waitAsync(typedArray: Int32Array, index: number, value: number, timeout?: number): { async: false; value: "not-equal" | "timed-out"; } | { async: true; value: Promise<"ok" | "timed-out">; };
    waitAsync(typedArray: BigInt64Array, index: number, value: bigint, timeout?: number): { async: false; value: "not-equal" | "timed-out"; } | { async: true; value: Promise<"ok" | "timed-out">; };
}
interface SharedArrayBuffer {
    get growable(): boolean;
    get maxByteLength(): number;
    grow(newByteLength?: number): void;
}
interface SharedArrayBufferConstructor {
    new (byteLength: number, options?: { maxByteLength?: number; }): SharedArrayBuffer;
}
