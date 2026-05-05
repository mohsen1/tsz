//! Behavior locks for the text-based import-usage heuristics.
//!
//! These tests pin the current behavior of the three exported functions in
//! `tsz_emitter::import_usage`:
//!
//! - [`contains_identifier_occurrence`] — word-boundary identifier search.
//! - [`strip_type_only_content`] — drop type-only lines and inline annotations
//!   so leftover identifier occurrences indicate value usage.
//! - [`strip_type_declaration_lines`] — drop type-only declaration lines
//!   while keeping inline annotations and namespace-alias imports intact.
//!
//! Both stripping functions are heuristic — they operate on raw source text
//! before parsing — so the test suite focuses on the operationally important
//! cases: import binding classification, multi-line type bodies, and the
//! boundaries where the emitter's value-usage decision changes.

use super::{
    contains_identifier_occurrence, name_appears_in_decorator_metadata_type,
    strip_qualified_accesses_for_names, strip_type_declaration_lines, strip_type_only_content,
};

// ----------------------------------------------------------------------
// contains_identifier_occurrence
// ----------------------------------------------------------------------

#[test]
fn contains_identifier_occurrence_matches_standalone_word() {
    assert!(contains_identifier_occurrence("foo", "foo"));
    assert!(contains_identifier_occurrence("foo bar", "foo"));
    assert!(contains_identifier_occurrence("bar foo", "foo"));
    assert!(contains_identifier_occurrence("(foo)", "foo"));
    assert!(contains_identifier_occurrence("foo()", "foo"));
}

#[test]
fn contains_identifier_occurrence_rejects_substring() {
    assert!(!contains_identifier_occurrence("foobar", "foo"));
    assert!(!contains_identifier_occurrence("barfoo", "foo"));
    assert!(!contains_identifier_occurrence("foo_bar", "foo"));
    assert!(!contains_identifier_occurrence("foo$bar", "foo"));
    assert!(!contains_identifier_occurrence("foo123", "foo"));
    assert!(!contains_identifier_occurrence("$foo", "foo"));
    assert!(!contains_identifier_occurrence("_foo", "foo"));
}

#[test]
fn contains_identifier_occurrence_rejects_member_access() {
    // `obj.foo` is property access, not a standalone reference to `foo`.
    assert!(!contains_identifier_occurrence("obj.foo", "foo"));
    assert!(!contains_identifier_occurrence("a.b.foo", "foo"));
}

#[test]
fn contains_identifier_occurrence_finds_after_skipped_position() {
    // First match is rejected (substring); second is a real word.
    assert!(contains_identifier_occurrence("foobar foo", "foo"));
    // First match is property access; second is real.
    assert!(contains_identifier_occurrence("obj.foo; foo()", "foo"));
}

#[test]
fn contains_identifier_occurrence_empty_needle_is_false() {
    assert!(!contains_identifier_occurrence("anything", ""));
    assert!(!contains_identifier_occurrence("", ""));
}

#[test]
fn contains_identifier_occurrence_empty_haystack() {
    assert!(!contains_identifier_occurrence("", "foo"));
}

#[test]
fn contains_identifier_occurrence_at_string_boundaries() {
    // Identifier at the very start.
    assert!(contains_identifier_occurrence("foo bar", "foo"));
    // Identifier at the very end (no trailing char).
    assert!(contains_identifier_occurrence("bar foo", "foo"));
    // Identifier alone.
    assert!(contains_identifier_occurrence("foo", "foo"));
}

#[test]
fn strip_qualified_accesses_for_names_removes_member_reads() {
    let names = rustc_hash::FxHashSet::from_iter(["Enum".to_string(), "Alias".to_string()]);
    let stripped = strip_qualified_accesses_for_names(
        "Enum.Foo; Alias[\"Bar\"]; Other.Baz; EnumValue;",
        &names,
    );
    assert!(!contains_identifier_occurrence(&stripped, "Enum"));
    assert!(!contains_identifier_occurrence(&stripped, "Alias"));
    assert!(contains_identifier_occurrence(&stripped, "Other"));
    assert!(contains_identifier_occurrence(&stripped, "EnumValue"));
}

#[test]
fn strip_qualified_accesses_for_names_keeps_plain_value_reads() {
    let names = rustc_hash::FxHashSet::from_iter(["Enum".to_string()]);
    let stripped = strip_qualified_accesses_for_names("Enum.Foo; use(Enum);", &names);
    assert!(contains_identifier_occurrence(&stripped, "Enum"));
}

