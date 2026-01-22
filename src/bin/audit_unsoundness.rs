//! CLI tool to audit TypeScript unsoundness catalog implementation
//!
//! Usage:
//!   cargo run --bin audit_unsoundness
//!   cargo run --bin audit_unsoundness -- --summary
//!   cargo run --bin audit_unsoundness -- --matrix
//!   cargo run --bin audit_unsoundness -- --missing
//!   cargo run --bin audit_unsoundness -- --phase 1

use std::env;
use std::process;
use wasm::solver::unsoundness_audit::{
    ImplementationPhase, ImplementationStatus, UnsoundnessAudit,
};

fn print_usage() {
    println!("Usage: audit_unsoundness [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --summary     Print summary report (default)");
    println!("  --matrix      Print full matrix table");
    println!("  --missing     Print only missing rules");
    println!("  --phase <N>   Print rules for phase N (1-4)");
    println!("  --status <S>  Print rules by status (full, partial, missing, blocked)");
    println!("  --help        Print this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let audit = UnsoundnessAudit::new();

    if args.len() < 2 {
        // Default: print summary
        println!("{}", audit.summary_report());
        return;
    }

    match args[1].as_str() {
        "--summary" => {
            println!("{}", audit.summary_report());
        }
        "--matrix" => {
            println!("{}", audit.matrix_table());
        }
        "--missing" => {
            println!("# Missing Rules (Not Implemented)\n");
            let missing = audit.missing_rules();
            if missing.is_empty() {
                println!("✅ All rules are implemented!");
            } else {
                println!("Total missing: {}\n", missing.len());
                for rule in missing {
                    println!("  {} Rule #{}: {} (Phase {:?})", rule.status.emoji(), rule.rule_number, rule.name, rule.phase);
                    println!("    Notes: {}", rule.notes);
                    println!();
                }
            }
        }
        "--phase" => {
            if args.len() < 3 {
                eprintln!("Error: --phase requires a phase number (1-4)");
                process::exit(1);
            }
            let phase_num = match args[2].parse::<u8>() {
                Ok(n) if (1..=4).contains(&n) => n,
                _ => {
                    eprintln!("Error: Phase must be between 1 and 4");
                    process::exit(1);
                }
            };

            let phase = match phase_num {
                1 => ImplementationPhase::Phase1,
                2 => ImplementationPhase::Phase2,
                3 => ImplementationPhase::Phase3,
                4 => ImplementationPhase::Phase4,
                _ => unreachable!(),
            };

            println!("# Phase {} Rules: {}\n", phase_num, phase);
            let rules = audit.rules_by_phase(phase);
            println!("Total: {} rules\n", rules.len());
            println!("Completion: {:.1}%\n", audit.completion_by_phase(phase) * 100.0);

            for rule in rules {
                println!("  {} Rule #{}: {}", rule.status.emoji(), rule.rule_number, rule.name);
                println!("    Status: {:?}", rule.status);
                println!("    Coverage: {:.0}%", rule.test_coverage * 100.0);
                if !rule.notes.is_empty() {
                    println!("    Notes: {}", rule.notes);
                }
                println!();
            }
        }
        "--status" => {
            if args.len() < 3 {
                eprintln!("Error: --status requires a status (full, partial, missing, blocked)");
                process::exit(1);
            }

            let status = match args[2].as_str() {
                "full" => ImplementationStatus::FullyImplemented,
                "partial" => ImplementationStatus::PartiallyImplemented,
                "missing" => ImplementationStatus::NotImplemented,
                "blocked" => ImplementationStatus::Blocked,
                _ => {
                    eprintln!("Error: Status must be one of: full, partial, missing, blocked");
                    process::exit(1);
                }
            };

            let status_name = match args[2].as_str() {
                "full" => "Fully Implemented",
                "partial" => "Partially Implemented",
                "missing" => "Not Implemented",
                "blocked" => "Blocked",
                _ => unreachable!(),
            };

            println!("# {} Rules\n", status_name);
            let rules = audit.rules_by_status(status);
            println!("Total: {} rules\n", rules.len());

            if rules.is_empty() {
                println!("No rules with this status.\n");
            } else {
                for rule in rules {
                    println!("  {} Rule #{}: {} (Phase {:?})", rule.status.emoji(), rule.rule_number, rule.name, rule.phase);
                    if !rule.notes.is_empty() {
                        println!("    Notes: {}", rule.notes);
                    }
                    println!();
                }
            }
        }
        "--help" | "-h" => {
            print_usage();
        }
        _ => {
            eprintln!("Unknown option: {}", args[1]);
            println!();
            print_usage();
            process::exit(1);
        }
    }
}
