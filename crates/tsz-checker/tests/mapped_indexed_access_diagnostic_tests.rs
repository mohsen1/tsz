use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn remapped_keyof_identity_value_accepts_iteration_member_assignment() {
    let source = r#"
type Dict<TItems extends readonly { readonly key: string }[]> = {
  [Item in TItems[number] as Item["key"]]: Item;
};

function put<TItems extends readonly { readonly key: string }[]>(
  item: TItems[number],
): Dict<TItems> {
  const result = {} as Dict<TItems>;
  const value: Dict<TItems>[keyof Dict<TItems>] = item;
  result[item.key as keyof Dict<TItems>] = item;
  void value;
  return result;
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let relation_diags: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322 || diag.code == 2345)
        .collect();

    assert!(
        relation_diags.is_empty(),
        "remapped mapped value union should accept the mapped iteration member: {diagnostics:#?}"
    );
}

#[test]
fn renamed_remapped_keyof_identity_value_accepts_iteration_member_assignment() {
    let source = r#"
type Lookup<TRows extends readonly { readonly id: string }[]> = {
  [Row in TRows[number] as Row["id"]]: Row;
};

function store<TRows extends readonly { readonly id: string }[]>(
  row: TRows[number],
): Lookup<TRows> {
  const lookup = {} as Lookup<TRows>;
  const stored: Lookup<TRows>[keyof Lookup<TRows>] = row;
  lookup[row.id as keyof Lookup<TRows>] = row;
  void stored;
  return lookup;
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let relation_diags: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322 || diag.code == 2345)
        .collect();

    assert!(
        relation_diags.is_empty(),
        "renamed remapped mapped value union should not depend on binder spelling: {diagnostics:#?}"
    );
}

#[test]
fn mapped_indexed_access_discriminated_union_reports_outer_assignment() {
    let source = r#"
type Pairs<T> = {
    [TKey in keyof T]: {
        key: TKey;
        value: T[TKey];
    };
};

type Pair<T> = Pairs<T>[keyof T];

type FooBar = {
    foo: string;
    bar: number;
};

let pair1: Pair<FooBar> = {
    key: "foo",
    value: 3
};

let pair2: Pairs<FooBar>[keyof FooBar] = {
    key: "foo",
    value: 3
};
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        2,
        "expected one TS2322 per invalid assignment, got: {diagnostics:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Pair<FooBar>")),
        "alias target should stay on the outer assignment diagnostic: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("{ key: \"foo\"; value: string; } | { key: \"bar\"; value: number; }")),
        "indexed-access target should display its evaluated union on the outer assignment: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().all(|diag| !diag
            .message_text
            .contains("Type 'number' is not assignable to type 'string'")),
        "mapped indexed access assignments should not elaborate into the selected union member's property: {ts2322:#?}"
    );
}

/// tsc loses the outer alias when a non-generic `type Foo = X[K]` reduces to
/// a single concrete type; the parameter shows the resolved form, not the
/// alias name. This mirrors the lib's
/// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` (where `WeakKeyTypes`
/// has only `object: object` in es2022) which displays as `object`.
///
/// Repro:
/// ```ts
/// interface MyKeyTypes { object: object; }
/// type MyKey = MyKeyTypes[keyof MyKeyTypes];
/// interface MockRegistry<T> { register(target: MyKey, heldValue: T): void; }
/// declare const f: MockRegistry<unknown>;
/// const s: symbol = Symbol("s");
/// f.register(s, null);
/// // tsc: Argument of type 'symbol' is not assignable to parameter of type 'object'.
/// ```
#[test]
fn indexed_access_alias_displays_resolved_form_in_call_parameter_diagnostic() {
    let source = r#"
interface MyKeyTypes { object: object; }
type MyKey = MyKeyTypes[keyof MyKeyTypes];

interface MockRegistry<T> {
    register(target: MyKey, heldValue: T): void;
}
declare const f: MockRegistry<unknown>;
const s: symbol = Symbol("s");
f.register(s, null);
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected one TS2345 for the symbol argument, got: {diagnostics:#?}"
    );
    assert!(
        ts2345[0].message_text.contains("'object'"),
        "parameter type should display as 'object' (the resolved form), not the alias name: {:?}",
        ts2345[0].message_text
    );
    assert!(
        !ts2345[0].message_text.contains("'MyKey'"),
        "outer alias should be lost in the indexed-access reduction: {:?}",
        ts2345[0].message_text
    );
}

