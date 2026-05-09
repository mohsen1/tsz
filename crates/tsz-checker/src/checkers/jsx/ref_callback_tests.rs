use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source;

fn check_jsx(source: &str) -> Vec<Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

#[test]
fn intrinsic_ref_callback_uses_html_element_context_without_false_positive() {
    let source = r#"
        declare namespace React {
            type Ref<T> = string | ((instance: T) => any);

            interface Attributes {
                key?: string | number;
            }

            interface ClassAttributes<T> extends Attributes {
                ref?: Ref<T>;
            }
        }

        declare namespace JSX {
            interface Element {}
            interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {}
            interface IntrinsicElements {
                div: React.ClassAttributes<HTMLDivElement> & {};
            }
        }

        interface HTMLDivElement {
            innerText: string;
        }

        <div ref={x => x.innerText} />;
        <div ref={x => x.propertyNotOnHtmlDivElement} />;
        "#;
    let diagnostics = check_jsx(source);
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2339
                && d.message_text.contains("propertyNotOnHtmlDivElement")
                && d.message_text.contains("type 'HTMLDivElement'")
        }),
        "Expected intrinsic ref callback to be contextually typed as HTMLDivElement, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("innerText")),
        "Expected declared intrinsic ref callback property to remain valid, got: {diagnostics:?}"
    );
}

#[test]
fn intrinsic_ref_callback_uses_inherited_generic_html_props_context() {
    let source = r#"
        declare namespace React {
            type Ref<T> = string | ((instance: T) => any);

            interface Attributes {
                key?: string | number;
            }

            interface ClassAttributes<T> extends Attributes {
                ref?: Ref<T>;
            }

            interface HTMLAttributes<T> {
                id?: string;
            }

            interface HTMLProps<T> extends HTMLAttributes<T>, ClassAttributes<T> {}
        }

        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                div: React.HTMLProps<HTMLDivElement>;
            }
        }

        interface HTMLDivElement {
            innerText: string;
        }

        <div ref={x => x.propertyNotOnHtmlDivElement} />;
        "#;
    let diagnostics = check_jsx(source);
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2339
                && d.message_text.contains("propertyNotOnHtmlDivElement")
                && d.message_text.contains("type 'HTMLDivElement'")
        }),
        "Expected inherited generic intrinsic ref callback to be contextually typed as HTMLDivElement, got: {diagnostics:?}"
    );
}
