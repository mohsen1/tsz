//! Excess-property construction boundary scans.
//!
//! State checking owns excess-property policy and source traversal. Solver
//! object shape construction, target union construction, and interning belong
//! behind query boundaries.

use std::fs;
use std::path::{Path, PathBuf};

const EXCESS_PROPERTY_TAIL: &str = "src/state/state_checking/property/excess_property_tail.rs";
const EXCESS_PROPERTY_CORE: &str = "src/state/state_checking/property.rs";
const STATE_CHECKING_BOUNDARY: &str = "src/query_boundaries/state/checking.rs";
const INTERSECTION_DISPLAY_BOUNDARY: &str = "src/query_boundaries/intersection_display.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn excess_property_tail_routes_object_construction_through_boundaries() {
    let source =
        fs::read_to_string(checker_path(EXCESS_PROPERTY_TAIL)).expect("failed to read source");
    let core_source =
        fs::read_to_string(checker_path(EXCESS_PROPERTY_CORE)).expect("failed to read source");

    for forbidden in [
        "tsz_solver::ObjectShape {",
        "ObjectShape::default()",
        "tsz_solver::PropertyInfo {",
        "PropertyInfo::new(",
        ".factory().object(",
        ".factory.object(",
    ] {
        assert!(
            !source.contains(forbidden),
            "excess_property_tail must route object construction through query \
             boundaries, found `{forbidden}`"
        );
    }
    for forbidden in [
        "tsz_solver::utils::union_or_single(",
        "tsz_solver::utils::union_or_single_literal_reduce(",
    ] {
        assert!(
            !source.contains(forbidden) && !core_source.contains(forbidden),
            "excess-property checking must route target union construction \
             through query boundaries, found `{forbidden}`"
        );
    }

    assert!(
        source.contains("intersection_display::collected_properties_object_shape(")
            && source.contains("query::excess_property_any_object_type_from_names("),
        "excess_property_tail should call the intersection/display and state-checking boundaries"
    );
    assert!(
        core_source.contains("query::excess_property_target_union("),
        "property.rs should route nested excess-property target unions through the state-checking boundary"
    );
    assert!(
        source
            .matches("query::excess_property_display_target_union(")
            .count()
            >= 2,
        "excess_property_tail should route annotation and discriminant display unions through the state-checking boundary"
    );
    assert!(
        source.contains(
            "return query::excess_property_target_union(self.ctx.types, matching_members);"
        ),
        "excess_property_tail should route discriminant-narrowed target unions through the state-checking boundary"
    );
}

#[test]
fn excess_property_boundaries_own_construction_helpers() {
    let state_boundary = fs::read_to_string(checker_path(STATE_CHECKING_BOUNDARY))
        .expect("failed to read state checking boundary");
    assert!(
        state_boundary.contains("fn excess_property_any_object_type_from_names("),
        "state checking boundary must own synthetic excess-property object construction"
    );
    assert!(
        state_boundary.contains("fn excess_property_target_union(")
            && state_boundary.contains("fn excess_property_display_target_union("),
        "state checking boundary must own excess-property target union construction"
    );
    assert!(
        state_boundary.contains("tsz_solver::utils::union_or_single(")
            && state_boundary.contains("tsz_solver::utils::union_or_single_literal_reduce("),
        "state checking boundary must own excess-property target union constructors"
    );

    let intersection_boundary = fs::read_to_string(checker_path(INTERSECTION_DISPLAY_BOUNDARY))
        .expect("failed to read intersection display boundary");
    assert!(
        intersection_boundary.contains("fn collected_properties_object_shape"),
        "intersection display boundary must own collected-property object shape construction"
    );
}
