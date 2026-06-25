#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::Result;
use clap::Parser;
use tsz_cli::try_tsz::{TryTszArgs, run};

fn main() -> Result<()> {
    let args = TryTszArgs::parse();
    let cwd = std::env::current_dir()?;
    let exit_code = run(args, &cwd)?;
    // #14345 Stage-2 PROBE dump (temporary; gated by `TSZ_STAGE2_PROBE`).
    tsz_solver::stage2_probe_dump();
    std::process::exit(exit_code);
}
