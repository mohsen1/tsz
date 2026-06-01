//! Cache-enabled/cache-disabled agreement tests for behavior-changing relation
//! policies.

#[path = "relation_policy_cache_agreement_tests/cache_agreement.rs"]
mod cache_agreement;

#[path = "relation_policy_cache_agreement_tests/callback_kind_partitioning.rs"]
mod callback_kind_partitioning;

#[path = "relation_policy_cache_agreement_tests/any_mode_partitioning.rs"]
mod any_mode_partitioning;

#[path = "relation_policy_cache_agreement_tests/class_context_partitioning.rs"]
mod class_context_partitioning;

#[path = "relation_policy_cache_agreement_tests/default_policy_protocol.rs"]
mod default_policy_protocol;

#[path = "relation_policy_cache_agreement_tests/effective_any_mode_policy_bits.rs"]
mod effective_any_mode_policy_bits;

#[path = "relation_policy_cache_agreement_tests/kind_partitioning.rs"]
mod kind_partitioning;

#[path = "relation_policy_cache_agreement_tests/policy_config.rs"]
mod policy_config;

#[path = "relation_policy_cache_agreement_tests/subtype_policy_projection.rs"]
mod subtype_policy_projection;
