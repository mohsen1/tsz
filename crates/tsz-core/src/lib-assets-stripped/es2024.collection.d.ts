/// <reference lib="es2023.collection" />
interface MapConstructor {
    groupBy<K, T>(
        items: Iterable<T>,
        keySelector: (item: T, index: number) => K,
    ): Map<K, T[]>;
}
