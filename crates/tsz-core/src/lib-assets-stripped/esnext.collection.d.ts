/// <reference lib="es2025.collection" />

interface Map<K, V> {
    getOrInsert(key: K, defaultValue: V): V;
    getOrInsertComputed(key: K, callback: (key: K) => V): V;
}

interface WeakMap<K extends WeakKey, V> {
    getOrInsert(key: K, defaultValue: V): V;
    getOrInsertComputed(key: K, callback: (key: K) => V): V;
}
