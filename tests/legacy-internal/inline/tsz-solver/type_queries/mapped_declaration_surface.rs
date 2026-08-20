//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/mapped_declaration_surface.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bff4b54cf3c139ed4c3b8dac32db3447e3cf406e7345c0cf0f8b3c4f72dcadda 383 optional_surface_depth_allows_below_cap
    #[test]
    fn optional_surface_depth_allows_below_cap() {
        assert_eq!(
            mapped_surface_optional_depth_state(MAX_MAPPED_SURFACE_OPTIONAL_DEPTH - 1),
            MappedSurfaceOptionalDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END bff4b54cf3c139ed4c3b8dac32db3447e3cf406e7345c0cf0f8b3c4f72dcadda

// TSZ_INLINE_TEST_BEGIN 3d378005f6a18b942269962a94810cefda3fa8b8931ed496f9e326ae318fd532 391 optional_surface_depth_limits_at_cap
    #[test]
    fn optional_surface_depth_limits_at_cap() {
        assert_eq!(
            mapped_surface_optional_depth_state(MAX_MAPPED_SURFACE_OPTIONAL_DEPTH),
            MappedSurfaceOptionalDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 3d378005f6a18b942269962a94810cefda3fa8b8931ed496f9e326ae318fd532

// TSZ_INLINE_TEST_BEGIN 0a8d5545b51b4266208aa9768526a8a6e4935815c5d1c601d92facae05779f66 399 optional_surface_depth_cap_preserves_surface
    #[test]
    fn optional_surface_depth_cap_preserves_surface() {
        let db = TypeInterner::new();
        let prop = db.intern_string("value");
        let surface = db.object(vec![PropertyInfo::opt(prop, TypeId::NUMBER)]);

        let limited = mapped_surface_with_optional_undefined_inner(
            &db,
            surface,
            MAX_MAPPED_SURFACE_OPTIONAL_DEPTH,
        );

        assert_eq!(limited, surface);
    }
// TSZ_INLINE_TEST_END 0a8d5545b51b4266208aa9768526a8a6e4935815c5d1c601d92facae05779f66

// TSZ_INLINE_TEST_BEGIN 205a5682d4d6e59a1a081000185516bc7a31dfab9b5f7711988d3b5b20007a16 414 optional_surface_below_depth_cap_adds_undefined
    #[test]
    fn optional_surface_below_depth_cap_adds_undefined() {
        let db = TypeInterner::new();
        let prop = db.intern_string("value");
        let surface = db.object(vec![PropertyInfo::opt(prop, TypeId::NUMBER)]);

        let mapped = mapped_surface_with_optional_undefined_inner(
            &db,
            surface,
            MAX_MAPPED_SURFACE_OPTIONAL_DEPTH - 1,
        );

        let Some(TypeData::Object(shape_id)) = db.lookup(mapped) else {
            panic!("expected mapped object surface");
        };
        let shape = db.object_shape(shape_id);
        let value = shape
            .properties
            .iter()
            .find(|property| property.name == prop)
            .expect("optional property must survive");
        assert!(super::super::type_includes_undefined(&db, value.type_id));
    }
// TSZ_INLINE_TEST_END 205a5682d4d6e59a1a081000185516bc7a31dfab9b5f7711988d3b5b20007a16
