interface ObjectConstructor {
    values<T>(o: { [s: string]: T; } | ArrayLike<T>): T[];
    values(o: {}): any[];
    entries<T>(o: { [s: string]: T; } | ArrayLike<T>): [string, T][];
    entries(o: {}): [string, any][];
    getOwnPropertyDescriptors<T>(o: T): { [P in keyof T]: TypedPropertyDescriptor<T[P]>; } & { [x: string]: PropertyDescriptor; };
}
