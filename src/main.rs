use clap::Parser;
use eyre::Result;
use foundry_cli::{self, handler};
use foundry_evm::inspectors::cheatcodes::{set_execution_context, ForgeContext};

pub mod debugger;
pub mod flamegraph;
pub mod function_call;
pub mod step;
pub mod utils;

pub mod forge;

pub mod opts;
use opts::ForgeFlamegraph;

#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> Result<()> {
    handler::install();
    foundry_cli::utils::load_dotenv();
    foundry_cli::utils::subscriber();
    foundry_cli::utils::enable_paint();

    let cmd = ForgeFlamegraph::parse();
    set_execution_context(ForgeContext::Test);

    let outcome = foundry_cli::utils::block_on(cmd.args.run())?;
    outcome.ensure_ok()
    // } // ForgeSubcommand::Script(cmd) => {
    //     // install the shell before executing the command
    //     foundry_common::shell::set_shell(foundry_common::shell::Shell::from_args(
    //         cmd.opts.silent,
    //         cmd.json,
    //     ))?;
    //     utils::block_on(cmd.run_script())
    // }
    // ForgeSubcommand::Coverage(cmd) => utils::block_on(cmd.run()),
    // ForgeSubcommand::Bind(cmd) => cmd.run(),
    // ForgeSubcommand::Build(cmd) => {
    //     if cmd.is_watch() {
    //         utils::block_on(watch::watch_build(cmd))
    //     } else {
    //         cmd.run().map(|_| ())
    //     }
    // }
    // ForgeSubcommand::Debug(cmd) => utils::block_on(cmd.run()),
    // ForgeSubcommand::VerifyContract(args) => utils::block_on(args.run()),
    // ForgeSubcommand::VerifyCheck(args) => utils::block_on(args.run()),
    // ForgeSubcommand::Cache(cmd) => match cmd.sub {
    //     CacheSubcommands::Clean(cmd) => cmd.run(),
    //     CacheSubcommands::Ls(cmd) => cmd.run(),
    // },
    // ForgeSubcommand::Create(cmd) => utils::block_on(cmd.run()),
    // ForgeSubcommand::Update(cmd) => cmd.run(),
    // ForgeSubcommand::Install(cmd) => cmd.run(),
    // ForgeSubcommand::Remove(cmd) => cmd.run(),
    // ForgeSubcommand::Remappings(cmd) => cmd.run(),
    // ForgeSubcommand::Init(cmd) => cmd.run(),
    // ForgeSubcommand::Completions { shell } => {
    //     // generate(
    //     //     shell,
    //     //     &mut Forge::command(),
    //     //     "forge",
    //     //     &mut std::io::stdout(),
    //     // );
    //     Ok(())
    // }
    // ForgeSubcommand::GenerateFigSpec => {
    //     // clap_complete::generate(
    //     //     clap_complete_fig::Fig,
    //     //     &mut Forge::command(),
    //     //     "forge",
    //     //     &mut std::io::stdout(),
    //     // );
    //     Ok(())
    // }
    // ForgeSubcommand::Clean { root } => {
    //     let config = utils::load_config_with_root(root);
    //     config.project()?.cleanup()?;
    //     Ok(())
    // }
    // ForgeSubcommand::Snapshot(cmd) => {
    //     if cmd.is_watch() {
    //         utils::block_on(watch::watch_snapshot(cmd))
    //     } else {
    //         utils::block_on(cmd.run())
    //     }
    // }
    // ForgeSubcommand::Fmt(cmd) => cmd.run(),
    // ForgeSubcommand::Config(cmd) => cmd.run(),
    // ForgeSubcommand::Flatten(cmd) => cmd.run(),
    // ForgeSubcommand::Inspect(cmd) => cmd.run(),
    // ForgeSubcommand::Tree(cmd) => cmd.run(),
    // ForgeSubcommand::Geiger(cmd) => {
    //     let check = cmd.check;
    //     let n = cmd.run()?;
    //     if check && n > 0 {
    //         std::process::exit(n as i32);
    //     }
    //     Ok(())
    // }
    // ForgeSubcommand::Doc(cmd) => cmd.run(),
    // ForgeSubcommand::Selectors { command } => utils::block_on(command.run()),
    // ForgeSubcommand::Generate(cmd) => match cmd.sub {
    //     GenerateSubcommands::Test(cmd) => cmd.run(),
    // },
    // ForgeSubcommand::VerifyBytecode(cmd) => utils::block_on(cmd.run()),
    // ForgeSubcommand::External(cmd) => {
    //     let bin = format!("forge-{}", cmd[0].to_str().unwrap());
    //     let out = Command::new(bin)
    //         .args(&cmd[1..])
    //         .stdin(Stdio::null())
    //         .stdout(Stdio::inherit())
    //         .spawn();

    //     if let Err(e) = &out {
    //         if e.kind() == std::io::ErrorKind::NotFound {
    //             println!("Command not found: {:?}. Please use forge --help", &cmd[0]);
    //             return Ok(());
    //         }
    //     }
    //     Ok(())
    // }
    // }
}
