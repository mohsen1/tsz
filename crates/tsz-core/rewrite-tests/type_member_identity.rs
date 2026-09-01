use std::path::Path;
use std::sync::Arc;

use tsz::bind::{TypeMemberSymbol, bind_source_with_kind};
use tsz::diagnostics::{Diagnostic, DiagnosticCategory, RelatedInformation};
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    InterfaceDeclaration, StatementKind, TypeMember, TypeMemberKind, TypeMemberName, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

type RelatedDiagnosticIdentity = (String, u32, u32, u32, String, u32);
type DiagnosticIdentity = (
    String,
    u32,
    u32,
    u32,
    DiagnosticCategory,
    String,
    Vec<RelatedDiagnosticIdentity>,
);

fn related_diagnostic_identities(related: &[RelatedInformation]) -> Vec<RelatedDiagnosticIdentity> {
    related
        .iter()
        .map(|related| {
            (
                related.file.clone(),
                related.code,
                related.start,
                related.length,
                related.message_text.clone(),
                related.depth,
            )
        })
        .collect()
}

fn diagnostic_identities(diagnostics: &[Diagnostic]) -> Vec<DiagnosticIdentity> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.file.clone(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.clone(),
                related_diagnostic_identities(&diagnostic.related_information),
            )
        })
        .collect()
}

fn last_utf16_span(source: &str, needle: &str) -> (u32, u32) {
    let start = source.rfind(needle).expect("diagnostic target");
    (
        source[..start].encode_utf16().count() as u32,
        needle.encode_utf16().count() as u32,
    )
}

fn compile(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    )
}

fn interface(parsed: &tsz::syntax::ParseOutput) -> &InterfaceDeclaration {
    parsed
        .unit
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            StatementKind::Interface(declaration) => Some(declaration),
            _ => None,
        })
        .expect("interface declaration")
}

fn method_name(member: &TypeMember) -> &TypeMemberName {
    let TypeMemberKind::Method { name, .. } = &member.kind else {
        panic!("method signature")
    };
    name
}

#[test]
fn binder_groups_static_literal_names_by_cooked_property_key() {
    let source = r#"interface Shape {
        "same"?(): void;
        same(): void;
        "sa\u006de"(): void;
        1?(): void;
        "1"(): void;
        1.0(): void;
        0x10?(): void;
        16(): void;
        "left"?(): void;
        "right"(): void;
    }"#;
    let source_text = SourceText::new(FileId(0), "case.ts".into(), Arc::<str>::from(source));
    let parsed = parse_source(&source_text);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let members = &interface(&parsed).members;
    let bound = bind_source_with_kind(
        source_text.id,
        tsz::source::SourceKind::TypeScript,
        &parsed.unit,
    );

    for (indices, key) in [
        (&[0, 1, 2][..], "same"),
        (&[3, 4, 5][..], "1"),
        (&[6, 7][..], "16"),
    ] {
        let canonical = bound
            .type_member_group(members[indices[0]].id)
            .and_then(|group| group.first())
            .copied();
        assert_eq!(
            bound
                .type_member_group(members[indices[0]].id)
                .map(<[_]>::len),
            Some(indices.len())
        );
        for index in indices {
            assert_eq!(
                bound
                    .type_member_group(members[*index].id)
                    .and_then(|group| group.first())
                    .copied(),
                canonical
            );
            assert!(matches!(
                bound.type_members[&members[*index].id].symbol,
                Some(TypeMemberSymbol::Named(ref name))
                    if name.iter().copied().eq(key.encode_utf16())
            ));
        }
    }

    assert_ne!(
        bound
            .type_member_group(members[8].id)
            .and_then(|group| group.first())
            .copied(),
        bound
            .type_member_group(members[9].id)
            .and_then(|group| group.first())
            .copied()
    );
    assert_eq!(
        source_text.slice(method_name(&members[2]).span),
        r#""sa\u006de""#
    );
    assert_eq!(source_text.slice(method_name(&members[6]).span), "0x10");
}

