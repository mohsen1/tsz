pub(crate) const fn is_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::is_compound_assignment_operator(operator_token)
}

pub(crate) const fn is_logical_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::is_logical_compound_assignment_operator(operator_token)
}

pub(crate) const fn is_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::is_assignment_operator(operator_token)
}

pub(crate) const fn map_compound_assignment_to_binary(operator_token: u16) -> Option<&'static str> {
    tsz_solver::map_compound_assignment_to_binary(operator_token)
}
