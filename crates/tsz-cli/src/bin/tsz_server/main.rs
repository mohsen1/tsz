#[cfg(test)]
#[path = "tests.rs"]
mod tests;

mod check;

mod handlers_code_fixes;

mod handlers_code_fixes_enum_member;

mod handlers_code_fixes_fallbacks;

mod handlers_code_fixes_implement_interface;

mod handlers_code_fixes_imports;

mod handlers_code_fixes_jsdoc;

mod handlers_code_fixes_synthetic;

mod handlers_code_fixes_utils;

mod handlers_completions;

mod handlers_completions_display;

mod handlers_completions_parameters;

mod handlers_completions_snippets;

mod handlers_diagnostics;

mod handlers_editing;

mod handlers_files;

mod handlers_info;

mod handlers_info_alias;

mod handlers_legacy;

mod handlers_project_info;

mod handlers_quickinfo;

mod handlers_quickinfo_text;

mod handlers_structure;

mod text_edits;

include!("main_parts/part1.rs");
include!("main_parts/part2.rs");
