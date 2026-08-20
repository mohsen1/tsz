interface AggregateError extends Error {
    errors: any[];
}
interface AggregateErrorConstructor {
    new (errors: Iterable<any>, message?: string): AggregateError;
    (errors: Iterable<any>, message?: string): AggregateError;
    readonly prototype: AggregateError;
}
declare var AggregateError: AggregateErrorConstructor;
interface PromiseConstructor {
    any<T extends readonly unknown[] | []>(values: T): Promise<Awaited<T[number]>>;
    any<T>(values: Iterable<T | PromiseLike<T>>): Promise<Awaited<T>>;
}
