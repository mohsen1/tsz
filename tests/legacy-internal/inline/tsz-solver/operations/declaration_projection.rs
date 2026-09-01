//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/declaration_projection.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c1d9534207a12ba7a520793475aed805243b806e9eb2775af7ae87a81d966823 466 bare_any_projects_only_when_read
    #[test]
    fn bare_any_projects_only_when_read() {
        let db = TypeInterner::new();
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Covariant),
            TypeId::UNKNOWN,
            "read position projects any -> unknown"
        );
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Contravariant),
            TypeId::ANY,
            "write position keeps any permissive"
        );
        assert_eq!(
            project_declaration_boundary(&db, TypeId::ANY, Polarity::Invariant),
            TypeId::ANY,
            "invariant position is left unchanged"
        );
    }
// TSZ_INLINE_TEST_END c1d9534207a12ba7a520793475aed805243b806e9eb2775af7ae87a81d966823

// TSZ_INLINE_TEST_BEGIN 55ed89d35e71181c579abb4e16d4a083b0fb3177b13ea53466feaef39a936fbf 486 non_any_leaf_is_unchanged
    #[test]
    fn non_any_leaf_is_unchanged() {
        let db = TypeInterner::new();
        assert_eq!(
            project_declaration_boundary(&db, TypeId::STRING, Polarity::Covariant),
            TypeId::STRING
        );
        let clean = func(&db, vec![param(TypeId::STRING)], TypeId::NUMBER);
        assert_eq!(
            project_declaration_boundary(&db, clean, Polarity::Covariant),
            clean,
            "a function with no any returns the identical TypeId"
        );
    }
// TSZ_INLINE_TEST_END 55ed89d35e71181c579abb4e16d4a083b0fb3177b13ea53466feaef39a936fbf

// TSZ_INLINE_TEST_BEGIN 047b9469324ed2661bc6d1a305d86996384a86efc30e13c5d69ad5d6038bb265 501 function_return_is_read_position
    #[test]
    fn function_return_is_read_position() {
        let db = TypeInterner::new();
        // `(x: any) => any`: return projects, parameter stays permissive.
        let f = func(&db, vec![param(TypeId::ANY)], TypeId::ANY);
        let projected = project_declaration_boundary(&db, f, Polarity::Covariant);
        assert_eq!(function_return(&db, projected), TypeId::UNKNOWN);
        assert_eq!(
            function_param(&db, projected, 0),
            TypeId::ANY,
            "parameter is a write position and keeps any"
        );
    }
// TSZ_INLINE_TEST_END 047b9469324ed2661bc6d1a305d86996384a86efc30e13c5d69ad5d6038bb265

// TSZ_INLINE_TEST_BEGIN bd709a653d3538f3b24bc8e40c98d1e9fe87d6f55f9953b8c5218316b70505ec 515 function_projected_contravariantly_keeps_return_any
    #[test]
    fn function_projected_contravariantly_keeps_return_any() {
        let db = TypeInterner::new();
        let f = func(&db, vec![], TypeId::ANY);
        // As a write position the whole function flips: its return is now
        // contravariant, so the any stays.
        assert_eq!(
            project_declaration_boundary(&db, f, Polarity::Contravariant),
            f
        );
    }
// TSZ_INLINE_TEST_END bd709a653d3538f3b24bc8e40c98d1e9fe87d6f55f9953b8c5218316b70505ec

// TSZ_INLINE_TEST_BEGIN c89c281c91f44b8d044b509a010fa96640fd807ea70deb8ddd0b6e24e15fa9c7 527 library_supplied_callback_parameter_is_a_read_position
    #[test]
    fn library_supplied_callback_parameter_is_a_read_position() {
        let db = TypeInterner::new();
        // `(cb: (value: any) => void) => void`. The callback parameter is a
        // value the library pushes into user code, so `value` must project to
        // `unknown` via the double polarity flip.
        let callback = func(&db, vec![param(TypeId::ANY)], TypeId::VOID);
        let outer = func(&db, vec![param(callback)], TypeId::VOID);
        let projected = project_declaration_boundary(&db, outer, Polarity::Covariant);
        let projected_callback = function_param(&db, projected, 0);
        assert_eq!(
            function_param(&db, projected_callback, 0),
            TypeId::UNKNOWN,
            "callback parameter supplied to user code projects any -> unknown"
        );
    }
// TSZ_INLINE_TEST_END c89c281c91f44b8d044b509a010fa96640fd807ea70deb8ddd0b6e24e15fa9c7

// TSZ_INLINE_TEST_BEGIN 837d41873d8a2fabe62a3bfc7c964784dfa8717d0ab8ea386817e1c049b64cc4 544 object_property_splits_read_and_write
    #[test]
    fn object_property_splits_read_and_write() {
        let db = TypeInterner::new();
        let value = db.intern_string("value");
        let obj = db.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
        let projected = project_declaration_boundary(&db, obj, Polarity::Covariant);
        let shape = match db.lookup(projected) {
            Some(TypeData::Object(shape_id)) => db.object_shape(shape_id),
            other => panic!("expected object, got {other:?}"),
        };
        let prop = &shape.properties[0];
        assert_eq!(
            prop.type_id,
            TypeId::UNKNOWN,
            "read side projects to unknown"
        );
        assert_eq!(
            prop.write_type,
            TypeId::ANY,
            "write side stays permissive (any)"
        );
    }
// TSZ_INLINE_TEST_END 837d41873d8a2fabe62a3bfc7c964784dfa8717d0ab8ea386817e1c049b64cc4

// TSZ_INLINE_TEST_BEGIN 2ee2b3d0eec523fb48d0cfdfd5db60f0a37b838cd115adaa81b8124d82039669 567 readonly_property_projects_read_side
    #[test]
    fn readonly_property_projects_read_side() {
        let db = TypeInterner::new();
        let value = db.intern_string("value");
        let mut prop = PropertyInfo::new(value, TypeId::ANY);
        prop.readonly = true;
        let obj = db.object(vec![prop]);
        let projected = project_declaration_boundary(&db, obj, Polarity::Covariant);
        let shape = match db.lookup(projected) {
            Some(TypeData::Object(shape_id)) => db.object_shape(shape_id),
            other => panic!("expected object, got {other:?}"),
        };
        assert_eq!(shape.properties[0].type_id, TypeId::UNKNOWN);
    }
// TSZ_INLINE_TEST_END 2ee2b3d0eec523fb48d0cfdfd5db60f0a37b838cd115adaa81b8124d82039669
