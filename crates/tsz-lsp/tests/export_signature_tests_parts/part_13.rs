#[test]
fn test_surface_hash_matches_binder_default_export() {
    let (a, b) = compute_both_sigs("export default function main() {}");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for default exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_type_only() {
    let (a, b) = compute_both_sigs("export type Foo = string;");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for type-only exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_mixed() {
    let (a, b) = compute_both_sigs(
        r#"
export function foo(): void {}
export type Bar = number;
export { baz } from './baz';
export * from './utils';
export default class Main {}
"#,
    );
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for mixed exports"
    );
}
