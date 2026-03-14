interface ArrayBuffer {
    get maxByteLength(): number;
    get resizable(): boolean;
    resize(newByteLength?: number): void;
    get detached(): boolean;
    transfer(newByteLength?: number): ArrayBuffer;
    transferToFixedLength(newByteLength?: number): ArrayBuffer;
}
interface ArrayBufferConstructor {
    new (byteLength: number, options?: { maxByteLength?: number; }): ArrayBuffer;
}
