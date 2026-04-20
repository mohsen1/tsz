//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[path = "checker_state_tests_parts/part_00.rs"] mod checker_state_tests_part_00;
#[path = "checker_state_tests_parts/part_01.rs"] mod checker_state_tests_part_01;
#[path = "checker_state_tests_parts/part_02.rs"] mod checker_state_tests_part_02;
#[path = "checker_state_tests_parts/part_03.rs"] mod checker_state_tests_part_03;
#[path = "checker_state_tests_parts/part_04.rs"] mod checker_state_tests_part_04;
#[path = "checker_state_tests_parts/part_05.rs"] mod checker_state_tests_part_05;
#[path = "checker_state_tests_parts/part_06.rs"] mod checker_state_tests_part_06;
#[path = "checker_state_tests_parts/part_07.rs"] mod checker_state_tests_part_07;
#[path = "checker_state_tests_parts/part_08.rs"] mod checker_state_tests_part_08;
#[path = "checker_state_tests_parts/part_09.rs"] mod checker_state_tests_part_09;
#[path = "checker_state_tests_parts/part_10.rs"] mod checker_state_tests_part_10;
#[path = "checker_state_tests_parts/part_11.rs"] mod checker_state_tests_part_11;
#[path = "checker_state_tests_parts/part_12.rs"] mod checker_state_tests_part_12;
#[path = "checker_state_tests_parts/part_13.rs"] mod checker_state_tests_part_13;
#[path = "checker_state_tests_parts/part_14.rs"] mod checker_state_tests_part_14;
#[path = "checker_state_tests_parts/part_15.rs"] mod checker_state_tests_part_15;
#[path = "checker_state_tests_parts/part_16.rs"] mod checker_state_tests_part_16;
#[path = "checker_state_tests_parts/part_17.rs"] mod checker_state_tests_part_17;
#[path = "checker_state_tests_parts/part_18.rs"] mod checker_state_tests_part_18;
#[path = "checker_state_tests_parts/part_19.rs"] mod checker_state_tests_part_19;
#[path = "checker_state_tests_parts/part_20.rs"] mod checker_state_tests_part_20;
#[path = "checker_state_tests_parts/part_21.rs"] mod checker_state_tests_part_21;
#[path = "checker_state_tests_parts/part_22.rs"] mod checker_state_tests_part_22;
#[path = "checker_state_tests_parts/part_23.rs"] mod checker_state_tests_part_23;
#[path = "checker_state_tests_parts/part_24.rs"] mod checker_state_tests_part_24;
#[path = "checker_state_tests_parts/part_25.rs"] mod checker_state_tests_part_25;
#[path = "checker_state_tests_parts/part_26.rs"] mod checker_state_tests_part_26;
#[path = "checker_state_tests_parts/part_27.rs"] mod checker_state_tests_part_27;
#[path = "checker_state_tests_parts/part_28.rs"] mod checker_state_tests_part_28;
#[path = "checker_state_tests_parts/part_29.rs"] mod checker_state_tests_part_29;
#[path = "checker_state_tests_parts/part_30.rs"] mod checker_state_tests_part_30;
#[path = "checker_state_tests_parts/part_31.rs"] mod checker_state_tests_part_31;
#[path = "checker_state_tests_parts/part_32.rs"] mod checker_state_tests_part_32;
#[path = "checker_state_tests_parts/part_33.rs"] mod checker_state_tests_part_33;
#[path = "checker_state_tests_parts/part_34.rs"] mod checker_state_tests_part_34;
#[path = "checker_state_tests_parts/part_35.rs"] mod checker_state_tests_part_35;
#[path = "checker_state_tests_parts/part_36.rs"] mod checker_state_tests_part_36;
#[path = "checker_state_tests_parts/part_37.rs"] mod checker_state_tests_part_37;
#[path = "checker_state_tests_parts/part_38.rs"] mod checker_state_tests_part_38;
#[path = "checker_state_tests_parts/part_39.rs"] mod checker_state_tests_part_39;
#[path = "checker_state_tests_parts/part_40.rs"] mod checker_state_tests_part_40;
#[path = "checker_state_tests_parts/part_41.rs"] mod checker_state_tests_part_41;
#[path = "checker_state_tests_parts/part_42.rs"] mod checker_state_tests_part_42;
#[path = "checker_state_tests_parts/part_43.rs"] mod checker_state_tests_part_43;
#[path = "checker_state_tests_parts/part_44.rs"] mod checker_state_tests_part_44;
#[path = "checker_state_tests_parts/part_45.rs"] mod checker_state_tests_part_45;
#[path = "checker_state_tests_parts/part_46.rs"] mod checker_state_tests_part_46;
#[path = "checker_state_tests_parts/part_47.rs"] mod checker_state_tests_part_47;
#[path = "checker_state_tests_parts/part_48.rs"] mod checker_state_tests_part_48;
#[path = "checker_state_tests_parts/part_49.rs"] mod checker_state_tests_part_49;
#[path = "checker_state_tests_parts/part_50.rs"] mod checker_state_tests_part_50;
#[path = "checker_state_tests_parts/part_51.rs"] mod checker_state_tests_part_51;
#[path = "checker_state_tests_parts/part_52.rs"] mod checker_state_tests_part_52;
#[path = "checker_state_tests_parts/part_53.rs"] mod checker_state_tests_part_53;
#[path = "checker_state_tests_parts/part_54.rs"] mod checker_state_tests_part_54;
#[path = "checker_state_tests_parts/part_55.rs"] mod checker_state_tests_part_55;
#[path = "checker_state_tests_parts/part_56.rs"] mod checker_state_tests_part_56;
#[path = "checker_state_tests_parts/part_57.rs"] mod checker_state_tests_part_57;
#[path = "checker_state_tests_parts/part_58.rs"] mod checker_state_tests_part_58;
#[path = "checker_state_tests_parts/part_59.rs"] mod checker_state_tests_part_59;
#[path = "checker_state_tests_parts/part_60.rs"] mod checker_state_tests_part_60;
#[path = "checker_state_tests_parts/part_61.rs"] mod checker_state_tests_part_61;
#[path = "checker_state_tests_parts/part_62.rs"] mod checker_state_tests_part_62;
#[path = "checker_state_tests_parts/part_63.rs"] mod checker_state_tests_part_63;
#[path = "checker_state_tests_parts/part_64.rs"] mod checker_state_tests_part_64;
#[path = "checker_state_tests_parts/part_65.rs"] mod checker_state_tests_part_65;
#[path = "checker_state_tests_parts/part_66.rs"] mod checker_state_tests_part_66;
#[path = "checker_state_tests_parts/part_67.rs"] mod checker_state_tests_part_67;
#[path = "checker_state_tests_parts/part_68.rs"] mod checker_state_tests_part_68;
#[path = "checker_state_tests_parts/part_69.rs"] mod checker_state_tests_part_69;
#[path = "checker_state_tests_parts/part_70.rs"] mod checker_state_tests_part_70;
