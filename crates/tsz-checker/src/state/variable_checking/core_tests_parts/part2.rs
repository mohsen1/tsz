#[cfg(test)]
mod ts2502_alias_prior_decl_tests {
    //! TS2502 self-reference detection should not be suppressed when the prior
    //! declaration of the same name is an alias (import/UMD namespace export).
    //! Aliases bind a name to another module's surface but do not establish a
    //! value-typed binding in the redeclaring scope, so `typeof X` inside a
    //! later `const X` declaration with the same name is genuinely circular.
    //!
    //! Mirrors tsc behavior for cases like
    //!   import * as foo from './foo';
    //!   declare global { const foo: typeof foo; }
    //! (conformance/compiler/crashDeclareGlobalTypeofExport.ts)
    use super::test_utils::check_and_collect;

    #[test]
    fn umd_namespace_export_does_not_suppress_ts2502() {
        // `export as namespace foo` is a UMD alias — it should NOT be
        // treated as a prior value declaration that satisfies `typeof foo`
        // for a later `const foo` declaration.
        let source = "export as namespace foo;\n\
            declare global {\n\
            \x20\x20\x20\x20const foo: typeof foo;\n\
            }";
        let errors = check_and_collect(source, 2502);
        assert_eq!(
            errors.len(),
            1,
            "Expected 1 TS2502 (UMD alias should not suppress self-reference): {errors:?}"
        );
        assert!(
            errors[0].1.contains("'foo'"),
            "Diagnostic should reference 'foo': {errors:?}"
        );
    }

    #[test]
    fn block_scoped_var_still_suppresses_ts2502_unchanged() {
        // Regression guard: `var p: T1; var p: typeof p;` is the canonical
        // valid redeclaration where `typeof p` legitimately resolves to the
        // prior var's annotation, so TS2502 must remain suppressed.
        let source = "var p: number;\nvar p: typeof p;";
        let errors = check_and_collect(source, 2502);
        assert_eq!(
            errors.len(),
            0,
            "Expected no TS2502 for legitimate var/var typeof redecl: {errors:?}"
        );
    }

    #[test]
    fn no_prior_decl_self_reference_still_fires() {
        // Regression guard: a lone `const foo: typeof foo` (no prior decl)
        // must continue to emit TS2502.
        let source = "const foo: typeof foo = 0 as any;";
        let errors = check_and_collect(source, 2502);
        assert_eq!(
            errors.len(),
            1,
            "Expected 1 TS2502 for lone const self-reference: {errors:?}"
        );
    }
}

