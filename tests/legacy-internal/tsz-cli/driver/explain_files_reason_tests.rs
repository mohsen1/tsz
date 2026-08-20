use super::*;

/// Issue #3901: tsc surfaces tsconfig `files` entries with a distinct
/// reason from `include` matches.
#[test]
fn files_list_entry_renders_tsc_phrasing() {
    assert_eq!(
        FileInclusionReason::FilesListEntry.to_string(),
        "Part of 'files' list in tsconfig.json"
    );
}

/// Default-lib reasons must mention the configured target so users
/// can attribute the lib pull. tsc renders the lowercase ECMAScript
/// revision name.
#[test]
fn default_library_reason_includes_target() {
    assert_eq!(
        FileInclusionReason::DefaultLibrary("es2018".to_string()).to_string(),
        "Default library for target 'es2018'"
    );
}

/// `is_default_lib_for_target` matches both the `lib.<target>.full.d.ts`
/// and `lib.<target>.d.ts` shapes that the lib resolver produces.
#[test]
fn default_lib_matches_full_and_bare_for_target() {
    let full = PathBuf::from("/usr/typescript/lib.es2018.full.d.ts");
    let bare = PathBuf::from("/usr/typescript/lib.es2018.d.ts");
    let other_target = PathBuf::from("/usr/typescript/lib.es2020.full.d.ts");
    let unrelated = PathBuf::from("/usr/typescript/lib.dom.d.ts");

    assert!(is_default_lib_for_target(&full, ScriptTarget::ES2018));
    assert!(is_default_lib_for_target(&bare, ScriptTarget::ES2018));
    assert!(!is_default_lib_for_target(
        &other_target,
        ScriptTarget::ES2018
    ));
    assert!(!is_default_lib_for_target(&unrelated, ScriptTarget::ES2018));
}

/// Locks the lowercase target spelling for the explainFiles surface.
#[test]
fn target_display_for_explain_files_lowercase() {
    assert_eq!(
        script_target_display_for_explain_files(ScriptTarget::ES5),
        "es5"
    );
    assert_eq!(
        script_target_display_for_explain_files(ScriptTarget::ES2018),
        "es2018"
    );
    assert_eq!(
        script_target_display_for_explain_files(ScriptTarget::ESNext),
        "esnext"
    );
}
