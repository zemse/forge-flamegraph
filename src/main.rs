use clap::Parser;
use cli::FlamegraphArgs;
use eyre::Result;
use foundry_cli::{self, handler};
use foundry_evm::inspectors::cheatcodes::{set_execution_context, ForgeContext};

pub mod backends;
pub mod cli;
pub mod flamegraph;

pub mod forge;

fn main() -> Result<()> {
    handler::install();
    foundry_cli::utils::load_dotenv();
    foundry_cli::utils::subscriber();
    foundry_cli::utils::enable_paint();

    let flamegraph = FlamegraphArgs::parse();
    set_execution_context(ForgeContext::Test);

    let outcome = foundry_cli::utils::block_on(flamegraph.run())?;
    outcome.ensure_ok()
}
