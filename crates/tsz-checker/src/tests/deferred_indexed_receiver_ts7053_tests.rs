use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn numeric_index_through_deferred_tuple_alias_uses_array_constraint() {
    let codes = diagnostic_codes(
        r#"
type Delay<T> = [T][T extends any ? 0 : never];

export function read<A extends readonly unknown[]>(items: Delay<A>, index: number): A[number] {
    return items[index];
}
"#,
    );

    assert!(
        !codes.contains(&7053),
        "deferred alias should expose the array constraint for numeric indexing, got {codes:?}"
    );
}

#[test]
fn numeric_index_through_deferred_tuple_alias_is_not_name_sensitive() {
    let codes = diagnostic_codes(
        r#"
type Later<RowSet> = [RowSet][RowSet extends unknown ? 0 : never];

export function pick<Choices extends readonly string[]>(
    values: Later<Choices>,
    offset: number,
): Choices[number] {
    return values[offset];
}
"#,
    );

    assert!(
        !codes.contains(&7053),
        "renamed deferred alias should still expose the array constraint, got {codes:?}"
    );
}

#[test]
fn numeric_index_through_deferred_tuple_alias_still_rejects_non_indexable_constraint() {
    let codes = diagnostic_codes(
        r#"
type Delay<T> = [T][T extends any ? 0 : never];

export function bad<T extends {}>(item: Delay<T>, index: number) {
    return item[index];
}
"#,
    );

    assert!(
        codes.contains(&7053),
        "non-indexable constraints must still report TS7053, got {codes:?}"
    );
}
