interface PromiseFulfilledResult<T> {
    status: "fulfilled";
    value: T;
}
interface PromiseRejectedResult {
    status: "rejected";
    reason: any;
}
type PromiseSettledResult<T> = PromiseFulfilledResult<T> | PromiseRejectedResult;
interface PromiseConstructor {
    allSettled<T extends readonly unknown[] | []>(values: T): Promise<{ -readonly [P in keyof T]: PromiseSettledResult<Awaited<T[P]>>; }>;
    allSettled<T>(values: Iterable<T | PromiseLike<T>>): Promise<PromiseSettledResult<Awaited<T>>[]>;
}
