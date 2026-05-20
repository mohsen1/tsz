//! Explicit emit plan boundary.
//!
//! This is the behavior-preserving skeleton for the direct-to-target emit plan.
//! Today it wraps the existing `TransformContext`; follow-up PRs should move
//! helper, hoist, export, temp, and region scheduling into the typed plan fields
//! instead of discovering those facts while printing.

use crate::context::target_facts::EmitTargetFacts;
use crate::context::transform::TransformContext;
use crate::emitter::PrinterOptions;
use crate::transforms::helpers::HelpersNeeded;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;

/// File-level plan consumed by the printer.
#[derive(Clone)]
pub struct EmitPlan {
    pub target_facts: EmitTargetFacts,
    pub module: ModuleKind,
    /// Existing directive map. This remains the compatibility bridge while
    /// scheduling facts migrate into the typed plan fields below.
    pub transforms: TransformContext,
    pub helpers: HelpersNeeded,
    pub temps: EmitTempPlan,
    pub hoists: EmitHoistPlan,
    pub exports: EmitExportPlan,
    pub regions: Vec<EmitRegionPlan>,
}

impl EmitPlan {
    #[must_use]
    pub fn empty(options: &PrinterOptions) -> Self {
        Self::from_transforms(options, TransformContext::new())
    }

    #[must_use]
    pub fn from_transforms(options: &PrinterOptions, transforms: TransformContext) -> Self {
        let helpers = transforms.helpers().clone();
        Self {
            target_facts: EmitTargetFacts::from_target(options.target),
            module: options.module,
            transforms,
            helpers,
            temps: EmitTempPlan::default(),
            hoists: EmitHoistPlan::default(),
            exports: EmitExportPlan::default(),
            regions: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_legacy_target_lane(&self) -> bool {
        self.target_facts.legacy_below_ts6_floor
    }
}

/// Future home for generated-name reservations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmitTempPlan {
    pub reserved_names: Vec<String>,
}

/// Future home for prologue and declaration hoisting schedules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmitHoistPlan {
    pub statement_count: usize,
}

/// Future home for module/export scheduling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmitExportPlan {
    pub binding_count: usize,
}

/// Region-level transform plan entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitRegionPlan {
    pub root: NodeIndex,
    pub kind: EmitRegionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitRegionKind {
    Disposable,
    ModuleWrapper,
    ClassLike,
    FunctionBody,
}

/// Builder used by emit lowering to make the plan construction explicit.
pub struct EmitPlanBuilder {
    options: PrinterOptions,
    transforms: TransformContext,
}

impl EmitPlanBuilder {
    #[must_use]
    pub fn new(options: &PrinterOptions) -> Self {
        Self {
            options: options.clone(),
            transforms: TransformContext::new(),
        }
    }

    #[must_use]
    pub fn with_transforms(mut self, transforms: TransformContext) -> Self {
        self.transforms = transforms;
        self
    }

    #[must_use]
    pub fn build(self) -> EmitPlan {
        EmitPlan::from_transforms(&self.options, self.transforms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitter::ScriptTarget;

    #[test]
    fn plan_carries_target_facts_from_options() {
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };

        let plan = EmitPlan::empty(&options);

        assert_eq!(plan.target_facts.target, ScriptTarget::ES5);
        assert!(plan.is_legacy_target_lane());
    }

    #[test]
    fn plan_snapshots_lowering_helpers() {
        let options = PrinterOptions::default();
        let mut transforms = TransformContext::new();
        transforms.helpers_mut().awaiter = true;

        let plan = EmitPlan::from_transforms(&options, transforms);

        assert!(plan.helpers.awaiter);
    }
}
