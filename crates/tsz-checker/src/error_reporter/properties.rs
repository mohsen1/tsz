use crate::diagnostics::diagnostic_codes;

use crate::error_reporter::fingerprint_policy::{DiagnosticAnchorKind, DiagnosticRenderRequest};

use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;

use crate::query_boundaries::common as query;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("properties_parts/part1.rs");
include!("properties_parts/part2.rs");

include!("properties/diagnostic_methods_tail.rs");

fn strip_property_namespace_module_extension(module_name: &str) -> &str {
    const EXTS: &[&str] = &[
        ".d.ts", ".d.mts", ".d.cts", ".js", ".ts", ".jsx", ".tsx", ".mjs", ".cjs", ".mts", ".cts",
    ];
    for ext in EXTS {
        if let Some(stripped) = module_name.strip_suffix(ext) {
            return stripped;
        }
    }
    module_name
}

/// Match tsc's `^(?:EventTarget|Node|(?:HTML[a-zA-Z]*)?Element)$` regex used by
/// `containerSeemsToBeEmptyDomElement` to detect DOM element-like type names.
fn is_dom_element_like_name(name: &str) -> bool {
    if name == "EventTarget" || name == "Node" || name == "Element" {
        return true;
    }
    if let Some(prefix) = name.strip_suffix("Element")
        && let Some(rest) = prefix.strip_prefix("HTML")
        && rest.chars().all(|c| c.is_ascii_alphabetic())
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::diagnostic_codes;

    fn diagnostics_for_source(source: &str) -> Vec<u32> {
        crate::test_utils::check_source_codes(source)
    }

    /// TS2339 must be suppressed for property access on type parameters with
    /// circular `typeof` constraints (`T extends typeof a` where `a: T`).
    /// This applies to both direct parameters and destructured bindings.
    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_direct_param() {
        // Direct parameter: `a: T` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>(a: T) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for direct param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_destructured_param() {
        // Destructured parameter: `{a}: {a:T}` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>({a}: {a:T}) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for destructured param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_suppressed_for_circular_typeof_constraint_array_destructured_param() {
        // Array destructured parameter: `[a]: T[]` where `T extends typeof a`
        let diags = diagnostics_for_source("function f<T extends typeof a>([a]: T[]) { a.b; }");
        assert!(
            !diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be suppressed for array-destructured param with circular typeof constraint, got: {diags:?}"
        );
    }

    #[test]
    fn ts2339_not_suppressed_for_unconstrained_type_param() {
        // Unconstrained type parameter should still emit TS2339
        let diags = diagnostics_for_source("function f<T>(a: T) { a.b; }");
        assert!(
            diags.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
            "TS2339 should be emitted for unconstrained type param, got: {diags:?}"
        );
    }
}
