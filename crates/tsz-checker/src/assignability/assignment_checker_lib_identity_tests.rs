use crate::context::{CheckerOptions, ScriptTarget};
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::{
    check_multi_file_with_libs, check_source, check_source_with_libs, load_compiled_lib_files,
    load_default_lib_files, load_lib_files,
};

fn diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

#[test]
fn property_key_index_signature_accepts_well_known_symbol_object_literal_key() {
    let diagnostics = diagnostics_for(
        r#"
declare const Symbol: { readonly iterator: unique symbol };
type PropertyKey = string | number | symbol;
interface PropertyDescriptor {}
interface PropertyDescriptorMap {
    [key: PropertyKey]: PropertyDescriptor;
}
interface ThisType<T> {}
declare function defineProperties(properties: PropertyDescriptorMap & ThisType<any>): void;

defineProperties({
    [Symbol.iterator]: {},
});
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2353),
        "PropertyKey index signatures should accept well-known symbol object-literal keys, got: {diagnostics:?}"
    );
}

#[test]
fn actual_lib_property_descriptor_map_accepts_well_known_symbol_object_literal_key() {
    let libs = load_lib_files(&[
        "es5.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ]);
    let diagnostics = check_source_with_libs(
        r#"
Object.defineProperties({}, {
    [Symbol.iterator]: {
        configurable: true,
        value: function () {},
    },
});
"#,
        "test.ts",
        CheckerOptions::default(),
        &libs,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2353),
        "actual lib PropertyDescriptorMap should accept well-known symbol object-literal keys, got: {diagnostics:?}"
    );
}

#[test]
fn default_lib_property_descriptor_map_accepts_multiple_well_known_symbol_keys() {
    let libs = load_default_lib_files();
    let diagnostics = check_source_with_libs(
        r#"
Object.defineProperties({}, {
    [Symbol.iterator]: {
        configurable: true,
        value: function () {},
    },
    [Symbol.toStringTag]: {
        configurable: true,
        value: "Tagged",
    },
});
"#,
        "test.ts",
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2353),
        "default lib PropertyDescriptorMap should accept well-known symbol object-literal keys, got: {diagnostics:?}"
    );
}

const ES2020_FULL_COMPILED_LIBS: &[&str] = &[
    "lib.es2020.full.d.ts",
    "lib.es2020.d.ts",
    "lib.dom.d.ts",
    "lib.webworker.importscripts.d.ts",
    "lib.scripthost.d.ts",
    "lib.dom.iterable.d.ts",
    "lib.dom.asynciterable.d.ts",
    "lib.es2019.d.ts",
    "lib.es2020.bigint.d.ts",
    "lib.es2020.date.d.ts",
    "lib.es2020.number.d.ts",
    "lib.es2020.promise.d.ts",
    "lib.es2020.sharedmemory.d.ts",
    "lib.es2020.string.d.ts",
    "lib.es2020.symbol.wellknown.d.ts",
    "lib.es2020.intl.d.ts",
    "lib.es2015.d.ts",
    "lib.es2018.asynciterable.d.ts",
    "lib.es2018.d.ts",
    "lib.es2019.array.d.ts",
    "lib.es2019.object.d.ts",
    "lib.es2019.string.d.ts",
    "lib.es2019.symbol.d.ts",
    "lib.es2019.intl.d.ts",
    "lib.es2015.iterable.d.ts",
    "lib.es2015.symbol.d.ts",
    "lib.es2018.intl.d.ts",
    "lib.es5.d.ts",
    "lib.es2015.core.d.ts",
    "lib.es2015.collection.d.ts",
    "lib.es2015.generator.d.ts",
    "lib.es2015.promise.d.ts",
    "lib.es2015.proxy.d.ts",
    "lib.es2015.reflect.d.ts",
    "lib.es2015.symbol.wellknown.d.ts",
    "lib.es2017.d.ts",
    "lib.es2018.asyncgenerator.d.ts",
    "lib.es2018.promise.d.ts",
    "lib.es2018.regexp.d.ts",
    "lib.decorators.d.ts",
    "lib.decorators.legacy.d.ts",
    "lib.es2016.d.ts",
    "lib.es2017.arraybuffer.d.ts",
    "lib.es2017.date.d.ts",
    "lib.es2017.intl.d.ts",
    "lib.es2017.object.d.ts",
    "lib.es2017.sharedmemory.d.ts",
    "lib.es2017.string.d.ts",
    "lib.es2017.typedarrays.d.ts",
    "lib.es2016.array.include.d.ts",
    "lib.es2016.intl.d.ts",
];

#[test]
fn intl_resolved_number_format_options_uses_merged_es2020_members() {
    let libs = load_compiled_lib_files(ES2020_FULL_COMPILED_LIBS);
    let source = r#"
            const options = new Intl.NumberFormat("en-NZ").resolvedOptions();
            options.notation;
            const { notation, signDisplay } =
                new Intl.NumberFormat("en-NZ").resolvedOptions();
        "#;
    let diagnostics = check_multi_file_with_libs(
        &[("intl-options.ts", source)],
        "intl-options.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2020,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "merged Intl.ResolvedNumberFormatOptions members should be visible, got: {diagnostics:?}",
    );
}

#[test]
fn user_resolved_number_format_options_does_not_use_intl_merge_fallback() {
    let libs = load_compiled_lib_files(ES2020_FULL_COMPILED_LIBS);
    let source = r#"
            interface ResolvedNumberFormatOptions {
                foo: string;
            }
            declare const options: ResolvedNumberFormatOptions;
            options.notation;
        "#;
    let diagnostics = check_multi_file_with_libs(
        &[("user-options.ts", source)],
        "user-options.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2020,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "user-defined ResolvedNumberFormatOptions must not inherit Intl namespace merge fallback, got: {diagnostics:?}",
    );
}

#[test]
fn inferred_generic_call_recheck_skips_unresolved_conditional_object_literal_target() {
    let diagnostics = diagnostics_for(
        r#"
interface Array<T> {
    [index: number]: T;
}

type RecursivePartial<Value> = {
    [Key in keyof Value]?: Value[Key] extends Array<any>
        ? { [index: number]: RecursivePartial<Value[Key][0]> }
        : Value[Key] extends object
            ? RecursivePartial<Value[Key]>
            : Value[Key];
};

declare function assignPartial<Subject>(original: Subject, patch: RecursivePartial<Subject>): void;

const value = { o: 1, c: [{ a: 1, c: "x" }] };
assignPartial(value, { c: { 0: { a: 2, c: "y" } } });
"#,
    );

    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "inferred generic object-literal arguments should not be rechecked against unresolved conditional targets, got: {diagnostics:?}"
    );
}
