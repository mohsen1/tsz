pub(crate) const fn is_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_compound_assignment_operator(operator_token)
}

pub(crate) const fn is_logical_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_logical_compound_assignment_operator(
        operator_token,
    )
}

pub(crate) const fn is_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::operations::compound_assignment::is_assignment_operator(operator_token)
}

pub(crate) const fn map_compound_assignment_to_binary(operator_token: u16) -> Option<&'static str> {
    tsz_solver::operations::compound_assignment::map_compound_assignment_to_binary(operator_token)
}
