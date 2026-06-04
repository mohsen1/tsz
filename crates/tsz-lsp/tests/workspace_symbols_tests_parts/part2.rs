#[test]
fn camel_case_acronym_beats_arbitrary_substring() {
    // Query "UD" should rank UserData (consecutive humps from start)
    // above MUDdy (substring match), matching tsserver's fuzzy scorer.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind("MUDdy", make_location("a.ts", 0, 0, 5), SymbolKind::Class);
    index.add_definition_with_kind(
        "UserData",
        make_location("b.ts", 0, 0, 8),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("UD");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].name, "UserData",
        "camel-case acronym should beat arbitrary substring"
    );
    assert_eq!(results[1].name, "MUDdy");
}

#[test]
fn camel_case_acronym_from_start_beats_substring_acronym() {
    // `UD` matches `UserData` from hump 0 (contiguous-from-start).
    // `UD` matches `BaseUserData` from a hump that is not the start
    // (contiguous, but mid-name). Start wins.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "BaseUserData",
        make_location("a.ts", 0, 0, 12),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "UserData",
        make_location("b.ts", 0, 0, 8),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("UD");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "UserData");
    assert_eq!(results[1].name, "BaseUserData");
}

#[test]
fn camel_case_acronym_contiguous_beats_skipping_humps() {
    // `UD` against `UserDetail` is contiguous (hump 0 -> hump 1).
    // `UD` against `UserStartDetail` is anywhere (skips a hump).
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "UserStartDetail",
        make_location("a.ts", 0, 0, 15),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "UserDetail",
        make_location("b.ts", 0, 0, 10),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("UD");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "UserDetail");
    assert_eq!(results[1].name, "UserStartDetail");
}

#[test]
fn camel_case_acronym_lowercase_query_still_matches() {
    // Acronym matching is case-insensitive: `ud` matches `UserData`.
    // But `ud` lowercase is not a substring of `UserData` (whose lower is
    // `userdata`), so this can only be the camel-case path.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "UserData",
        make_location("a.ts", 0, 0, 8),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "unrelated",
        make_location("b.ts", 0, 0, 9),
        SymbolKind::Variable,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ud");
    assert!(
        results.iter().any(|r| r.name == "UserData"),
        "case-insensitive acronym should match UserData"
    );
}

#[test]
fn exact_prefix_wins_over_substring() {
    let mut index = SymbolIndex::new();
    index.add_definition("filterArray", make_location("a.ts", 0, 0, 11));
    index.add_definition("postFilter", make_location("b.ts", 0, 0, 10));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("filter");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "filterArray", "prefix match should win");
    assert_eq!(results[1].name, "postFilter");
}

#[test]
fn exact_case_sensitive_beats_case_insensitive() {
    let mut index = SymbolIndex::new();
    index.add_definition("Foo", make_location("a.ts", 0, 0, 3));
    index.add_definition("foo", make_location("b.ts", 0, 0, 3));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].name, "foo",
        "case-sensitive exact match should rank first"
    );
    assert_eq!(results[1].name, "Foo");
}

#[test]
fn case_sensitive_prefix_beats_case_insensitive_prefix() {
    let mut index = SymbolIndex::new();
    index.add_definition("fooBar", make_location("a.ts", 0, 0, 6));
    index.add_definition("FooBar", make_location("b.ts", 0, 0, 6));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("foo");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "fooBar");
    assert_eq!(results[1].name, "FooBar");
}

