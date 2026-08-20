//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/property_checker.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a5045e29229c33f21fd457458d56b966b3edde30c4660b82a562e28d113f09ee 1261 union_restricted_property_access_missing_member_emits_ts2339
    #[test]
    fn union_restricted_property_access_missing_member_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class A {
                readonly x: number = 0;
            }
            class B {
                y: string = "";
            }
            let value: A | B;
            value.x;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
    }
// TSZ_INLINE_TEST_END a5045e29229c33f21fd457458d56b966b3edde30c4660b82a562e28d113f09ee

// TSZ_INLINE_TEST_BEGIN 97669c77a5c5d042cab278e9438ac4fd61627f761ace6747e830d9363f2b7802 1279 union_restricted_property_access_same_declaring_class_is_allowed
    #[test]
    fn union_restricted_property_access_same_declaring_class_is_allowed() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                x: number = 0;
            }
            class Derived extends Base {
                y: string = "";
            }
            let value: Base | Derived;
            value.x;
        "#,
        );

        assert!(!has_code(&diagnostics, 2339));
    }
// TSZ_INLINE_TEST_END 97669c77a5c5d042cab278e9438ac4fd61627f761ace6747e830d9363f2b7802

// TSZ_INLINE_TEST_BEGIN cb2657b1f76a3f1720108e7824687f1fbc74fcbf3a52f4d569abccdf4410ccba 1297 union_restricted_property_access_different_decls_emits_ts2339
    #[test]
    fn union_restricted_property_access_different_decls_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class A {
                private x: number = 0;
            }
            class B {
                private x: number = 1;
            }
            let value: A | B;
            value.x;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
    }
// TSZ_INLINE_TEST_END cb2657b1f76a3f1720108e7824687f1fbc74fcbf3a52f4d569abccdf4410ccba

// TSZ_INLINE_TEST_BEGIN 7d5c442b84cd9fffd2811e505bdf4174d996a955393a236ac3eef5052fc1083e 1318 union_public_and_protected_emits_ts2339_not_ts2445
    /// When a union has one public member and one protected member,
    /// TSC treats the property as "not existing" on the union (TS2339).
    /// Previously this was order-dependent and emitted TS2445 instead.
    #[test]
    fn union_public_and_protected_emits_ts2339_not_ts2445() {
        let diagnostics = check_diagnostics(
            r#"
            class Default {
                member: string = "";
            }
            class Protected {
                protected member: string = "";
            }
            declare var v: Default | Protected;
            v.member;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2339),
            "expected TS2339 for union with public + protected"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "should NOT emit TS2445 for union type"
        );
    }
// TSZ_INLINE_TEST_END 7d5c442b84cd9fffd2811e505bdf4174d996a955393a236ac3eef5052fc1083e

// TSZ_INLINE_TEST_BEGIN a024025d235d67dcbe6f04371dc2f6a542f04bad8bc32a97867eb03ad3da1b3f 1345 union_public_and_private_emits_ts2339_not_ts2341
    /// When a union has one public member and one private member,
    /// TSC emits TS2339, not TS2341.
    #[test]
    fn union_public_and_private_emits_ts2339_not_ts2341() {
        let diagnostics = check_diagnostics(
            r#"
            class Public {
                public member: string = "";
            }
            class Private {
                private member: number = 0;
            }
            declare var v: Public | Private;
            v.member;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2339),
            "expected TS2339 for union with public + private"
        );
        assert!(
            !has_code(&diagnostics, 2341),
            "should NOT emit TS2341 for union type"
        );
    }
// TSZ_INLINE_TEST_END a024025d235d67dcbe6f04371dc2f6a542f04bad8bc32a97867eb03ad3da1b3f

// TSZ_INLINE_TEST_BEGIN 27c34af16ad96aacbf6da8d787e132d86747961cdaee3821b173be2dbec14d2a 1372 union_three_member_mixed_access_emits_ts2339
    /// Three-member union with mix of public, protected, private.
    /// All should get TS2339.
    #[test]
    fn union_three_member_mixed_access_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class Default { member: string = ""; }
            class Public { public member: string = ""; }
            class Protected { protected member: string = ""; }
            declare var v: Default | Public | Protected;
            v.member;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
        assert!(!has_code(&diagnostics, 2445));
    }
// TSZ_INLINE_TEST_END 27c34af16ad96aacbf6da8d787e132d86747961cdaee3821b173be2dbec14d2a

