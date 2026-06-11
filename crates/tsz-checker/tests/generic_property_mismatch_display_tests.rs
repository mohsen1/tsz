use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, check_source_diagnostics, diagnostic_code_messages};
use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

fn ts2322_diagnostic(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one TS2322 diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related_messages(diagnostic: &Diagnostic) -> Vec<&str> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.as_str())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related_messages(diagnostic)
        .iter()
        .any(|message| message.contains(expected))
}

fn check_es2015(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    ))
}

#[test]
fn generic_class_instance_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
class Animal {
  name: string = "";
}

class Dog extends Animal {
  bark(): void {}
}

interface Box<T> {
  value: T;
}

declare let animalBox: Box<Animal>;
declare let dogBox: Box<Dog>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<Animal>' is not assignable to type 'Box<Dog>'."),
        "expected generic class instance display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type 'Animal' but required in type 'Dog'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
    assert!(
        !diagnostic.message_text.contains("typeof Animal")
            && !diagnostic.message_text.contains("typeof Dog"),
        "class instance type arguments must not display as constructor types, got {diagnostic:#?}"
    );
    assert!(
        !has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "generic wrapper property should not hide the nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_interface_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Animal { name: string; }
interface Dog extends Animal { bark(): void; }
interface Box<T> { value: T; }

declare let animalBox: Box<Animal>;
declare let dogBox: Box<Dog>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<Animal>' is not assignable to type 'Box<Dog>'."),
        "expected generic interface display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type 'Animal' but required in type 'Dog'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
    assert!(
        !has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "generic wrapper property should not hide the nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_object_property_mismatch_flattens_to_missing_member() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let animalBox: Box<{ name: string }>;
declare let dogBox: Box<{ name: string; bark(): void }>;

dogBox = animalBox;
"#,
    );

    assert!(
        diagnostic.message_text.contains(
            "Type 'Box<{ name: string; }>' is not assignable to type 'Box<{ name: string; bark(): void; }>'."
        ),
        "expected generic object display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'bark' is missing in type '{ name: string; }' but required in type '{ name: string; bark(): void; }'."
        ),
        "expected nested missing member, got {diagnostic:#?}"
    );
}

#[test]
fn generic_object_property_mismatch_preserves_multiple_missing_properties() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let emptyBox: Box<{}>;
declare let targetBox: Box<{ a: string; b: number }>;

targetBox = emptyBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<{}>' is not assignable to type 'Box<{ a: string; b: number; }>'."),
        "expected generic object display, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '{}' is missing the following properties from type '{ a: string; b: number; }': a, b"
        ),
        "expected nested multiple-missing-property message, got {diagnostic:#?}"
    );
}

#[test]
fn generic_primitive_property_mismatch_surfaces_type_arg_directly() {
    // tsc elaborates same-generic applications via type-argument comparison,
    // skipping the intermediate "Types of property 'P' are incompatible." line.
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }

declare let numberBox: Box<number>;
declare let stringBox: Box<string>;

stringBox = numberBox;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<number>' is not assignable to type 'Box<string>'."),
        "expected generic primitive display, got {diagnostic:#?}"
    );
    // tsc skips the "Types of property" wrapper for same-generic type-argument mismatches.
    assert!(
        !has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "same-generic type-arg mismatch must NOT show property-path wrapper, got {diagnostic:#?}"
    );
    // The inner type-argument mismatch is nested directly under the outer line.
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "expected type-argument mismatch nested directly, got {diagnostic:#?}"
    );
    // The leaf must use the property types, never the enclosing application.
    assert!(
        !has_related(
            &diagnostic,
            "Type 'Box<number>' is not assignable to type 'string'."
        ),
        "must not render the enclosing application as the leaf source, got {diagnostic:#?}"
    );
}

#[test]
fn generic_primitive_property_mismatch_is_type_parameter_name_independent() {
    // The same type-argument mismatch must appear regardless of the type-parameter
    // spelling, proving the fix is structural and not hardcoded to any name.
    for type_param in ["T", "K", "Value", "Element"] {
        let source = format!(
            "interface Box<{type_param}> {{ value: {type_param}; }}\n\
             declare let numberBox: Box<number>;\n\
             declare let stringBox: Box<string>;\n\
             stringBox = numberBox;\n"
        );
        let diagnostic = ts2322_diagnostic(&source);
        assert!(
            !has_related(&diagnostic, "Types of property 'value' are incompatible."),
            "same-generic type-arg mismatch must NOT show property-path wrapper for param '{type_param}', got {diagnostic:#?}"
        );
        assert!(
            has_related(
                &diagnostic,
                "Type 'number' is not assignable to type 'string'."
            ),
            "expected type-argument mismatch for param '{type_param}', got {diagnostic:#?}"
        );
    }
}