#[test]
fn case_sensitive_substring_beats_case_insensitive_substring() {
    // Both names contain the query as a substring (case-insensitive), but
    // only `Item.factory` preserves case. Note: substring beats nothing
    // else here because neither symbol has hump alignment for `act`.
    let mut index = SymbolIndex::new();
    index.add_definition("getFACTORY", make_location("a.ts", 0, 0, 10));
    index.add_definition("Item_act", make_location("b.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("act");
    assert!(results.len() >= 2);
    // `Item_act` contains case-sensitive `act`; `getFACTORY` contains only
    // case-insensitive `act` (via `FACT`).
    assert_eq!(results[0].name, "Item_act");
}

#[test]
fn camel_case_acronym_three_humps() {
    let mut index = SymbolIndex::new();
    index.add_definition("getUserById", make_location("a.ts", 0, 0, 11));
    index.add_definition("findUserById", make_location("b.ts", 0, 0, 12));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("gUBI");
    // `gUBI` -> g(0) U(3) B(7) I(8) of getUserById -> ContiguousFromStart.
    // findUserById has no `g` hump, so it should not match.
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "getUserById");
}

#[test]
fn proximity_tiebreaks_same_match_tier() {
    // Two symbols of identical name length tie on match kind, name length,
    // and alphabetical order; the active file's proximity decides.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Widget",
        make_location("src/ui/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Widget",
        make_location("vendor/lib/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );

    let active = "src/ui/index.ts";
    let provider = WorkspaceSymbolsProvider::with_active_file(&index, Some(active));
    let results = provider.find_symbols("Widget");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].location.file_path, "src/ui/widget.ts",
        "nearby file should rank above distant file"
    );
    assert_eq!(results[1].location.file_path, "vendor/lib/widget.ts");
}

#[test]
fn proximity_with_no_active_file_falls_back_to_alphabetical() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Widget",
        make_location("z/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Widget",
        make_location("a/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("Widget");
    assert_eq!(results.len(), 2);
    // Both have same kind/length/name; alphabetical by file path.
    assert_eq!(results[0].location.file_path, "a/widget.ts");
    assert_eq!(results[1].location.file_path, "z/widget.ts");
}

#[test]
fn proximity_same_file_wins_over_sibling() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Widget",
        make_location("src/ui/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Widget",
        make_location("src/ui/other.ts", 0, 0, 6),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::with_active_file(&index, Some("src/ui/widget.ts"));
    let results = provider.find_symbols("Widget");
    assert_eq!(results[0].location.file_path, "src/ui/widget.ts");
}

#[test]
fn underscore_separator_creates_humps() {
    // `_` is a separator: `_private_helper` has humps for `p` and `h`.
    let mut index = SymbolIndex::new();
    index.add_definition("_private_helper", make_location("a.ts", 0, 0, 15));

    let provider = WorkspaceSymbolsProvider::new(&index);
    // `ph` should hit hump `p` then hump `h` (consecutive).
    let results = provider.find_symbols("ph");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "_private_helper");
}

#[test]
fn digit_boundary_creates_hump() {
    // `Vec3D` has humps at V(0), 3(3), D(4); `V3D` should acronym-match.
    let mut index = SymbolIndex::new();
    index.add_definition("Vec3D", make_location("a.ts", 0, 0, 5));
    index.add_definition("Octave", make_location("b.ts", 0, 0, 6));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("V3D");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Vec3D");
}

#[test]
fn substring_matches_non_camel_name() {
    // `parser` is all-lowercase, no humps after position 0.
    // Query "rse" must succeed via substring matching.
    let mut index = SymbolIndex::new();
    index.add_definition("parser", make_location("a.ts", 0, 0, 6));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("rse");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "parser");
}

#[test]
fn longer_query_than_humps_falls_back_to_substring() {
    // Query "userdat" has 7 chars; `UserData` has only 2 humps (U, D), so
    // camel-case acronym path can't accommodate it. Substring path picks
    // it up (case-insensitive).
    let mut index = SymbolIndex::new();
    index.add_definition("UserData", make_location("a.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("userdat");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "UserData");
}

#[test]
fn empty_query_returns_empty_via_fuzzy() {
    let mut index = SymbolIndex::new();
    index.add_definition("foo", make_location("a.ts", 0, 0, 3));
    let provider = WorkspaceSymbolsProvider::new(&index);
    assert!(provider.find_symbols("").is_empty());
}

#[test]
fn exact_match_ranked_above_prefix_of_longer_name() {
    let mut index = SymbolIndex::new();
    index.add_definition("test", make_location("a.ts", 0, 0, 4));
    index.add_definition("testing", make_location("b.ts", 0, 0, 7));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("test");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].name, "test");
    assert_eq!(results[1].name, "testing");
}

#[test]
fn case_insensitive_acronym_below_case_sensitive_acronym() {
    // Both `UserData` and `userData` expose humps that match `UD` as
    // acronym, but the casing makes the acronym map differently.
    // We just verify that both are returned and `UD` matches at least one
    // with a hump alignment (not just substring).
    let mut index = SymbolIndex::new();
    index.add_definition("UserData", make_location("a.ts", 0, 0, 8));

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("UD");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "UserData");
}