#[test]
fn generic_key_call_through_mapped_application_preserves_indexed_return() {
    let source = r#"
interface IndexedModel {
    prop0: { value: number; tag: "prop0"; nested: { flag: boolean } };
    prop1: { value: number; tag: "prop1"; nested: { flag: boolean } };
}

type IndexedReaders<T> = { [K in keyof T]: (value: T[K]) => T[K] };

declare const model: IndexedModel;
declare const readers: IndexedReaders<IndexedModel>;

function readIndexed<K extends keyof IndexedModel>(key: K): IndexedModel[K] {
    return readers[key](model[key]);
}

interface RenamedModel {
    alpha: { value: string; nested: { flag: boolean } };
    beta: { value: boolean; nested: { flag: boolean } };
}

type RenamedReaders<Source> = { [Member in keyof Source]: (value: Source[Member]) => Source[Member] };

declare const renamedModel: RenamedModel;
declare const renamedReaders: RenamedReaders<RenamedModel>;

function readRenamed<Key extends keyof RenamedModel>(key: Key): RenamedModel[Key] {
    return renamedReaders[key](renamedModel[key]);
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert!(
        ts2322.is_empty(),
        "generic mapped-reader calls should preserve the indexed return type, got: {diagnostics:#?}"
    );
}

#[test]
fn remapped_mapped_type_constraint_indexed_access_diagnostics_match_tsc_surface() {
    let source = r#"
type Mapped2<K extends string> = { [P in K as `get${P}`]: { a: P } };

function f2<K extends string>(obj: Mapped2<K>, key: `get${K}`) {
    const x: { a: K } = obj[key];
}

type Mapped3<K extends string> = { [P in K as Uppercase<P>]: { a: P } };

function f3<K extends string>(obj: Mapped3<K>, key: Uppercase<K>) {
    const x: { a: K } = obj[key];
}

type Mapped5<K extends string> = {
  [P in K as P extends `_${string}` ? P : never]: P;
};

function f5<K extends string>(obj: Mapped5<K>, key: keyof Mapped5<K>) {
  let s: `_${string}` = obj[key];
}

type Mapped6<K extends string> = {
  [P in K as `_${P}`]: P;
};

function f6<K extends string>(obj: Mapped6<K>, key: keyof Mapped6<K>) {
  let s: `_${string}` = obj[key];
}

type Foo<T extends string> = {
    [RemappedT in T as `get${RemappedT}`]: RemappedT;
};

const get = <T extends string>(t: T, foo: Foo<T>): T => foo[`get${t}`];

type ObjectWithUnderscoredKeys<K extends string> = {
    [k in K as `_${k}`]: true;
};

function genericTest<K extends string>(objectWithUnderscoredKeys: ObjectWithUnderscoredKeys<K>, key: K) {
  const shouldBeTrue: true = objectWithUnderscoredKeys[`_${key}`];
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Mapped2<K>[`get${K}`]")),
        "remapped template index should stay deferred and report Mapped2<K>[`get${{K}}`], got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Mapped3<K>[Uppercase<K>]")),
        "intrinsic remapped index should stay deferred and report Mapped3<K>[Uppercase<K>], got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|diag| !diag.message_text.contains("keyof Mapped5<K>")),
        "filtered remapped keys should not produce a false TS2322 for f5, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Mapped6<K>[keyof Mapped6<K>]")),
        "keyof remapped mapped type should preserve the outer indexed-access surface, got: {ts2322:#?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|diag| diag.message_text.contains("Foo<T>[`get${T}`]")),
        "generic remapped alias index should preserve Foo<T>[`get${{T}}`], got: {ts2322:#?}"
    );
    assert!(
        ts2322.iter().any(|diag| {
            diag.message_text
                .contains("ObjectWithUnderscoredKeys<K>[`_${K}`]")
        }),
        "value-level template key should preserve ObjectWithUnderscoredKeys<K>[`_${{K}}`], got: {ts2322:#?}"
    );
}