// ----------------------------------------------------------------------
// strip_type_only_content
// ----------------------------------------------------------------------

#[test]
fn strip_type_only_content_drops_import_type_lines() {
    let src = "import type { Foo } from './foo';\nconst x = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_drops_export_type_lines() {
    let src = "export type Bar = { y: number };\nconst x = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Bar"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_drops_declare_lines() {
    let src = "declare const Foo: number;\nFoo;\n";
    let stripped = strip_type_only_content(src);
    // The declare line is dropped, but the `Foo;` value usage is kept.
    assert!(contains_identifier_occurrence(&stripped, "Foo"));
}

#[test]
fn strip_type_only_content_drops_interface_block_multiline() {
    let src = "interface Foo {\n  x: number;\n  y: string;\n}\nconst z = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    // Interior `x`, `y`, `string`, `number` are inside the type body and
    // should not survive as value identifiers.
    assert!(!contains_identifier_occurrence(&stripped, "x"));
    assert!(contains_identifier_occurrence(&stripped, "z"));
}

#[test]
fn strip_type_only_content_drops_type_alias_block_multiline() {
    let src = "type Foo = {\n  x: number;\n};\nconst z = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(!contains_identifier_occurrence(&stripped, "x"));
    assert!(contains_identifier_occurrence(&stripped, "z"));
}

#[test]
fn strip_type_only_content_drops_other_module_imports() {
    // Identifiers from *other* imports should not count as value usages
    // of `this` import, so other module imports are stripped.
    let src = "import { other } from './m';\nconst x = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "other"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_keeps_namespace_alias_import() {
    // `import X = Y;` references `Y` as a value-level binding and must NOT be
    // stripped. (Counterexample to the generic "import" rule.)
    let src = "import X = Y;\n";
    let stripped = strip_type_only_content(src);
    assert!(contains_identifier_occurrence(&stripped, "Y"));
}

#[test]
fn strip_type_only_content_keeps_export_import_namespace_alias() {
    // `export import X = Y;` is also a value-level reference to `Y`.
    let src = "export import X = Y;\n";
    let stripped = strip_type_only_content(src);
    assert!(contains_identifier_occurrence(&stripped, "Y"));
}

#[test]
fn strip_type_only_content_drops_export_star_reexport() {
    let src = "export * from './m';\nconst x = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_drops_named_reexport_with_from() {
    // `export { a } from "m"` re-exports from a module; `a` is not a local
    // value usage and should be stripped.
    let src = "export { a } from './m';\nconst x = 1;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "a"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_keeps_local_named_reexport() {
    // `export { a }` (no `from`) re-exports a local value binding —
    // it IS a value usage and must NOT be stripped.
    let src = "const a = 1;\nexport { a };\n";
    let stripped = strip_type_only_content(src);
    assert!(contains_identifier_occurrence(&stripped, "a"));
}

#[test]
fn strip_type_only_content_strips_inline_var_annotations() {
    let src = "const x: Foo = 1;\n";
    let stripped = strip_type_only_content(src);
    // `Foo` is a type annotation; should not be a value usage.
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    // `x` is the bound name; remains.
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_strips_param_annotations() {
    let src = "function f(p: Foo) { p; }\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "p"));
}

#[test]
fn strip_type_only_content_strips_return_type_annotation() {
    let src = "function f(): Foo { return null; }\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
}

#[test]
fn strip_type_only_content_strips_as_type_assertion() {
    let src = "const x = v as Foo;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "v"));
}

#[test]
fn strip_type_only_content_strips_satisfies_type_assertion() {
    let src = "const x = v satisfies Foo;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "v"));
}

#[test]
fn strip_type_only_content_strips_implements_clause() {
    let src = "class A implements Foo { x() {} }\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    // Class name and body remain.
    assert!(contains_identifier_occurrence(&stripped, "A"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_keeps_extends_in_class_decl() {
    // `extends` in a class declaration is value-level (runs at runtime).
    let src = "class A extends Base { }\n";
    let stripped = strip_type_only_content(src);
    assert!(contains_identifier_occurrence(&stripped, "Base"));
}

#[test]
fn strip_type_only_content_strips_generic_type_args_in_call() {
    let src = "f<Foo>(x);\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "f"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_keeps_object_literal_value_after_colon() {
    // `{ key: value }` should NOT trigger the variable annotation stripper.
    let src = "const obj = { foo: bar };\n";
    let stripped = strip_type_only_content(src);
    // The value reference `bar` must survive.
    assert!(contains_identifier_occurrence(&stripped, "bar"));
}

#[test]
fn strip_type_only_content_keeps_string_literals_intact() {
    // `:` inside string literals must not be misread as a type annotation.
    let src = "const x = \"a:b:Foo\";\n";
    let stripped = strip_type_only_content(src);
    // The string content is preserved verbatim, including `Foo`.
    assert!(stripped.contains("\"a:b:Foo\""));
}

#[test]
fn strip_type_only_content_strips_var_decl_without_initializer() {
    // `let x: T;` — no `=`, the colon IS a type annotation.
    let src = "let x: Foo;\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_only_content_skips_line_comments() {
    // Identifiers inside `// ...` should not count as value usages.
    let src = "const x = 1; // see Foo\n";
    let stripped = strip_type_only_content(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

// ----------------------------------------------------------------------
// strip_type_declaration_lines
// ----------------------------------------------------------------------

#[test]
fn strip_type_declaration_lines_drops_type_only_declarations() {
    let src =
        "import type { Foo } from './m';\ntype Bar = number;\ninterface Baz {}\nconst x = 1;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(!contains_identifier_occurrence(&stripped, "Bar"));
    assert!(!contains_identifier_occurrence(&stripped, "Baz"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_declaration_lines_keeps_inline_annotations() {
    // Unlike `strip_type_only_content`, this function does NOT strip inline
    // type annotations. `Foo` after `:` is preserved.
    let src = "const x: Foo = 1;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "x"));
}

#[test]
fn strip_type_declaration_lines_keeps_value_imports() {
    // Value imports from other modules are NOT stripped here.
    let src = "import { other } from './m';\nconst x = 1;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(contains_identifier_occurrence(&stripped, "other"));
}

#[test]
fn strip_type_declaration_lines_keeps_namespace_alias() {
    // `import X = Y;` is preserved (namespace aliases reference values).
    let src = "import X = Y;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(contains_identifier_occurrence(&stripped, "Y"));
}

#[test]
fn strip_type_declaration_lines_drops_multiline_interface_body() {
    let src = "interface Foo {\n  x: number;\n  y: string;\n}\nconst z = 1;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(!contains_identifier_occurrence(&stripped, "x"));
    assert!(contains_identifier_occurrence(&stripped, "z"));
}

#[test]
fn strip_type_declaration_lines_drops_export_declare_block() {
    let src = "export declare class Foo {\n  m(): void;\n}\nconst z = 1;\n";
    let stripped = strip_type_declaration_lines(src);
    assert!(!contains_identifier_occurrence(&stripped, "Foo"));
    assert!(contains_identifier_occurrence(&stripped, "z"));
}

// ----------------------------------------------------------------------
// name_appears_in_decorator_metadata_type
// ----------------------------------------------------------------------

#[test]
fn metadata_type_finds_decorated_property_annotation() {
    let src = r"
class Test {
    @whatever
    declare prop: Observable<string>;
}
";
    assert!(name_appears_in_decorator_metadata_type(src, "Observable"));
    // A name that doesn't appear in any annotation should not match.
    assert!(!name_appears_in_decorator_metadata_type(src, "Unrelated"));
}

#[test]
fn metadata_type_skips_undecorated_property() {
    // No decorator, so the type-only annotation must NOT be reported as a
    // value usage. This is the exact case the standard type-strip already
    // catches; the helper must agree.
    let src = "class Test { prop: Observable<string>; }\n";
    assert!(!name_appears_in_decorator_metadata_type(src, "Observable"));
}

#[test]
fn metadata_type_walks_chained_decorators() {
    // Two decorators on the same member; the helper must walk past both.
    let src = "class T { @a @b prop: Observable<string>; }\n";
    assert!(name_appears_in_decorator_metadata_type(src, "Observable"));
}

#[test]
fn metadata_type_walks_decorator_with_args() {
    // `@dec(arg)` — paren args must be skipped.
    let src = "class T { @inject('token') prop: Observable<string>; }\n";
    assert!(name_appears_in_decorator_metadata_type(src, "Observable"));
}

#[test]
fn metadata_type_handles_method_param_annotations() {
    // For methods, parameter annotations are also `design:paramtypes`
    // metadata. The current helper only matches the first `:` after a
    // decorator (the return type or first param) — covering the common
    // property-decorator case.
    let src = "class T { @dec method(p: Observable<string>): void {} }\n";
    assert!(name_appears_in_decorator_metadata_type(src, "Observable"));
}