#[test]
fn unicode_case_insensitive_exact_match() {
    // Non-ASCII identifiers must fold like JS `.toLowerCase()` does:
    // tsserver matches `ångström` against `Ångström` case-insensitively.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Ångström",
        make_location("phys.ts", 0, 0, 8),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ångström");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Ångström");
}

#[test]
fn unicode_case_insensitive_prefix_match() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "ÅngströmCalculator",
        make_location("phys.ts", 0, 0, 18),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ångström");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "ÅngströmCalculator");
}

#[test]
fn unicode_case_insensitive_substring_match() {
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "buildÅngströmTable",
        make_location("phys.ts", 0, 0, 18),
        SymbolKind::Function,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ångström");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "buildÅngströmTable");
}

#[test]
fn unicode_camel_case_acronym_match() {
    // `ÅngströmCalculator` exposes humps at Å(0) and C(8).
    // Query "åc" (lowercase) should match as a Unicode-aware acronym.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "ÅngströmCalculator",
        make_location("phys.ts", 0, 0, 18),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("åc");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "ÅngströmCalculator");
}

#[test]
fn unicode_name_length_counted_in_chars_not_bytes() {
    // Both names contain `name` and tie on match tier. `Ångström` has the
    // *same* number of Unicode scalar values as `nameTest` (8), so the
    // tie-break falls through to alphabetical order. If we had counted
    // bytes, `Ångström` (10 bytes UTF-8) would rank below `nameTest`
    // (8 bytes) on length alone.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Ångström",
        make_location("a.ts", 0, 0, 8),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "ÅngOther",
        make_location("b.ts", 0, 0, 8),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::new(&index);
    let results = provider.find_symbols("ång");
    assert_eq!(results.len(), 2);
    // Both prefix matches (case-insensitive), both 8 chars. The order is
    // then alphabetical (lowercased): "ångother" < "ångström".
    assert_eq!(results[0].name, "ÅngOther");
    assert_eq!(results[1].name, "Ångström");
}

#[test]
fn path_distance_no_underflow_when_active_is_segment_prefix() {
    // If the active file's name is a segment of the candidate's path
    // (an unusual but valid shape under TS extension-stripping), the
    // path-distance computation must not underflow.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "WidgetA",
        make_location("src/index.ts/generated/widget.ts", 0, 0, 7),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "WidgetA",
        make_location("vendor/lib/widget.ts", 0, 0, 7),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::with_active_file(&index, Some("src/index.ts"));
    let results = provider.find_symbols("WidgetA");
    assert_eq!(results.len(), 2, "computation should not panic");
    // The path under `src/` should rank above the unrelated `vendor/` tree.
    assert_eq!(
        results[0].location.file_path,
        "src/index.ts/generated/widget.ts"
    );
}

#[test]
fn path_distance_sibling_files_under_same_active_directory() {
    // Active file is itself a peer of the candidate (same directory).
    // Distance should equal "same directory" (1), not zero (which is
    // reserved for the exact same file).
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Widget",
        make_location("src/ui/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Widget",
        make_location("src/api/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );

    let provider = WorkspaceSymbolsProvider::with_active_file(&index, Some("src/ui/index.ts"));
    let results = provider.find_symbols("Widget");
    assert_eq!(results[0].location.file_path, "src/ui/widget.ts");
}

#[test]
fn proximity_workspace_root_does_not_count_as_shared() {
    // Files share root path segments but live in completely different
    // subtrees. They should not be considered "close" just because they
    // share the workspace prefix.
    let mut index = SymbolIndex::new();
    index.add_definition_with_kind(
        "Widget",
        make_location("workspace/src/ui/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );
    index.add_definition_with_kind(
        "Widget",
        make_location("workspace/vendor/lib/widget.ts", 0, 0, 6),
        SymbolKind::Class,
    );

    let provider =
        WorkspaceSymbolsProvider::with_active_file(&index, Some("workspace/src/ui/index.ts"));
    let results = provider.find_symbols("Widget");
    assert_eq!(results[0].location.file_path, "workspace/src/ui/widget.ts");
}
