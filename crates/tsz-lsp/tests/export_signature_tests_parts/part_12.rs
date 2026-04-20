#[test]
fn test_from_input_different_exports_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("bar".to_string(), 0x10, false)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Different export names should produce different hashes"
    );
}

#[test]
fn test_from_input_different_flags_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x20, false)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Different symbol flags should produce different hashes"
    );
}

#[test]
fn test_from_input_type_only_change_different_hash() {
    let input_a = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        exports: vec![("foo".to_string(), 0x10, true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Changing is_type_only should produce different hashes"
    );
}

#[test]
fn test_from_input_empty_is_consistent() {
    let input = ExportSignatureInput::default();
    let sig1 = ExportSignature::from_input(&input);
    let sig2 = ExportSignature::from_input(&input);

    assert_eq!(sig1, sig2, "Empty input should produce consistent hashes");
}

#[test]
fn test_from_input_wildcard_type_only_change_different_hash() {
    // Changing `export * from "x"` to `export type * from "x"` must change the signature,
    // because type-only wildcard re-exports filter out value exports from the source module.
    let input_a = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), false)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_ne!(
        sig_a, sig_b,
        "Wildcard re-export type_only change must produce different signatures"
    );
}

#[test]
fn test_from_input_wildcard_same_module_same_type_only_same_hash() {
    let input_a = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };
    let input_b = ExportSignatureInput {
        wildcard_reexports: vec![("./utils".to_string(), true)],
        ..Default::default()
    };

    let sig_a = ExportSignature::from_input(&input_a);
    let sig_b = ExportSignature::from_input(&input_b);

    assert_eq!(
        sig_a, sig_b,
        "Same wildcard re-export entries must produce same signature"
    );
}

// ── ExportSurface hash-equivalence tests ──────────────────────────────

/// Helper: parse, bind, then compute both the `from_binder()` signature
/// and the `from_surface()` signature.  Returns `(from_binder, from_surface)`.
fn compute_both_sigs(source: &str) -> (ExportSignature, ExportSignature) {
    let file_name = "test.ts";
    let mut parser = tsz_parser::ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sig_binder = ExportSignature::compute(&binder, file_name);
    let surface =
        tsz_binder::ExportSurface::from_binder(&binder, parser.get_arena(), file_name, root);
    let sig_surface = ExportSignature::from_surface(&surface);
    (sig_binder, sig_surface)
}

#[test]
fn test_surface_hash_matches_binder_simple_exports() {
    let (a, b) = compute_both_sigs("export function foo(): void {}");
    assert_eq!(a, b, "from_surface hash must match from_binder hash");
}

#[test]
fn test_surface_hash_matches_binder_multiple_exports() {
    let (a, b) = compute_both_sigs(
        "export function foo(): void {}\nexport const bar: number = 1;\nexport class Baz {}",
    );
    assert_eq!(a, b, "from_surface hash must match from_binder hash");
}

#[test]
fn test_surface_hash_matches_binder_reexports() {
    let (a, b) = compute_both_sigs("export { foo } from './other';");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for named re-exports"
    );
}

#[test]
fn test_surface_hash_matches_binder_wildcard() {
    let (a, b) = compute_both_sigs("export * from './other';");
    assert_eq!(
        a, b,
        "from_surface hash must match from_binder hash for wildcard re-exports"
    );
}

