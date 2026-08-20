//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-binder/src/state/declaration_summary.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2e223c1039f99a87393a3fab46abed5f5c8090e09d3d6b594b7b6776dad25652 77 default_summary_has_no_public_surface_facts
    #[test]
    fn default_summary_has_no_public_surface_facts() {
        let summary = DeclarationSummary::default();

        assert!(summary.overloaded_functions().is_empty());
        assert!(!summary.has_public_api_scope());
        assert_eq!(summary.public_api_size(), 0);
    }
// TSZ_INLINE_TEST_END 2e223c1039f99a87393a3fab46abed5f5c8090e09d3d6b594b7b6776dad25652

// TSZ_INLINE_TEST_BEGIN bb3f1955baaf34e4f99d96119e056c54a342ae0a6441c1be6709e2f39f98c5c4 86 summary_exposes_export_surface_overload_facts
    #[test]
    fn summary_exposes_export_surface_overload_facts() {
        let mut export_surface = ExportSurface::default();
        export_surface
            .overloaded_functions
            .insert("parse".to_string());
        export_surface.has_public_api_scope = true;
        let summary = DeclarationSummary::from_export_surface(export_surface);

        assert!(summary.overloaded_functions().contains("parse"));
        assert!(summary.has_public_api_scope());
    }
// TSZ_INLINE_TEST_END bb3f1955baaf34e4f99d96119e056c54a342ae0a6441c1be6709e2f39f98c5c4

// TSZ_INLINE_TEST_BEGIN ea780241095d1c20c437a65445080e88d502c3b9a33bfe483d8a223d96ee3b32 99 summary_wraps_export_surface_queries
    #[test]
    fn summary_wraps_export_surface_queries() {
        let mut export_surface = ExportSurface::default();
        export_surface.module_exports.insert(
            "PublicType".to_string(),
            crate::ExportedSymbol {
                symbol_id: crate::SymbolId(1),
                flags: 0,
                is_type_only: true,
            },
        );
        let summary = DeclarationSummary::from_export_surface(export_surface);

        assert!(summary.is_exported("PublicType"));
        assert!(summary.is_type_only_export("PublicType"));
        assert_eq!(summary.public_api_size(), 1);
    }
// TSZ_INLINE_TEST_END ea780241095d1c20c437a65445080e88d502c3b9a33bfe483d8a223d96ee3b32
