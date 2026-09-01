//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/operations.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bdfd97b6de6bd3d105e851c92179c9df0d3a205c7a0a543b50a14c3ee8b135ac 1686 find_references_terminates_on_circular_class_heritage
    /// Regression for issue #8527: two-class heritage cycle.
    #[test]
    fn find_references_terminates_on_circular_class_heritage() {
        let source = r#"class C extends D {
    prop0: string;
    prop1: string;
}

class D extends C {
    prop0: string;
    prop1: string;
}

var d: D;
d.prop1;
"#;
        let mut project = Project::new();
        project.set_file("/file1.ts".to_string(), source.to_string());

        let position = position_of(source, "prop1");
        let result = project.find_references("/file1.ts", position);
        assert!(
            result.is_some(),
            "find_references must terminate on circular heritage and return references"
        );
    }
// TSZ_INLINE_TEST_END bdfd97b6de6bd3d105e851c92179c9df0d3a205c7a0a543b50a14c3ee8b135ac

// TSZ_INLINE_TEST_BEGIN 9a426861d674681fdcdfe880966b9fcd08e31e771f7cc214ef38abed3b4fa36f 1714 find_references_terminates_on_circular_interface_heritage
    /// Regression for issue #8527: interface-cycle variant. We only require
    /// termination — interface symbol classification is handled elsewhere.
    #[test]
    fn find_references_terminates_on_circular_interface_heritage() {
        let source = r#"interface A extends B {
    prop: string;
}

interface B extends A {
    prop: string;
}

let a: A;
a.prop;
"#;
        let mut project = Project::new();
        project.set_file("/file1.ts".to_string(), source.to_string());

        let position = position_of(source, "prop");
        let _ = project.find_references("/file1.ts", position);
    }
// TSZ_INLINE_TEST_END 9a426861d674681fdcdfe880966b9fcd08e31e771f7cc214ef38abed3b4fa36f

// TSZ_INLINE_TEST_BEGIN fbc54ffb578be82318151181e24e6f62ad4269d4085b04057481ce27dd45bcc5 1736 find_references_terminates_on_three_way_heritage_cycle
    /// Three-way heritage cycle. Forces the `visited` set to span the whole
    /// walk rather than just the previous frame.
    #[test]
    fn find_references_terminates_on_three_way_heritage_cycle() {
        let source = r#"class A extends B { member: string; }
class B extends C { member: string; }
class C extends A { member: string; }
var c: C;
c.member;
"#;
        let mut project = Project::new();
        project.set_file("/file1.ts".to_string(), source.to_string());

        let position = position_of(source, "member");
        let result = project.find_references("/file1.ts", position);
        assert!(
            result.is_some(),
            "find_references must terminate on a heritage cycle of length 3"
        );
    }
// TSZ_INLINE_TEST_END fbc54ffb578be82318151181e24e6f62ad4269d4085b04057481ce27dd45bcc5

// TSZ_INLINE_TEST_BEGIN e715dd3a18236e4a5a079f8147c8876dc41031409cb7a04095547059e37029a8 1757 find_references_visits_transitive_base_class_members
    /// Non-cyclic heritage chain: confirms the BFS walker still reaches
    /// transitive ancestors. `Child extends Mid extends Base`.
    #[test]
    fn find_references_visits_transitive_base_class_members() {
        let source = r#"class Base { member: string; }
class Mid extends Base {}
class Child extends Mid { member: string; }
var c: Child;
c.member;
"#;
        let mut project = Project::new();
        project.set_file("/file1.ts".to_string(), source.to_string());

        // The second `member` occurrence is the `Child` declaration —
        // the same anchor the original Position::new(2, 26) cursor used.
        let position = position_of_nth(source, "member", 1);
        let refs = project
            .find_references("/file1.ts", position)
            .expect("must find references for a normal heritage chain");
        assert!(
            !refs.is_empty(),
            "expected at least the declaration to be returned"
        );
    }
// TSZ_INLINE_TEST_END e715dd3a18236e4a5a079f8147c8876dc41031409cb7a04095547059e37029a8

// TSZ_INLINE_TEST_BEGIN 3dafcf67a155d0c5d7cb54a4ba51f04244d8736fbd135cd347047facaa845a67 1780 find_file_references_visits_import_and_export_module_specifiers
    #[test]
    fn find_file_references_visits_import_and_export_module_specifiers() {
        let dep = "export const value = 1;";
        let source = r#"import { value } from "./dep";
export { value as renamed } from "./dep";
export * as namespace from "./dep";
const local = value;
"#;
        let mut project = Project::new();
        project.set_file("/dep.ts".to_string(), dep.to_string());
        project.set_file("/index.ts".to_string(), source.to_string());

        let locations = project.find_file_references("/dep.ts");
        let refs: Vec<_> = locations
            .iter()
            .filter(|location| location.file_path == "/index.ts")
            .map(|location| location.range.start)
            .collect();

        assert_eq!(
            refs,
            vec![
                position_of_nth(source, "./dep", 0),
                position_of_nth(source, "./dep", 1),
                position_of_nth(source, "./dep", 2),
            ]
        );
    }
// TSZ_INLINE_TEST_END 3dafcf67a155d0c5d7cb54a4ba51f04244d8736fbd135cd347047facaa845a67
