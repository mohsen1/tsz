/// <reference lib="es2015.symbol.wellknown" />
interface WeakRef<T extends WeakKey> {
    readonly [Symbol.toStringTag]: "WeakRef";
    deref(): T | undefined;
}
interface WeakRefConstructor {
    readonly prototype: WeakRef<any>;
    new <T extends WeakKey>(target: T): WeakRef<T>;
}
declare var WeakRef: WeakRefConstructor;
interface FinalizationRegistry<T> {
    readonly [Symbol.toStringTag]: "FinalizationRegistry";
    register(target: WeakKey, heldValue: T, unregisterToken?: WeakKey): void;
    unregister(unregisterToken: WeakKey): boolean;
}
interface FinalizationRegistryConstructor {
    readonly prototype: FinalizationRegistry<any>;
    new <T>(cleanupCallback: (heldValue: T) => void): FinalizationRegistry<T>;
}
declare var FinalizationRegistry: FinalizationRegistryConstructor;
