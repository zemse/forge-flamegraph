use crate::cmd::flamegraph;
use clap::{Parser, Subcommand, ValueHint};
// use forge_script::ScriptArgs;
// use forge_verify::{bytecode::VerifyBytecodeArgs, VerifyArgs, VerifyCheckArgs};

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_GIT_SHA"),
    " ",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")"
);

/// Build, test, fuzz, debug and deploy Solidity contracts.
#[derive(Parser)]
#[command(
    name = "forge",
    version = VERSION_MESSAGE,
    after_help = "Find more information in the book: http://book.getfoundry.sh/reference/forge/forge.html",
    next_display_order = None,
)]
pub struct Forge {
    #[command(subcommand)]
    pub cmd: ForgeSubcommand,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ForgeSubcommand {
    Flamegraph(flamegraph::FlamegraphArgs),
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Forge::command().debug_assert();
    }
}
