use super::strict_diagnostics_for;

#[test]
fn typebox_static_array_return_diagnostics_use_structural_display() {
    let diagnostics = strict_diagnostics_for(
        r#"
export type Input = Static<typeof Input>
export const Input = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
        })
    })
})

export type Output = Static<typeof Output>
export const Output = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
            bar: Type.String(),
        })
    })
})

function problematicFunction1(ors: Input[]): Output[] {
    return ors;
}

export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Hint: unique symbol;
export declare const Kind: unique symbol;

export interface TKind {
    [Kind]: string
}
export interface TSchema extends TKind {
    [Readonly]?: string
    [Optional]?: string
    [Hint]?: string
    params: unknown[]
    static: unknown
}

export type TReadonlyOptional<T extends TSchema> = TOptional<T> & TReadonly<T>
export type TReadonly<T extends TSchema> = T & { [Readonly]: 'Readonly' }
export type TOptional<T extends TSchema> = T & { [Optional]: 'Optional' }

export interface TString extends TSchema {
    [Kind]: 'String';
    static: string;
    type: 'string';
}

export type TPropertyKey = string | number
export type TProperties = Record<TPropertyKey, TSchema>
export interface TObject<T extends TProperties = TProperties> extends TSchema {
    [Kind]: 'Object'
    static: { [K in keyof T]: Static<T[K], this['params']> }
    type: 'object'
    properties: T
}

export type Static<T extends TSchema, P extends unknown[] = []> = (T & { params: P; })['static']

declare namespace Type {
    function Object<T extends TProperties>(object: T): TObject<T>
    function String(): TString
}
"#,
    );

    assert_static_array_message(&diagnostics, "Static<typeof", "TypeBox");
}

#[test]
fn renamed_static_schema_array_return_diagnostics_use_structural_display() {
    let diagnostics = strict_diagnostics_for(
        r#"
export type Input = Resolve<typeof Input>
export const Input = Schema.Object({
    level1: Schema.Object({
        level2: Schema.Object({
            foo: Schema.String(),
        })
    })
})

export type Output = Resolve<typeof Output>
export const Output = Schema.Object({
    level1: Schema.Object({
        level2: Schema.Object({
            foo: Schema.String(),
            bar: Schema.String(),
        })
    })
})

function problematicFunction1(ors: Input[]): Output[] {
    return ors;
}

export declare const Readonly: unique symbol;
export declare const Optional: unique symbol;
export declare const Hint: unique symbol;
export declare const Kind: unique symbol;

export interface SchemaKind {
    [Kind]: string
}
export interface SchemaBase extends SchemaKind {
    [Readonly]?: string
    [Optional]?: string
    [Hint]?: string
    params: unknown[]
    static: unknown
}

export type ReadonlyOptional<T extends SchemaBase> = OptionalSchema<T> & ReadonlySchema<T>
export type ReadonlySchema<T extends SchemaBase> = T & { [Readonly]: 'Readonly' }
export type OptionalSchema<T extends SchemaBase> = T & { [Optional]: 'Optional' }

export interface StringSchema extends SchemaBase {
    [Kind]: 'String';
    static: string;
    type: 'string';
}

export type PropertyKeyName = string | number
export type PropertyMap = Record<PropertyKeyName, SchemaBase>
export interface ObjectSchema<T extends PropertyMap = PropertyMap> extends SchemaBase {
    [Kind]: 'Object'
    static: { [K in keyof T]: Resolve<T[K], this['params']> }
    type: 'object'
    properties: T
}

export type Resolve<T extends SchemaBase, P extends unknown[] = []> = (T & { params: P; })['static']

declare namespace Schema {
    function Object<T extends PropertyMap>(object: T): ObjectSchema<T>
    function String(): StringSchema
}
"#,
    );

    assert_static_array_message(&diagnostics, "Resolve<typeof", "renamed schema");
}

fn assert_static_array_message(
    diagnostics: &[crate::diagnostics::Diagnostic],
    leaked_alias: &str,
    label: &str,
) {
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    if messages.is_empty() {
        return;
    }

    assert!(
        messages.iter().any(|message| message.contains("Type '{ level1: { level2: { foo: string; }; }; }[]' is not assignable to type '{ level1: { level2: { foo: string; bar: string; }; }; }[]'.")),
        "{label} return diagnostic should use structural array displays, got: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Input[]") && !message.contains(leaked_alias)),
        "{label} structural display should not leak alias/application array names, got: {messages:?}"
    );
}