// TSZ_INLINE_TEST_BEGIN a3cbbfb6689e17940ced9a5144d616839351de3830784610aeff86d860a233d5 1389 union_both_public_no_error
    /// Union of two public members — no error expected.
    #[test]
    fn union_both_public_no_error() {
        let diagnostics = check_diagnostics(
            r#"
            class A { member: string = ""; }
            class B { public member: string = ""; }
            declare var v: A | B;
            v.member;
        "#,
        );

        assert!(!has_code(&diagnostics, 2339));
        assert!(!has_code(&diagnostics, 2445));
        assert!(!has_code(&diagnostics, 2341));
    }
// TSZ_INLINE_TEST_END a3cbbfb6689e17940ced9a5144d616839351de3830784610aeff86d860a233d5

// TSZ_INLINE_TEST_BEGIN 3395a43102d2b76d4a586ad9f1028feeaa8a06095f4a9c71795229788bea5e53 1405 intersection_protected_and_public_property_is_public
    #[test]
    fn intersection_protected_and_public_property_is_public() {
        let diagnostics = check_diagnostics(
            r#"
            class Protected {
                protected member: string = "";
            }
            class Public {
                public member: string = "";
            }
            declare var value: Protected & Public;
            value.member;
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2339),
            "property should exist on protected/public intersection, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "public constituent should make the intersection property publicly accessible, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 3395a43102d2b76d4a586ad9f1028feeaa8a06095f4a9c71795229788bea5e53

// TSZ_INLINE_TEST_BEGIN b2ed4005882c8fb4a071ddc2eb956fc2822be4dbdfb87b1e4d3ad7f166e98db8 1430 intersection_all_protected_property_stays_protected
    #[test]
    fn intersection_all_protected_property_stays_protected() {
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Protected {
                protected member: string = "";
            }
            class Protected2 {
                protected member: string = "";
            }
            declare var value: Protected & Protected2;
            value.member;
        "#,
        );

        assert!(
            diagnostics.iter().any(|(code, message)| {
                *code == 2445
                    && message.contains("Property 'member' is protected")
                    && message.contains("Protected & Protected2")
            }),
            "expected TS2445 against the protected intersection owner, got: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2339),
            "protected intersection member should not fall back to missing property, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END b2ed4005882c8fb4a071ddc2eb956fc2822be4dbdfb87b1e4d3ad7f166e98db8

// TSZ_INLINE_TEST_BEGIN 1bd6efae05322818327723146e57810bcbff9e625eb761f093dd494ee5c3bb40 1459 intersection_private_and_non_private_property_reports_never
    #[test]
    fn intersection_private_and_non_private_property_reports_never() {
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Private {
                private member: string = "";
            }
            class Public {
                public member: string = "";
            }
            declare var value: Private & Public;
            value.member;
        "#,
        );

        assert!(
            diagnostics.iter().any(|(code, message)| {
                *code == 2339 && message.contains("does not exist on type 'never'")
            }),
            "private/public intersection should report a missing property on never, got: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2341),
            "private/public intersection should not report direct private access, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 1bd6efae05322818327723146e57810bcbff9e625eb761f093dd494ee5c3bb40

// TSZ_INLINE_TEST_BEGIN 6a63df5ff094cba332d1509d237a5e0e0d3ade648a5855d2e4cd5c7840381603 1486 intersection_private_public_conflict_generic_class_outside_class
    #[test]
    fn intersection_private_public_conflict_generic_class_outside_class() {
        // Verify that the check works for generic Application types outside the class.
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Container<T> {
                private value: T | null = null;
            }
            declare var c: Container<string> & { value: string };
            c.value;
        "#,
        );
        assert!(
            diagnostics.iter().any(|(code, message)| {
                *code == 2339 && message.contains("does not exist on type 'never'")
            }),
            "generic class private/public intersection outside class should report TS2339, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 6a63df5ff094cba332d1509d237a5e0e0d3ade648a5855d2e4cd5c7840381603

// TSZ_INLINE_TEST_BEGIN f845ccad9e8ffe7ed5bbade6945f5856f638449c588d347a19c4b67f3c5fad86 1506 intersection_private_public_conflict_inside_class_method_errors
    #[test]
    fn intersection_private_public_conflict_inside_class_method_errors() {
        // When `this` is narrowed via a type predicate to an intersection that
        // has a private property from the class AND a public property with the
        // same name, the intersection reduces to `never` even inside the class.
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Container<T> {
                constructor(private value: T | null) {}

                hasValue(): this is Container<T> & { value: T } {
                    return this.value !== null;
                }

                getValue(): T | null {
                    if (this.hasValue()) {
                        return this.value;
                    }
                    return null;
                }
            }
        "#,
        );

        assert!(
            diagnostics.iter().any(|(code, message)| {
                *code == 2339 && message.contains("does not exist on type 'never'")
            }),
            "narrowed this-intersection with private/public conflict should report TS2339, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END f845ccad9e8ffe7ed5bbade6945f5856f638449c588d347a19c4b67f3c5fad86

// TSZ_INLINE_TEST_BEGIN 8fe65103b29607abdc26c08ca210d848fdd162ce2dfeb929619a0e9694d304de 1538 intersection_private_public_conflict_inside_class_different_type_param_name
    #[test]
    fn intersection_private_public_conflict_inside_class_different_type_param_name() {
        // Same structural rule, different type-parameter spelling — proves the fix
        // is not keyed to a specific identifier name.
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Box<U> {
                constructor(private item: U | null) {}

                filled(): this is Box<U> & { item: U } {
                    return this.item !== null;
                }

                unwrap(): U | null {
                    if (this.filled()) {
                        return this.item;
                    }
                    return null;
                }
            }
        "#,
        );

        assert!(
            diagnostics.iter().any(|(code, message)| {
                *code == 2339 && message.contains("does not exist on type 'never'")
            }),
            "narrowed this-intersection (Box/item) should report TS2339, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 8fe65103b29607abdc26c08ca210d848fdd162ce2dfeb929619a0e9694d304de

// TSZ_INLINE_TEST_BEGIN a97b11e511fffc370839006cf678166a0e92b413158f30e509cb11f7cbabf40f 1569 intersection_same_private_from_same_class_inside_method_no_error
    #[test]
    fn intersection_same_private_from_same_class_inside_method_no_error() {
        // When both sides of an intersection have the SAME private property
        // from the same declaring class, there is no conflict and no error.
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class A {
                private x: number = 0;
                method() {
                    const self: A & A = this as any;
                    self.x;
                }
            }
        "#,
        );

        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2339),
            "same-class private intersection should not produce TS2339, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END a97b11e511fffc370839006cf678166a0e92b413158f30e509cb11f7cbabf40f

// TSZ_INLINE_TEST_BEGIN 996ed6311d3afd336c12ae789515265e3528d0714f0a51cf39e0c301cbd87e55 1591 normal_private_access_inside_class_no_error
    #[test]
    fn normal_private_access_inside_class_no_error() {
        // Removing the enclosing_class guard must not break ordinary private
        // property access from within the declaring class.
        let diagnostics = crate::test_utils::check_source_code_messages(
            r#"
            class Simple {
                private x: number = 0;
                getX() { return this.x; }
            }
        "#,
        );

        assert!(
            diagnostics.is_empty(),
            "direct private access inside class should produce no diagnostics, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 996ed6311d3afd336c12ae789515265e3528d0714f0a51cf39e0c301cbd87e55

// TSZ_INLINE_TEST_BEGIN 73e483e9a49f56f924406826d95491643a2fd9757aa527eb7dd24e5948184f14 1610 divergent_accessor_public_get_private_set_checks_write_visibility
    #[test]
    fn divergent_accessor_public_get_private_set_checks_write_visibility() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                get value() { return 0; }
                private set value(v) {}
            }
            class Derived extends Base {
                test() {
                    this.value = 1;
                    void this.value;
                }
            }
        "#,
        );

        assert!(
            has_code(&diagnostics, 2341),
            "Expected TS2341 for writing through private setter, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 73e483e9a49f56f924406826d95491643a2fd9757aa527eb7dd24e5948184f14

// TSZ_INLINE_TEST_BEGIN e313bc315c2e2f4ef1c0679246790298fd891873613069156d2df359d2315fcf 1633 divergent_accessor_private_get_protected_set_checks_read_visibility
    #[test]
    fn divergent_accessor_private_get_protected_set_checks_read_visibility() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                private get value() { return 0; }
                protected set value(v) {}
            }
            class Derived extends Base {
                test() {
                    void this.value;
                    this.value = 1;
                }
            }
        "#,
        );

        assert!(
            has_code(&diagnostics, 2341),
            "Expected TS2341 for reading through private getter, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "Read should use the private getter, not protected setter, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END e313bc315c2e2f4ef1c0679246790298fd891873613069156d2df359d2315fcf

// TSZ_INLINE_TEST_BEGIN 392c9f94cf0b69223bb808655c43c28eb97e14eaca1a315ac170935777d93057 1664 nested_class_protected_access_through_correct_instance_is_allowed
    #[test]
    fn nested_class_protected_access_through_correct_instance_is_allowed() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Derived1 instance — should be allowed (no error).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var d1: Derived1 = undefined as any;
                            d1.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should not emit TS2445 for access through correct instance, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should not emit TS2446 for access through correct instance, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 392c9f94cf0b69223bb808655c43c28eb97e14eaca1a315ac170935777d93057

// TSZ_INLINE_TEST_BEGIN bfd0ad6f313e421a190b702cd160e4f844f27bda0e539f1a565257d8da83203b 1696 nested_class_protected_access_through_wrong_instance_emits_ts2446
    #[test]
    fn nested_class_protected_access_through_wrong_instance_emits_ts2446() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Base instance — should emit TS2446 (wrong instance type).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var b: Base = undefined as any;
                            b.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            has_code(&diagnostics, 2446),
            "Expected TS2446 for access through Base instance inside nested class, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "Should emit TS2446 not TS2445 for wrong-instance access, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END bfd0ad6f313e421a190b702cd160e4f844f27bda0e539f1a565257d8da83203b

// TSZ_INLINE_TEST_BEGIN 4f183e2214666f0661e4f3a67f3167b31fd8226193a9577ba775591489acf49a 1728 nested_class_protected_access_through_sibling_emits_ts2446
    #[test]
    fn nested_class_protected_access_through_sibling_emits_ts2446() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Derived2 instance — should emit TS2446 (sibling class, wrong instance).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var d2: Derived2 = undefined as any;
                            d2.x;
                        }
                    }
                }
            }
            class Derived2 extends Base {}
        "#,
        );

        assert!(
            has_code(&diagnostics, 2446),
            "Expected TS2446 for access through sibling instance, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 4f183e2214666f0661e4f3a67f3167b31fd8226193a9577ba775591489acf49a

// TSZ_INLINE_TEST_BEGIN 445c5e2b4545a5d0838cde8ab0fdd3ead9ad0179e78906251a5d1ce87a04ac9d 1757 super_protected_access_via_generic_mixin_constraint_is_allowed
    #[test]
    fn super_protected_access_via_generic_mixin_constraint_is_allowed() {
        let diagnostics = check_diagnostics(
            r#"
            type Constructor<T> = new (...args: any[]) => T;

            class Person {
                protected myProtectedFunction() {}
            }

            function PersonMixin<T extends Constructor<Person>>(Base: T) {
                return class extends Base {
                    myProtectedFunction() {
                        super.myProtectedFunction();
                    }
                };
            }
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should not emit TS2445 for protected super access through a generic mixin constraint, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should not emit TS2446 for protected super access through a generic mixin constraint, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 445c5e2b4545a5d0838cde8ab0fdd3ead9ad0179e78906251a5d1ce87a04ac9d

// TSZ_INLINE_TEST_BEGIN 47187ffc59eedeec4e888d2d8292b8d88602177a5e401b8dbcc6c7c8ff343fee 1787 conformance_mixin_private_and_protected_does_not_emit_extra_super_ts2445
    #[test]
    fn conformance_mixin_private_and_protected_does_not_emit_extra_super_ts2445() {
        let diagnostics = check_diagnostics(
            r#"
            type Constructor<T> = new (...args: any[]) => T;

            class Person {
                protected myProtectedFunction() {}
            }

            function PersonMixin<T extends Constructor<Person>>(Base: T) {
                return class extends Base {
                    myProtectedFunction() {
                        super.myProtectedFunction();
                    }
                };
            }

            const MixedPerson = PersonMixin(class extends Person {});
            new MixedPerson().myProtectedFunction();
        "#,
        );

        assert_eq!(
            diagnostics.iter().filter(|&&code| code == 2445).count(),
            1,
            "Expected only the top-level protected access TS2445 from the conformance file, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 47187ffc59eedeec4e888d2d8292b8d88602177a5e401b8dbcc6c7c8ff343fee

// TSZ_INLINE_TEST_BEGIN c509b95733ae62faeb1edf3eb56858fc400306f99ad46d8fbd4d991279578aed 1817 inherited_static_member_property_access_emits_ts2576
    #[test]
    fn inherited_static_member_property_access_emits_ts2576() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                static count = 1;
                static get size() {
                    return 2;
                }
            }
            class Derived extends Base {}
            const value = new Derived();
            value.count;
            value.size;
        "#,
        );

        assert_eq!(
            diagnostics.iter().filter(|&&code| code == 2576).count(),
            2,
            "Expected TS2576 for inherited static field and accessor property access, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END c509b95733ae62faeb1edf3eb56858fc400306f99ad46d8fbd4d991279578aed

// TSZ_INLINE_TEST_BEGIN cf68fe6a3eefdf8e7e14ea49301b7b70cc0f1166a4756f6bf4704d79cc7aa869 1841 nested_class_protected_access_through_subclass_instance_is_allowed
    #[test]
    fn nested_class_protected_access_through_subclass_instance_is_allowed() {
        // Inside Derived2.method, nested class C accesses protected x through
        // a Derived4 instance (which extends Derived2) — should be allowed.
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived2 extends Base {
                method() {
                    class C {
                        test() {
                            var d4: Derived4 = undefined as any;
                            d4.x;
                        }
                    }
                }
            }
            class Derived4 extends Derived2 {}
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should allow access through subclass instance, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should allow access through subclass instance, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END cf68fe6a3eefdf8e7e14ea49301b7b70cc0f1166a4756f6bf4704d79cc7aa869

// TSZ_INLINE_TEST_BEGIN 424f365938c9391c3b57ae4e424111ac97b95c5b675ce1310f980b3a5fdf83f7 1874 non_nested_class_protected_access_outside_hierarchy_emits_ts2445
    #[test]
    fn non_nested_class_protected_access_outside_hierarchy_emits_ts2445() {
        // Outside any derived class, accessing protected member should emit TS2445.
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            var b: Base = undefined as any;
            b.x;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2445),
            "Expected TS2445 for access outside class hierarchy, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 424f365938c9391c3b57ae4e424111ac97b95c5b675ce1310f980b3a5fdf83f7

// TSZ_INLINE_TEST_BEGIN 4e7b33f7daf38cae765bb00079b9225c89ece10186a1cbbb5f63c4e720a9f4cf 1893 nested_class_declaring_class_allows_access
    #[test]
    fn nested_class_declaring_class_allows_access() {
        // Inside Base.method, nested class A accesses protected x through
        // a Base instance — should be allowed (we're in the declaring class).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
                method() {
                    class A {
                        test() {
                            var b: Base = undefined as any;
                            b.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should allow access from nested class inside declaring class, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should allow access from nested class inside declaring class, got: {diagnostics:?}"
        );
    }
// TSZ_INLINE_TEST_END 4e7b33f7daf38cae765bb00079b9225c89ece10186a1cbbb5f63c4e720a9f4cf

// TSZ_INLINE_TEST_BEGIN e0206c13e0e7c780a14fb33542b6dbd1f89cad4ca165968c29fb8a9de66dc83d 1923 nested_class_full_hierarchy_emits_correct_errors
    #[test]
    fn nested_class_full_hierarchy_emits_correct_errors() {
        // Mirrors the conformance test pattern: Base > Derived1 with nested classes.
        // Inside Base.method > class A: access to b.x is OK (declaring class scope).
        // Inside Derived1.method > class B: b.x should be TS2446 (wrong instance),
        // d1.x should be OK (correct instance).
        let diagnostics = crate::test_utils::check_source_diagnostics(
            r#"
class Base {
    protected x!: string;
    method() {
        class A {
            methoda() {
                var b: Base = undefined as any;
                var d1: Derived1 = undefined as any;
                b.x;
                d1.x;
            }
        }
    }
}

class Derived1 extends Base {
    method1() {
        class B {
            method1b() {
                var b: Base = undefined as any;
                var d1: Derived1 = undefined as any;
                b.x;
                d1.x;
            }
        }
    }
}
"#,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2446),
            "Expected TS2446 for b.x inside nested class in Derived1, got: {codes:?}"
        );
        // Should not have TS2445 for the b.x in Derived1's nested class (it should be TS2446)
        // The only TS2445 errors should be from outside the class hierarchy if any
    }
// TSZ_INLINE_TEST_END e0206c13e0e7c780a14fb33542b6dbd1f89cad4ca165968c29fb8a9de66dc83d