#[test]
fn static_literal_optional_overloads_follow_pinned_diagnostic_identity() {
    for (source, expected_name) in [
        (
            r#"interface I { "quoted"?(): void; "quoted"(): void; }"#,
            r#""quoted""#,
        ),
        (
            r#"interface I { "quoted"(): void; "quoted"?(): void; }"#,
            r#""quoted""#,
        ),
        (
            r#"interface I { renamed?(): void; "renamed"(): void; }"#,
            r#""renamed""#,
        ),
        (
            r#"interface I { "renamed"?(): void; renamed(): void; }"#,
            "renamed",
        ),
        (
            r#"interface I { "co\u006fked"?(): void; cooked(): void; }"#,
            "cooked",
        ),
        (r#"interface I { 1?(): void; "1"(): void; }"#, r#""1""#),
        ("interface I { 1.0?(): void; 1(): void; }", "1"),
        ("interface I { 0x10?(): void; 16(): void; }", "16"),
    ] {
        let output = compile(source);
        let (start, length) = last_utf16_span(source, expected_name);
        assert_eq!(
            diagnostic_identities(&output.diagnostics),
            vec![(
                "case.ts".to_string(),
                2386,
                start,
                length,
                DiagnosticCategory::Error,
                "Overload signatures must all be optional or required.".to_string(),
                Vec::new(),
            )],
            "{source}: {:?}",
            output.diagnostics
        );
    }
}

#[test]
fn binder_identity_keeps_authored_literal_spelling_for_declaration_emit() {
    let source = r#"export interface I { "co\u006fked"(): void; 0x10(): void; }"#;
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "es2015".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .map(|file| (file.path.as_path(), file.declaration, file.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                Path::new("case.d.ts"),
                true,
                concat!(
                    "export interface I {\n",
                    "    \"co\\u006fked\"(): void;\n",
                    "    0x10(): void;\n",
                    "}\n",
                ),
            ),
            (
                Path::new("case.js"),
                false,
                concat!(
                    "\"use strict\";\n",
                    "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                ),
            ),
        ],
    );
}

#[test]
fn distinct_and_unsupported_computed_names_do_not_form_false_groups() {
    let distinct = compile(r#"interface I { "left"?(): void; "right"(): void; }"#);
    assert!(
        distinct
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2386),
        "{:?}",
        distinct.diagnostics
    );

    // Delete this Deferred control when computed literal names join the binder's
    // static-key path and TS2386 can be claimed for the pair.
    let source = r#"interface I { ["late"]?(): void; late(): void; } let value: I;"#;
    let source_text = SourceText::new(FileId(0), "case.ts".into(), Arc::<str>::from(source));
    let parsed = parse_source(&source_text);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let members = &interface(&parsed).members;
    let bound = bind_source_with_kind(
        source_text.id,
        tsz::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    assert!(bound.type_members[&members[0].id].symbol.is_none());
    assert!(bound.type_member_group(members[0].id).is_none());
    assert!(matches!(
        bound.type_members[&members[1].id].symbol,
        Some(TypeMemberSymbol::Named(ref name))
            if name.iter().copied().eq("late".encode_utf16())
    ));

    let computed = compile(source);
    assert_eq!(computed.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(computed.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(
        computed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2386),
        "{:?}",
        computed.diagnostics
    );
}

#[test]
fn lone_surrogate_member_keys_never_collapse_into_lossy_string_spellings() {
    let distinct = r#"interface I { "\uD800"?(): void; "\\uD800"(): void; }"#;
    let source_text = SourceText::new(FileId(0), "case.ts".into(), Arc::<str>::from(distinct));
    let parsed = parse_source(&source_text);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let members = &interface(&parsed).members;
    let bound = bind_source_with_kind(
        source_text.id,
        tsz::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    assert_ne!(
        bound.type_members[&members[0].id].symbol,
        bound.type_members[&members[1].id].symbol,
    );
    assert!(
        compile(distinct)
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2386)
    );

    let repeated = compile(r#"interface I { "\uD800"?(): void; "\uD800"(): void; }"#);
    assert_eq!(
        repeated
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2386],
        "{:?}",
        repeated.diagnostics,
    );

    let invalid = r#"interface I { "\u{110000}"?(): void; "\u{110000}"(): void; }"#;
    let source_text = SourceText::new(FileId(0), "invalid.ts".into(), Arc::<str>::from(invalid));
    let parsed = parse_source(&source_text);
    assert!(!parsed.diagnostics.is_empty());
    let members = &interface(&parsed).members;
    let bound = bind_source_with_kind(
        source_text.id,
        tsz::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    assert!(members.iter().all(|member| {
        bound
            .type_members
            .get(&member.id)
            .is_none_or(|member| member.symbol.is_none())
    }));
}
