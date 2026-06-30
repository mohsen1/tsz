//! Named checker-side lazy-resolution guard states.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApplicationResolutionEntryState {
    AlreadyResolved,
    AlreadyVisiting,
    FuelExhausted,
    DepthExceeded,
    Entered { outermost: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApplicationResolutionWorkState {
    Continue,
    LocalFuelExhausted,
    GlobalFuelExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefsResolutionWorkState {
    Continue,
    RefsFuelExhausted,
    GlobalFuelExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvalEnvEntryState {
    Entered { depth: u32 },
    DepthExceeded,
}

pub(crate) const fn application_resolution_entry_state(
    already_resolved: bool,
    inserted_active_visit: bool,
    fuel: u32,
    max_fuel: u32,
    depth: u32,
    max_depth: u32,
) -> ApplicationResolutionEntryState {
    if already_resolved {
        return ApplicationResolutionEntryState::AlreadyResolved;
    }
    if !inserted_active_visit {
        return ApplicationResolutionEntryState::AlreadyVisiting;
    }
    if fuel >= max_fuel {
        return ApplicationResolutionEntryState::FuelExhausted;
    }
    if depth >= max_depth {
        return ApplicationResolutionEntryState::DepthExceeded;
    }
    ApplicationResolutionEntryState::Entered {
        outermost: depth == 0,
    }
}

pub(crate) const fn application_resolution_local_fuel_state(
    fuel: u32,
    max_fuel: u32,
) -> ApplicationResolutionWorkState {
    if fuel >= max_fuel {
        ApplicationResolutionWorkState::LocalFuelExhausted
    } else {
        ApplicationResolutionWorkState::Continue
    }
}

pub(crate) const fn application_resolution_post_consume_state(
    global_fuel_exhausted: bool,
) -> ApplicationResolutionWorkState {
    if global_fuel_exhausted {
        ApplicationResolutionWorkState::GlobalFuelExhausted
    } else {
        ApplicationResolutionWorkState::Continue
    }
}

pub(crate) const fn refs_resolution_work_state(
    refs_fuel_exhausted: bool,
    global_fuel_exhausted: bool,
) -> RefsResolutionWorkState {
    if refs_fuel_exhausted {
        RefsResolutionWorkState::RefsFuelExhausted
    } else if global_fuel_exhausted {
        RefsResolutionWorkState::GlobalFuelExhausted
    } else {
        RefsResolutionWorkState::Continue
    }
}

pub(crate) const fn eval_env_entry_state(depth: u32, max_depth: u32) -> EvalEnvEntryState {
    if depth >= max_depth {
        EvalEnvEntryState::DepthExceeded
    } else {
        EvalEnvEntryState::Entered { depth: depth + 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplicationResolutionEntryState, ApplicationResolutionWorkState, EvalEnvEntryState,
        RefsResolutionWorkState, application_resolution_entry_state,
        application_resolution_local_fuel_state, application_resolution_post_consume_state,
        eval_env_entry_state, refs_resolution_work_state,
    };

    #[test]
    fn application_entry_state_names_every_top_level_cutoff() {
        assert_eq!(
            application_resolution_entry_state(true, true, 0, 5, 0, 1),
            ApplicationResolutionEntryState::AlreadyResolved
        );
        assert_eq!(
            application_resolution_entry_state(false, false, 0, 5, 0, 1),
            ApplicationResolutionEntryState::AlreadyVisiting
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 5, 5, 0, 1),
            ApplicationResolutionEntryState::FuelExhausted
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 1, 1),
            ApplicationResolutionEntryState::DepthExceeded
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 0, 1),
            ApplicationResolutionEntryState::Entered { outermost: true }
        );
        assert_eq!(
            application_resolution_entry_state(false, true, 0, 5, 1, 2),
            ApplicationResolutionEntryState::Entered { outermost: false }
        );
    }

    #[test]
    fn application_work_state_names_local_and_global_fuel_cutoffs() {
        assert_eq!(
            application_resolution_local_fuel_state(4, 5),
            ApplicationResolutionWorkState::Continue
        );
        assert_eq!(
            application_resolution_local_fuel_state(5, 5),
            ApplicationResolutionWorkState::LocalFuelExhausted
        );
        assert_eq!(
            application_resolution_post_consume_state(false),
            ApplicationResolutionWorkState::Continue
        );
        assert_eq!(
            application_resolution_post_consume_state(true),
            ApplicationResolutionWorkState::GlobalFuelExhausted
        );
    }

    #[test]
    fn refs_work_state_names_prewalk_cutoffs() {
        assert_eq!(
            refs_resolution_work_state(false, false),
            RefsResolutionWorkState::Continue
        );
        assert_eq!(
            refs_resolution_work_state(true, false),
            RefsResolutionWorkState::RefsFuelExhausted
        );
        assert_eq!(
            refs_resolution_work_state(false, true),
            RefsResolutionWorkState::GlobalFuelExhausted
        );
    }

    #[test]
    fn eval_env_entry_state_names_depth_cutoff() {
        assert_eq!(
            eval_env_entry_state(4, 5),
            EvalEnvEntryState::Entered { depth: 5 }
        );
        assert_eq!(eval_env_entry_state(5, 5), EvalEnvEntryState::DepthExceeded);
    }
}
