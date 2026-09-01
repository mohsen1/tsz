type ClassMemberDecoratorContext =
    | ClassMethodDecoratorContext
    | ClassGetterDecoratorContext
    | ClassSetterDecoratorContext
    | ClassFieldDecoratorContext
    | ClassAccessorDecoratorContext;
type DecoratorContext =
    | ClassDecoratorContext
    | ClassMemberDecoratorContext;
type DecoratorMetadataObject = Record<PropertyKey, unknown> & object;
type DecoratorMetadata = typeof globalThis extends { Symbol: { readonly metadata: symbol; }; } ? DecoratorMetadataObject : DecoratorMetadataObject | undefined;
interface ClassDecoratorContext<
    Class extends abstract new (...args: any) => any = abstract new (...args: any) => any,
> {
    readonly kind: "class";
    readonly name: string | undefined;
    addInitializer(initializer: (this: Class) => void): void;
    readonly metadata: DecoratorMetadata;
}
interface ClassMethodDecoratorContext<
    This = unknown,
    Value extends (this: This, ...args: any) => any = (this: This, ...args: any) => any,
> {
    readonly kind: "method";
    readonly name: string | symbol;
    readonly static: boolean;
    readonly private: boolean;
    readonly access: {
        has(object: This): boolean;
        get(object: This): Value;
    };
    addInitializer(initializer: (this: This) => void): void;
    readonly metadata: DecoratorMetadata;
}
interface ClassGetterDecoratorContext<
    This = unknown,
    Value = unknown,
> {
    readonly kind: "getter";
    readonly name: string | symbol;
    readonly static: boolean;
    readonly private: boolean;
    readonly access: {
        has(object: This): boolean;
        get(object: This): Value;
    };
    addInitializer(initializer: (this: This) => void): void;
    readonly metadata: DecoratorMetadata;
}
interface ClassSetterDecoratorContext<
    This = unknown,
    Value = unknown,
> {
    readonly kind: "setter";
    readonly name: string | symbol;
    readonly static: boolean;
    readonly private: boolean;
    readonly access: {
        has(object: This): boolean;
        set(object: This, value: Value): void;
    };
    addInitializer(initializer: (this: This) => void): void;
    readonly metadata: DecoratorMetadata;
}
interface ClassAccessorDecoratorContext<
    This = unknown,
    Value = unknown,
> {
    readonly kind: "accessor";
    readonly name: string | symbol;
    readonly static: boolean;
    readonly private: boolean;
    readonly access: {
        has(object: This): boolean;
        get(object: This): Value;
        set(object: This, value: Value): void;
    };
    addInitializer(initializer: (this: This) => void): void;
    readonly metadata: DecoratorMetadata;
}
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}
interface ClassAccessorDecoratorResult<This, Value> {
    get?(this: This): Value;
    set?(this: This, value: Value): void;
    init?(this: This, value: Value): Value;
}
interface ClassFieldDecoratorContext<
    This = unknown,
    Value = unknown,
> {
    readonly kind: "field";
    readonly name: string | symbol;
    readonly static: boolean;
    readonly private: boolean;
    readonly access: {
        has(object: This): boolean;
        get(object: This): Value;
        set(object: This, value: Value): void;
    };
    addInitializer(initializer: (this: This) => void): void;
    readonly metadata: DecoratorMetadata;
}
