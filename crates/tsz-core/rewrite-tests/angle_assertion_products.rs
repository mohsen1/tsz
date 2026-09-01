use super::*;

#[test]
fn angle_assertion_type_names_remain_typed_semantic_nonclaims() {
    for source in [
        "type Cedar = number; const renamed = 0; const value = <Cedar>renamed;",
        "const Cedar = 1; const renamed = 0; const value = <Cedar>renamed;",
        "const renamed = 0; const value = <MissingType>renamed;",
    ] {
        let output = compile(source, true, false);
        assert!(output.diagnostics.is_empty(), "{source}: {output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert!(output.emitted_files.is_empty(), "{source}: {output:#?}");
    }
}

#[test]
fn angle_assertions_emit_authored_types_in_checked_and_no_check_modes() {
    let source = concat!(
        "export const renamed = <number>1, changed = 1;\n",
        "export const nested = (<ReadonlyArray<number>>([1, 2]));\n",
    );
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "angle-assertion.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                target: "es2022".to_string(),
                strict: true,
                declaration: true,
                no_check,
                ..CompilerOptions::default()
            },
        );
        assert!(output.diagnostics.is_empty(), "{output:#?}");
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert_eq!(output.emitted_files.len(), 2, "{output:#?}");
        for (path, text, declaration) in [
            (
                "angle-assertion.js",
                "export const renamed = 1, changed = 1;\nexport const nested = ([1, 2]);\n",
                false,
            ),
            (
                "angle-assertion.d.ts",
                "export declare const renamed: number, changed = 1;\nexport declare const nested: ReadonlyArray<number>;\n",
                true,
            ),
        ] {
            let emitted = output
                .emitted_files
                .iter()
                .find(|file| file.path.to_str() == Some(path))
                .unwrap_or_else(|| panic!("missing {path}: {output:#?}"));
            assert_eq!(emitted.text, text, "{path}");
            assert_eq!(emitted.declaration, declaration, "{path}");
        }
    }
}