#[test]
fn generic_primitive_property_mismatch_is_property_name_independent() {
    // The skip applies regardless of what the property is named inside the generic.
    for prop_name in ["value", "item", "data", "inner"] {
        let source = format!(
            "interface Container<T> {{ {prop_name}: T; }}\n\
             declare let n: Container<number>;\n\
             declare let s: Container<string>;\n\
             s = n;\n"
        );
        let diagnostic = ts2322_diagnostic(&source);
        assert!(
            !has_related(
                &diagnostic,
                &format!("Types of property '{prop_name}' are incompatible.")
            ),
            "same-generic type-arg mismatch must NOT show property-path wrapper for property '{prop_name}', got {diagnostic:#?}"
        );
        assert!(
            has_related(
                &diagnostic,
                "Type 'number' is not assignable to type 'string'."
            ),
            "expected type-argument mismatch for property '{prop_name}', got {diagnostic:#?}"
        );
    }
}

#[test]
fn generic_mapped_property_mismatch_elaborates_structurally() {
    // A homomorphic mapped-type alias (`Wrap<T> = { [K in keyof T]: T[K] }`) is
    // NOT a nominal generic reference: even though it is spelled as a generic
    // application, tsc elaborates the mismatch *structurally* — it keeps the
    // `Types of property 'a' are incompatible.` wrapper and then the leaf —
    // rather than collapsing to a single covariant type-argument line the way a
    // class/interface reference (`Box<number>`) does. Verified against tsc:
    //   Type 'Wrap<{ a: number; }>' is not assignable to type 'Wrap<{ a: string; }>'.
    //     Types of property 'a' are incompatible.
    //       Type 'number' is not assignable to type 'string'.
    let diagnostic = ts2322_diagnostic(
        r#"
type Wrap<T> = { [K in keyof T]: T[K] };

declare let source: Wrap<{ a: number }>;
declare let target: Wrap<{ a: string }>;

target = source;
"#,
    );

    assert!(
        has_related(&diagnostic, "Types of property 'a' are incompatible."),
        "mapped-alias mismatch must keep the structural property wrapper, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "expected the leaf relation beneath the property wrapper, got {diagnostic:#?}"
    );
}

#[test]
fn generic_multi_argument_property_mismatch_surfaces_type_arg_directly() {
    // Multi-argument same-generic: tsc still skips the "Types of property" wrapper.
    let diagnostic = ts2322_diagnostic(
        r#"
interface Pair<A, B> { a: A; b: B; }

declare let source: Pair<number, number>;
declare let target: Pair<string, number>;

target = source;
"#,
    );

    assert!(
        !has_related(&diagnostic, "Types of property 'a' are incompatible."),
        "same-generic multi-arg mismatch must NOT show property-path wrapper, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "expected type-argument mismatch nested directly for multi-arg generic, got {diagnostic:#?}"
    );
}

#[test]
fn generic_class_property_mismatch_surfaces_type_arg_directly() {
    // Exact repro from issue #11778: class Box<T> { v!: T }.
    // tsc: "Type 'Box<number>' is not assignable to type 'Box<string>'.\n  Type 'number' is not assignable to type 'string'."
    // (no intermediate "Types of property 'v' are incompatible." line)
    let diagnostic = ts2322_diagnostic(
        r#"
class Box<T> { v!: T; }
declare const b: Box<number>;
const n: Box<string> = b;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type 'Box<number>' is not assignable to type 'Box<string>'."),
        "expected class generic display, got {diagnostic:#?}"
    );
    assert!(
        !has_related(&diagnostic, "Types of property 'v' are incompatible."),
        "same-generic class mismatch must NOT show property-path wrapper, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'number' is not assignable to type 'string'."
        ),
        "expected type-argument mismatch nested directly for class generic, got {diagnostic:#?}"
    );
}

#[test]
fn different_generic_types_keep_property_path() {
    // Negative case: when source and target are DIFFERENT generic types,
    // structural property elaboration must still appear.
    let diagnostic = ts2322_diagnostic(
        r#"
interface Box<T> { value: T; }
interface Bag<T> { value: T; }

declare let numBox: Box<number>;
declare let strBag: Bag<string>;

strBag = numBox;
"#,
    );

    assert!(
        has_related(&diagnostic, "Types of property 'value' are incompatible."),
        "different-generic mismatch must still show property-path elaboration, got {diagnostic:#?}"
    );
}

#[test]
fn direct_object_property_mismatch_keeps_property_path() {
    let diagnostic = ts2322_diagnostic(
        r#"
declare let source: { p: {} };
declare let target: { p: { a: string } };

target = source;
"#,
    );

    assert!(
        diagnostic
            .message_text
            .contains("Type '{ p: {}; }' is not assignable to type '{ p: { a: string; }; }'."),
        "expected direct object mismatch display, got {diagnostic:#?}"
    );
    assert!(
        has_related(&diagnostic, "Types of property 'p' are incompatible."),
        "non-generic object property mismatch should keep property path, got {diagnostic:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Property 'a' is missing in type '{}' but required in type '{ a: string; }'."
        ),
        "expected nested missing property under direct property path, got {diagnostic:#?}"
    );
}

