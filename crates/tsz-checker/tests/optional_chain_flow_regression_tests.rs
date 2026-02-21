use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_source(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn optional_chain_intermediate_flow_skip_preserves_types() {
    let source = r#"
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;

interface RetryOptions {
    timeout: number;
    retries: number;
    nested: {
        transport: {
            backoff: {
                base: number;
                max: number;
                jitter: number;
            };
        };
    };
}

declare const options: DeepPartial<RetryOptions> | undefined;

const base: number = options?.nested?.transport?.backoff?.base ?? 10;
const maxV: number = options?.nested?.transport?.backoff?.max ?? 100;
const jitter: number = options?.nested?.transport?.backoff?.jitter ?? 1;

if (options?.nested?.transport?.backoff?.base) {
    const stillNumber: number = options?.nested?.transport?.backoff?.base ?? 0;
}
"#;

    let diags = diagnostics_for_source(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for deep optional-chain access, got: {diags:?}"
    );
}
