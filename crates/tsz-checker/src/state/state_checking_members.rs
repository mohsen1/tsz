//! Declaration and statement checking (member submodules).

#[path = "../state_checking_members/ambient_signature_checks.rs"]
mod ambient_signature_checks;
#[path = "../state_checking_members/member_access.rs"]
mod member_access;
#[path = "../state_checking_members/member_declaration_checks.rs"]
mod member_declaration_checks;
#[path = "../state_checking_members/statement_callback_bridge.rs"]
mod statement_callback_bridge;
#[path = "../state_checking_members/statement_checks.rs"]
mod statement_checks;
