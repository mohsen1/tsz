use crate::transforms::async_es5_ir::opcodes;
use crate::transforms::ir::IRNode;

/// Sentinel values reserved for a single async try-region. The patch sweep
/// replaces each sentinel with its corresponding real label.
#[derive(Clone, Copy)]
pub(super) struct TryRegionPlaceholders {
    pub(super) catch_slot: u32,
    pub(super) finally_slot: u32,
    pub(super) end_slot: u32,
    pub(super) exit_break: u32,
}

/// Real labels for a planned async try-region, plus the shared exit target that
/// try-body and catch-body breaks land on.
pub(super) struct TryRegionResolution {
    pub(super) placeholders: TryRegionPlaceholders,
    pub(super) catch_label: Option<u32>,
    pub(super) finally_label: Option<u32>,
    pub(super) end_label: u32,
    pub(super) exit_target: u32,
}

pub(super) fn patch_try_region_placeholders(node: &mut IRNode, resolution: &TryRegionResolution) {
    let TryRegionResolution {
        placeholders,
        catch_label,
        finally_label,
        end_label,
        exit_target,
    } = resolution;
    let patch_slot = |slot: &mut u32| {
        if *slot == placeholders.catch_slot {
            *slot = catch_label.unwrap_or(0);
        } else if *slot == placeholders.finally_slot {
            *slot = finally_label.unwrap_or(0);
        } else if *slot == placeholders.end_slot {
            *slot = *end_label;
        }
    };
    match node {
        IRNode::GeneratorTryPush {
            catch_label,
            finally_label,
            end_label,
            ..
        } => {
            patch_slot(catch_label);
            patch_slot(finally_label);
            patch_slot(end_label);
        }
        IRNode::GeneratorTryPushFinally {
            finally_label,
            end_label,
            ..
        } => {
            patch_slot(finally_label);
            patch_slot(end_label);
        }
        IRNode::GeneratorTryPushCatch {
            catch_label,
            end_label,
            ..
        } => {
            patch_slot(catch_label);
            patch_slot(end_label);
        }
        IRNode::GeneratorOp {
            opcode,
            value: Some(boxed_value),
            ..
        } if *opcode == opcodes::BREAK => {
            if let IRNode::NumericLiteral(s) = boxed_value.as_ref()
                && s.as_ref().parse::<u32>().ok() == Some(placeholders.exit_break)
            {
                **boxed_value = IRNode::NumericLiteral(exit_target.to_string().into());
            }
        }
        IRNode::ReturnStatement(Some(inner)) => {
            patch_try_region_placeholders(inner, resolution);
        }
        _ => {}
    }
}