#[test]
fn generic_class_property_array_display_uses_owner_type_arg() {
    let diagnostics = check_es2015(
        r#"
class MyList<T> {
    public size: number;
    public data: T[];
    constructor(n: number) {
        this.size = n;
        this.data = [] as any;
    }
    public clone() {
        return new MyList<T>(this.size);
    }
}
declare let a: MyList<string>;
var d: MyList<number> = a.clone();
"#,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("MyList<string>")),
        "expected generic class display to recover T from T[] member declaration, got: {messages:#?}",
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("MyList<string[]>")),
        "generic class display should not use the raw T[] property type as the owner argument: {messages:#?}",
    );
}

#[test]
fn construct_only_generic_interface_display_keeps_type_arg() {
    let diagnostics = check_es2015(
        r#"
interface I1<T> { new (arg: T): object };
function f2<T>(args: T) {
    var v1!: { [index: string]: I1<T> };
    var v2 = v1['test'];
    var y = v2(args);
}
"#,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2348)
        .map(|(_, message)| message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Value of type 'I1<T>' is not callable")),
        "expected TS2348 to preserve the construct-only interface application, got: {messages:#?}",
    );
}

/// Type arguments in instance displays must be the application's ACTUAL
/// arguments. When they cannot be recovered from declared annotations, the
/// arguments are elided — never harvested from name-sorted member types
/// (kysely witness: `SelectQueryBuilderImpl<DB, TB, O>` displayed as
/// `SelectQueryBuilderImpl<SelectQueryBuilderProps, O | undefined, true>`).
#[test]
fn generic_instance_display_never_zips_member_types_into_type_args() {
    let diagnostic = ts2322_diagnostic(
        r#"
type AnyColumn<DB, TB extends keyof DB> = {
  [T in TB]: keyof DB[T]
}[TB] &
  string

type RefExpr<DB, TB extends keyof DB> = AnyColumn<DB, TB> | ((db: DB) => TB)

interface Maker<DB, TB extends keyof DB, O> {
  get expressionType(): O | undefined
  get isMaker(): true
  refine<LRE extends RefExpr<DB, TB>>(lhs: LRE): Maker<DB, TB, O>
}

class MakerImpl<DB, TB extends keyof DB, O> implements Maker<DB, TB, O> {
  get expressionType(): O | undefined {
    return undefined
  }
  get isMaker(): true {
    return true
  }
  refine(lhs: RefExpr<DB, TB>): Maker<DB, TB, O> {
    return new MakerImpl()
  }
}

function probe<DB, TB extends keyof DB, O>(): Maker<DB, TB, O> & { extra: 1 } {
  return new MakerImpl()
}
"#,
    );

    assert!(
        diagnostic.message_text.contains("Type 'MakerImpl"),
        "expected the instance type in the message, got {diagnostic:#?}"
    );
    // Member types (getter/method types, name-sorted) must never be zipped
    // into the displayed argument list.
    for fabricated in [
        "MakerImpl<true",
        "MakerImpl<O | undefined",
        "MakerImpl<Maker<",
    ] {
        assert!(
            !diagnostic.message_text.contains(fabricated),
            "member types were harvested into the type-argument display ({fabricated}): {diagnostic:#?}"
        );
    }
}

/// Same witness with renamed binders and reordered members: the zip defect
/// sorted properties alphabetically, so the fabricated argument list rotated
/// with member names. No member-name ordering may leak into the display.
#[test]
fn generic_instance_display_zip_guard_is_member_name_independent() {
    let diagnostic = ts2322_diagnostic(
        r#"
interface Zed<Alpha, Beta extends keyof Alpha, Out> {
  get zz(): Out | undefined
  get aa(): true
  pick<K extends keyof Alpha>(key: K): Zed<Alpha, Beta, Out>
}

class ZedImpl<Alpha, Beta extends keyof Alpha, Out> implements Zed<Alpha, Beta, Out> {
  get zz(): Out | undefined {
    return undefined
  }
  get aa(): true {
    return true
  }
  pick(key: keyof Alpha): Zed<Alpha, Beta, Out> {
    return new ZedImpl()
  }
}

function probe<Alpha, Beta extends keyof Alpha, Out>(): Zed<Alpha, Beta, Out> & { more: 1 } {
  return new ZedImpl()
}
"#,
    );

    assert!(
        diagnostic.message_text.contains("Type 'ZedImpl"),
        "expected the instance type in the message, got {diagnostic:#?}"
    );
    for fabricated in ["ZedImpl<true", "ZedImpl<Out | undefined", "ZedImpl<Zed<"] {
        assert!(
            !diagnostic.message_text.contains(fabricated),
            "member types were harvested into the type-argument display ({fabricated}): {diagnostic:#?}"
        );
    }
}
