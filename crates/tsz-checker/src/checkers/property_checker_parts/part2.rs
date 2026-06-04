#[cfg(test)]
mod tests {
    fn check_diagnostics(source: &str) -> Vec<u32> {
        crate::test_utils::check_source_codes(source)
    }

    fn has_code(diagnostics: &[u32], code: u32) -> bool {
        diagnostics.contains(&code)
    }

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

    // =========================================================================
    // TS2446: Protected access through wrong instance type in nested classes
    // =========================================================================

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
}
