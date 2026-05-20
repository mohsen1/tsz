//! Shared helpers for TS2344 class-constructor constraint tests.

pub(crate) fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(&format!(
        r#"
interface Array<T> {{}}
interface Boolean {{}}
interface CallableFunction {{}}
interface Function {{}}
interface IArguments {{}}
interface NewableFunction {{}}
interface Number {{}}
interface Object {{}}
interface RegExp {{}}
interface String {{}}

{source}
"#
    ))
}

pub(crate) fn diagnostics_for_code(
    diagnostics: &[(u32, String)],
    code: u32,
) -> Vec<&(u32, String)> {
    diagnostics
        .iter()
        .filter(|(actual_code, _)| *actual_code == code)
        .collect()
}
