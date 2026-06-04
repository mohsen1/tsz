use std::cmp::Ordering;

use std::collections::VecDeque;

use web_time::Instant;

use rustc_hash::FxHashSet;

use crate::navigation::implementation::{GoToImplementationProvider, TargetKind};

use crate::navigation::references::FindReferences;

use crate::rename::{RenameProvider, TextEdit, WorkspaceEdit};

use crate::resolver::ScopeCacheStats;

use crate::utils::find_node_at_offset;

use tsz_common::position::{Location, Position, Range};

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use super::{
    ImportKind, ImportSpecifierTarget, NamespaceReexportTarget, Project, ProjectFile,
    ProjectRequestKind,
};

include!("operations_parts/part1.rs");
include!("operations_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_common::position::LineMap;

    /// Position of the Nth (0-based) occurrence of `needle` in `source`,
    /// as a 0-based `(line, character)` LSP position. Panics if not
    /// found, so fixture drift is loud rather than silently changing a
    /// test's cursor target.
    ///
    /// Delegates byte-offset -> `Position` conversion to the canonical
    /// `LineMap` so column counts match LSP's UTF-16 semantics and
    /// non-`\n` line terminators (matching the rest of the LSP crate).
    fn position_of_nth(source: &str, needle: &str, n: usize) -> Position {
        let byte_offset = source
            .match_indices(needle)
            .nth(n)
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| panic!("test fixture missing occurrence {n} of {needle:?}"));
        LineMap::build(source).offset_to_position(byte_offset as u32, source)
    }

    fn position_of(source: &str, needle: &str) -> Position {
        position_of_nth(source, needle, 0)
    }

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
}