#[cfg(test)]
mod function_type_nested_check_tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::{
        check_source_diagnostics, check_source_with_libs, diagnostic_codes, diagnostic_count,
        load_default_lib_files,
    };

    /// TS2536 inside a function return type must be reported.
    /// Rule: tsc validates all nested type nodes in function/constructor return
    /// types (and parameter types) in the scope of the function's own type
    /// parameters. Any indexed-access expression `T[P]` where `P` is not a
    /// subtype of `keyof T` must emit TS2536 regardless of whether it appears
    /// inside a function return type, a constructor return type, a parameter
    /// type annotation, or any nesting of those.
    ///
    /// Pattern used: `{ [P in T]: T[P] }` (P iterates over T itself, the same
    /// unconstrained type param as the object). tsc emits TS2536 because T is
    /// not a valid key domain for T. The same pattern triggers TS2536 when the
    /// mapped type is at the top level (covered by `mapped_template_invalid_key_index_reports_ts2536`).
    #[test]
    fn ts2536_in_function_return_type_reported() {
        let source = "type Bad<T> = () => { [P in T]: T[P] };";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            1,
            "Expected TS2536 for T[P] (P in T) inside function return type"
        );
    }

    /// Same rule with a different iteration variable name — the fix must not be
    /// keyed on the variable name.
    #[test]
    fn ts2536_in_function_return_type_different_var_name() {
        let source = "type Bad<T> = () => { [Key in T]: T[Key] };";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            1,
            "Expected TS2536 for T[Key] (Key in T) inside function return type (variable name variant)"
        );
    }

    /// TS2536 inside a constructor return type must also be reported.
    #[test]
    fn ts2536_in_constructor_return_type_reported() {
        let source = "type BadCtor<T> = new () => { [Q in T]: T[Q] };";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            1,
            "Expected TS2536 for T[Q] (Q in T) inside constructor return type"
        );
    }

    /// TS2536 inside a parameter type annotation must be reported.
    #[test]
    fn ts2536_in_function_parameter_type_reported() {
        let source = "type BadParam<T> = (x: { [P in T]: T[P] }) => void;";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            1,
            "Expected TS2536 for T[P] (P in T) inside parameter type annotation"
        );
    }

    /// When the outer mapped type's key (`K` from `keyof T`) is used as the
    /// constraint of an inner mapped type inside a function return type, no
    /// TS2536 must be emitted because `K` is constrained to `keyof T`.
    /// Regression guard for false positives introduced by the fix.
    #[test]
    fn no_ts2536_when_outer_mapped_key_constrains_inner_index() {
        let source = "type Ok<T> = { [K in keyof T]: () => { [P in K[]]: T[K] } };";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            0,
            "Expected no TS2536 when outer mapped key constrains inner index"
        );
    }

    /// TS2536 inside a doubly-nested return type must be reported (recursive
    /// traversal through nested function types).
    #[test]
    fn ts2536_in_nested_function_return_type_reported() {
        let source = "type Nested<T> = () => () => { [P in T]: T[P] };";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            1,
            "Expected TS2536 for T[P] (P in T) inside doubly-nested function return type"
        );
    }

    /// A valid indexed access in a function return type with `keyof` constraint
    /// must not emit any diagnostic. This covers the OUTER alias type parameter path
    /// (K is an outer-scope param of the alias itself).
    #[test]
    fn no_ts2536_for_valid_indexed_access_in_function_return() {
        let source = "type Getter<T, K extends keyof T> = () => T[K];";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            0,
            "Expected no TS2536 for valid T[K] (K extends keyof T) in function return type"
        );
    }

    /// An INNER generic function type with `K extends keyof T` must not emit TS2536.
    /// This specifically tests that the constraint-preserving scope push is used:
    /// with a constraint-dropping push, K would look unconstrained and `T[K]`
    /// would incorrectly trigger TS2536.
    #[test]
    fn no_ts2536_for_inner_generic_function_with_keyof_constraint() {
        let source = "type F<T> = <K extends keyof T>() => T[K];";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            0,
            "Expected no TS2536 for inner generic <K extends keyof T>() => T[K]"
        );
    }

    /// An INNER generic constructor type with `K extends keyof T` must not emit TS2536.
    #[test]
    fn no_ts2536_for_inner_generic_constructor_with_keyof_constraint() {
        let source = "type C<T> = new <K extends keyof T>() => T[K];";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            0,
            "Expected no TS2536 for inner generic constructor new <K extends keyof T>() => T[K]"
        );
    }

    /// An inner generic function type with a defaulted `K extends keyof T = keyof T`
    /// must not emit TS2536 for parameter or return type annotations.
    #[test]
    fn no_ts2536_for_inner_generic_function_with_defaulted_keyof_constraint() {
        let source = "type F<T> = <K extends keyof T = keyof T>(x: T[K]) => T[K];";
        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_count(&diags, 2536),
            0,
            "Expected no TS2536 for inner generic <K extends keyof T = keyof T>(x: T[K]) => T[K]"
        );
    }

    #[test]
    fn merged_interface_function_constraints_keep_returntype_valid() {
        let libs = load_default_lib_files();
        for source in [
            r#"
                export namespace ns {
                    interface Function<T extends (...args: any) => any> {
                        throttle(): Function<T>;
                    }
                    interface Function<T> {
                        unary(): Function<() => ReturnType<T>>;
                    }
                }
            "#,
            r#"
                export namespace ns {
                    interface Function<T> {
                        unary(): Function<() => ReturnType<T>>;
                    }
                    interface Function<T extends (...args: any) => any> {
                        throttle(): Function<T>;
                    }
                }
            "#,
        ] {
            let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
            assert_eq!(
                diagnostic_codes(&diags),
                Vec::<u32>::new(),
                "expected merged interface function constraints to stay valid, got {diags:?}"
            );
        }
    }

    #[test]
    fn mapped_type_inference_from_apparent_type_keeps_only_assignment_error() {
        let source = r#"
            type Obj = {
                [s: string]: number;
            };

            type foo = <T>(target: { [K in keyof T]: T[K] }) => void;
            type bar = <U extends string[]>(source: { [K in keyof U]: Obj[K] }) => void;

            declare let f: foo;
            declare let b: bar;
            b = f;
        "#;

        let diags = check_source_diagnostics(source);
        assert_eq!(
            diagnostic_codes(&diags),
            vec![2322],
            "expected only the assignment error, got {diags:?}"
        );
    }
}
