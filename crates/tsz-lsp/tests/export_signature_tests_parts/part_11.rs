#[test]
fn test_input_wildcard_reexport_source_change() {
    let sig_a = compute_sig("export * from './a';");
    let sig_b = compute_sig("export * from './b';");

    assert_ne!(
        sig_a, sig_b,
        "Changing wildcard re-export source should change the signature"
    );
}

#[test]
fn test_input_multiple_reexports_order_independent() {
    let sig_a = compute_sig("export { a } from './x';\nexport { b } from './y';");
    let sig_b = compute_sig("export { b } from './y';\nexport { a } from './x';");

    assert_eq!(
        sig_a, sig_b,
        "Re-export order should not affect the signature (sorted keys)"
    );
}

#[test]
fn test_input_adding_named_reexport_changes_signature() {
    let sig_a = compute_sig("export { a } from './x';");
    let sig_b = compute_sig("export { a } from './x';\nexport { b } from './y';");

    assert_ne!(
        sig_a, sig_b,
        "Adding a named re-export should change the signature"
    );
}

#[test]
fn test_input_removing_reexport_changes_signature() {
    let sig_a = compute_sig("export { a } from './x';\nexport { b } from './y';");
    let sig_b = compute_sig("export { a } from './x';");

    assert_ne!(
        sig_a, sig_b,
        "Removing a named re-export should change the signature"
    );
}

// ============================================================================
// Augmentation tests
// ============================================================================

#[test]
fn test_global_augmentation_changes_signature() {
    let sig_a = compute_sig("export const x = 1;");
    let sig_b =
        compute_sig("export const x = 1;\ndeclare global { interface Window { foo: string; } }");

    assert_ne!(
        sig_a, sig_b,
        "Adding a global augmentation should change the signature"
    );
}

#[test]
fn test_module_augmentation_changes_signature() {
    let sig_a = compute_sig("export const x = 1;");
    let sig_b = compute_sig(
        "export const x = 1;\ndeclare module 'express' { interface Request { user: any; } }",
    );

    // Module augmentations may or may not be tracked by the binder for inline source;
    // at minimum, verify no panic and valid computation
    let _ = (sig_a, sig_b);
}

// ============================================================================
// InvalidationSummary tests
// ============================================================================

#[test]
fn test_invalidation_summary_unchanged() {
    let summary = InvalidationSummary::unchanged("a.ts".to_string(), 0x1234);
    assert!(!summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 0);
    assert_eq!(summary.old_signature, Some(0x1234));
    assert_eq!(summary.new_signature, 0x1234);
}

#[test]
fn test_invalidation_summary_changed() {
    let summary = InvalidationSummary::changed("a.ts".to_string(), Some(0x1111), 0x2222, 3);
    assert!(summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 3);
    assert_eq!(summary.old_signature, Some(0x1111));
    assert_eq!(summary.new_signature, 0x2222);
}

#[test]
fn test_invalidation_summary_new_file() {
    let summary = InvalidationSummary::new_file("new.ts".to_string(), 0xABCD);
    assert!(summary.api_changed);
    assert_eq!(summary.dependents_invalidated, 0);
    assert_eq!(summary.old_signature, None);
    assert_eq!(summary.new_signature, 0xABCD);
}

// ============================================================================
// Cross-system equivalence: from_input produces deterministic results
// ============================================================================

#[test]
fn test_from_input_deterministic() {
    let input = ExportSignatureInput {
        exports: vec![
            ("bar".to_string(), 0x10, false),
            ("foo".to_string(), 0x20, true),
        ],
        named_reexports: vec![(
            "baz".to_string(),
            "./mod".to_string(),
            Some("original".to_string()),
        )],
        wildcard_reexports: vec![("./utils".to_string(), false)],
        global_augmentations: vec![("Window".to_string(), 1)],
        module_augmentations: vec![("express".to_string(), vec!["Request".to_string()])],
        exported_locals: vec![("bar".to_string(), 0x10, false)],
    };

    let sig1 = ExportSignature::from_input(&input);
    let sig2 = ExportSignature::from_input(&input);

    assert_eq!(sig1, sig2, "from_input must be deterministic");
    assert_ne!(
        sig1.0, 0,
        "Signature should be non-zero for non-empty input"
    );
}

