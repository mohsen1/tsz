//! TS2303 for CommonJS export-property alias cycles in JS files.
//!
//! Oracle (`typescript@7.0.2`, `--allowJs --checkJs --strict false`):
//! `exports.X = exports.Y` / `module.exports.X = module.exports.Y` chains
//! report `Circular definition of import alias` only for a *genuine* cycle,
//! at every alias statement on the cycle, each named by its own alias.
//! A chain that dangles at an undefined name is NOT circular (its failing
//! RHS read is ordinary TS2339 territory), a chain that leads *into* a cycle
//! without being on it stays silent, and a bare `module.exports = X` export
//! assignment beside the property exports (the TS2309 mix) disables alias
//! classification wholesale — no TS2303 at all.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check a single JS file under `allowJs`/`checkJs` and return
/// `(code, start_offset, message)` per diagnostic.
fn check_js_positioned(file_name: &str, source: &str) -> Vec<(u32, u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: false,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// The `(start_offset, message)` of every TS2303 in `source`, sorted by
/// offset.
fn ts2303_sites(file_name: &str, source: &str) -> Vec<(u32, String)> {
    let mut sites: Vec<(u32, String)> = check_js_positioned(file_name, source)
        .into_iter()
        .filter(|(code, _, _)| *code == 2303)
        .map(|(_, start, message)| (start, message))
        .collect();
    sites.sort_by_key(|(start, _)| *start);
    sites
}

fn has_code(diags: &[(u32, u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _, _)| *c == code)
}

/// Byte offset of the first occurrence of `needle` in `source`.
fn offset_of(source: &str, needle: &str) -> u32 {
    u32::try_from(
        source
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not found")),
    )
    .unwrap()
}

fn circular_message(name: &str) -> String {
    format!("Circular definition of import alias '{name}'.")
}

#[test]
fn two_member_cycle_reports_both_sites_each_named_by_its_own_alias() {
    let source = "exports.a = exports.b;\nexports.b = exports.a;\n";
    assert_eq!(
        ts2303_sites("cycle.js", source),
        vec![
            (offset_of(source, "exports.a ="), circular_message("a")),
            (offset_of(source, "exports.b ="), circular_message("b")),
        ]
    );
}

#[test]
fn renamed_binder_cycle_beside_a_concrete_prop_reports_only_the_cycle() {
    let source = "exports.zig = exports.zag;\nexports.zag = exports.zig;\nexports.done = 1;\n";
    assert_eq!(
        ts2303_sites("renamed.js", source),
        vec![
            (offset_of(source, "exports.zig ="), circular_message("zig")),
            (offset_of(source, "exports.zag ="), circular_message("zag")),
        ]
    );
}

#[test]
fn module_exports_spelling_cycle_reports_both_sites() {
    let source =
        "module.exports.gorp = module.exports.zeb;\nmodule.exports.zeb = module.exports.gorp;\n";
    assert_eq!(
        ts2303_sites("spelling.js", source),
        vec![
            (
                offset_of(source, "module.exports.gorp ="),
                circular_message("gorp")
            ),
            (
                offset_of(source, "module.exports.zeb ="),
                circular_message("zeb")
            ),
        ]
    );
}

#[test]
fn mixed_exports_and_module_exports_spellings_form_one_cycle() {
    let source = "exports.gorp = module.exports.zeb;\nmodule.exports.zeb = exports.gorp;\n";
    assert_eq!(
        ts2303_sites("mixed.js", source),
        vec![
            (
                offset_of(source, "exports.gorp ="),
                circular_message("gorp")
            ),
            (
                offset_of(source, "module.exports.zeb ="),
                circular_message("zeb")
            ),
        ]
    );
}

#[test]
fn self_alias_reports_one_site() {
    let source = "exports.myself = exports.myself;\n";
    assert_eq!(
        ts2303_sites("selfref.js", source),
        vec![(
            offset_of(source, "exports.myself ="),
            circular_message("myself")
        )]
    );
}

#[test]
fn a_chain_into_a_cycle_reports_only_the_cycle_members() {
    // `x` aliases into the `a <-> b` cycle but is not on it: tsc reports
    // only `a` and `b`.
    let source = "exports.x = exports.a;\nexports.a = exports.b;\nexports.b = exports.a;\n";
    assert_eq!(
        ts2303_sites("into.js", source),
        vec![
            (offset_of(source, "exports.a ="), circular_message("a")),
            (offset_of(source, "exports.b ="), circular_message("b")),
        ]
    );
}

#[test]
fn a_chain_ending_at_an_undefined_name_is_not_circular() {
    // The dangling target is a TS2339 read failure in tsc, never TS2303.
    let source = "exports.blah = exports.someProp;\n";
    assert_eq!(ts2303_sites("dangling.js", source), vec![]);
}

#[test]
fn a_two_link_chain_to_an_undefined_name_is_not_circular() {
    let source = "exports.first = exports.second;\nexports.second = exports.third;\n";
    assert_eq!(ts2303_sites("twolink.js", source), vec![]);
}

#[test]
fn a_chain_resolved_by_a_concrete_prop_stays_clean() {
    let source = "exports.a = exports.b;\nexports.b = 1;\n";
    assert_eq!(ts2303_sites("resolved.js", source), vec![]);
}

#[test]
fn an_export_assignment_mix_disables_alias_cycle_ts2303() {
    // `module.exports = X` beside the property exports is the TS2309 mix;
    // tsc stops treating the siblings as alias declarations entirely.
    let source =
        "module.exports = function () {};\nexports.a = exports.b;\nexports.b = exports.a;\n";
    let diags = check_js_positioned("mix.js", source);
    assert!(
        !has_code(&diags, 2303),
        "no TS2303 under the export-assignment mix, got {diags:?}"
    );
    assert!(
        has_code(&diags, 2309),
        "the TS2309 mix diagnostic itself still fires, got {diags:?}"
    );
}

#[test]
fn an_export_assignment_mix_disables_unresolvable_ts2303() {
    let source = "module.exports = function () {};\nexports.blah = exports.someProp;\n";
    let diags = check_js_positioned("mixdangle.js", source);
    assert!(
        !has_code(&diags, 2303),
        "no TS2303 under the export-assignment mix, got {diags:?}"
    );
    assert!(
        has_code(&diags, 2309),
        "the TS2309 mix diagnostic itself still fires, got {diags:?}"
    );
}
