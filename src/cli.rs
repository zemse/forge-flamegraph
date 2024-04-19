// use super::watch::WatchArgs;

use super::{forge::install, forge::test::ProjectPathsAwareFilter};
use crate::{debugger::Debugger, flamegraph::Flamegraph, step::VecStep};
use clap::Parser;
use eyre::Result;
use forge::{
    inspectors::CheatsConfig,
    multi_runner::matches_contract,
    result::{SuiteResult, TestOutcome, TestStatus},
    traces::{identifier::SignaturesIdentifier, CallTraceDecoderBuilder, TraceKind},
    MultiContractRunner, MultiContractRunnerBuilder, TestFilter, TestOptions, TestOptionsBuilder,
};
use foundry_cli::{
    opts::CoreBuildArgs,
    utils::{self, LoadConfig},
};
use foundry_common::{
    compile::{ContractSources, ProjectCompiler},
    evm::EvmArgs,
    shell,
};
use foundry_compilers::{artifacts::output_selection::OutputSelection, utils::source_files_iter};
use foundry_config::{
    figment,
    figment::{
        value::{Dict, Map},
        Metadata, Profile, Provider,
    },
    get_available_profiles, Config,
};
use foundry_evm::traces::identifier::TraceIdentifiers;
use regex::Regex;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::mpsc::channel,
};
use tracing::trace;
// use watchexec::config::{InitConfig, RuntimeConfig};
use yansi::Paint;

pub use crate::forge::test::FilterArgs;
use forge::traces::render_trace_arena;

foundry_config::merge_impl_figment_convert!(FlamegraphArgs, opts, evm_opts);

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_GIT_SHA"),
    " ",
    env!("VERGEN_BUILD_TIMESTAMP"),
    ")"
);

/// CLI arguments for `forge test`.
#[derive(Clone, Debug, Parser)]
#[command(
    version = VERSION_MESSAGE,
    next_display_order = None,
)]
pub struct FlamegraphArgs {
    #[arg(long, short = 't', value_name = "TEST_FUNCTION")]
    test_function: Option<Regex>,

    #[arg(long, short, help_heading = "Internal functions")]
    internal: bool,

    #[arg(long, short, help_heading = "Output format")]
    json: bool,

    #[arg(long, short, help_heading = "Print entire trace")]
    steps: bool,

    #[command(flatten)]
    evm_opts: EvmArgs,

    #[command(flatten)]
    opts: CoreBuildArgs,
}

impl FlamegraphArgs {
    /// Returns the flattened [`CoreBuildArgs`].
    pub fn build_args(&self) -> &CoreBuildArgs {
        &self.opts
    }

    pub async fn run(self) -> Result<TestOutcome> {
        trace!(target: "forge::test", "executing test command");
        // shell::set_shell(shell::Shell::from_args(self.opts.silent, self.json))?;
        shell::set_shell(shell::Shell::from_args(false, false))?; // TODO:
        self.execute_tests().await
    }

    /// Returns sources which include any tests to be executed.
    /// If no filters are provided, sources are filtered by existence of test/invariant methods in
    /// them, If filters are provided, sources are additionaly filtered by them.
    pub fn get_sources_to_compile(
        &self,
        config: &Config,
        filter: &ProjectPathsAwareFilter,
    ) -> Result<BTreeSet<PathBuf>> {
        let mut project = config.create_project(true, true)?;
        project.solc_config.settings.output_selection =
            OutputSelection::common_output_selection(["abi".to_string()]);
        let output = project.compile()?;

        if output.has_compiler_errors() {
            println!("{}", output);
            eyre::bail!("Compilation failed");
        }

        // ABIs of all sources
        let abis = output
            .into_artifacts()
            .filter_map(|(id, artifact)| artifact.abi.map(|abi| (id, abi)))
            .collect::<BTreeMap<_, _>>();

        // Filter sources by their abis and contract names.
        let mut test_sources = abis
            .iter()
            .filter(|(id, abi)| matches_contract(id, abi, filter))
            .map(|(id, _)| id.source.clone())
            .collect::<BTreeSet<_>>();

        if test_sources.is_empty() {
            if filter.is_empty() {
                println!(
                    "No tests found in project! \
                        Forge looks for functions that starts with `test`."
                );
            } else {
                println!("No tests match the provided pattern:");
                print!("{filter}");

                // Try to suggest a test when there's no match
                if let Some(test_pattern) = &filter.args().test_pattern {
                    let test_name = test_pattern.as_str();
                    let candidates = abis
                        .into_iter()
                        .filter(|(id, _)| {
                            filter.matches_path(&id.source) && filter.matches_contract(&id.name)
                        })
                        .flat_map(|(_, abi)| abi.functions.into_keys())
                        .collect::<Vec<_>>();
                    if let Some(suggestion) = utils::did_you_mean(test_name, candidates).pop() {
                        println!("\nDid you mean `{suggestion}`?");
                    }
                }
            }

            eyre::bail!("No tests to run");
        }

        // Always recompile all sources to ensure that `getCode` cheatcode can use any artifact.
        test_sources.extend(source_files_iter(project.paths.sources));

        Ok(test_sources)
    }

    /// Executes all the tests in the project.
    ///
    /// This will trigger the build process first. On success all test contracts that match the
    /// configured filter will be executed
    ///
    /// Returns the test results for all matching tests.
    pub async fn execute_tests(self) -> Result<TestOutcome> {
        // Merge all configs
        let (mut config, mut evm_opts) = self.load_config_and_evm_opts_emit_warnings()?;

        // Explicitly enable isolation for gas reports for more correct gas accounting
        // if self.gas_report {
        //     evm_opts.isolate = true;
        // } else {
        //     // Do not collect gas report traces if gas report is not enabled.
        //     config.fuzz.gas_report_samples = 0;
        //     config.invariant.gas_report_samples = 0;
        // }

        // Set up the project.
        let mut project = config.project()?;

        // Install missing dependencies.
        if install::install_missing_dependencies(&mut config, self.build_args().silent)
            && config.auto_detect_remappings
        {
            // need to re-configure here to also catch additional remappings
            config = self.load_config();
            project = config.project()?;
        }

        let mut filter = self.filter(&config);
        trace!(target: "forge::test", ?filter, "using filter");

        let sources_to_compile = self.get_sources_to_compile(&config, &filter)?;

        let compiler = ProjectCompiler::new()
            // .quiet_if(self.json || self.opts.silent)
            .files(sources_to_compile);

        let output = compiler.compile(&project)?;

        // Create test options from general project settings and compiler output.
        let project_root = &project.paths.root;
        let toml = config.get_config_path();
        let profiles = get_available_profiles(toml)?;

        let test_options: TestOptions = TestOptionsBuilder::default()
            .fuzz(config.clone().fuzz)
            .invariant(config.invariant)
            .profiles(profiles)
            .build(&output, project_root)?;

        // Determine print verbosity and executor verbosity
        let verbosity = 3;
        evm_opts.verbosity = verbosity;
        // let verbosity = evm_opts.verbosity;
        // if self.gas_report && evm_opts.verbosity < 3 {
        //     evm_opts.verbosity = 3;
        // }

        let env = evm_opts.evm_env().await?;

        // Prepare the test builder
        let should_debug = self.internal;

        // Clone the output only if we actually need it later for the debugger.
        let output_clone = should_debug.then(|| output.clone());

        let artifact_ids = output.artifact_ids().map(|(id, _)| id).collect();

        let runner = MultiContractRunnerBuilder::default()
            .set_debug(should_debug)
            .initial_balance(evm_opts.initial_balance)
            .evm_spec(config.evm_spec_id())
            .sender(evm_opts.sender)
            .with_fork(evm_opts.get_fork(&config, env.clone()))
            .with_cheats_config(CheatsConfig::new(
                &config,
                evm_opts.clone(),
                Some(artifact_ids),
                None,
                None, // populated separately for each test contract
            ))
            .with_test_options(test_options)
            .enable_isolation(evm_opts.isolate)
            .build(project_root, output, env, evm_opts)?;

        if let Some(debug_test_pattern) = &self.test_function {
            let test_pattern = &mut filter.args_mut().test_pattern;
            if test_pattern.is_some() {
                eyre::bail!(
                    "Cannot specify both --debug and --match-test. \
                     Use --match-contract and --match-path to further limit the search instead."
                );
            }
            *test_pattern = Some(debug_test_pattern.clone());
        }

        let outcome = self.run_tests(runner, config, verbosity, &filter).await?;

        // flamegraph inputs: debug, sources

        let Some((suite_result, test_result)) = outcome
            .results
            .iter()
            .find(|(_, r)| !r.test_results.is_empty())
            .map(|(_, r)| (r, r.test_results.values().next().unwrap()))
        else {
            return Err(eyre::eyre!("no tests were executed"));
        };

        let keys: Vec<String> = suite_result.test_results.keys().cloned().collect();
        assert_eq!(keys.len(), 1, "number of tests ran must be 1");
        let test_name = &keys[0];

        if should_debug {
            // Get first non-empty suite result. We will have only one such entry

            let sources = ContractSources::from_project_output(
                output_clone.as_ref().unwrap(),
                project.root(),
                &suite_result.libraries,
            )?;

            let mut builder = Debugger::builder()
                .debug_arenas(test_result.debug.as_slice())
                .sources(sources)
                .breakpoints(test_result.breakpoints.clone());
            // identified contracts are set here
            if let Some(decoder) = &outcome.decoder {
                builder = builder.decoder(decoder);
            }
            let mut debugger = builder.build();

            let mut steps = VecStep::default();
            if self.steps {
                println!("steps {:#?}", steps);
            }

            // debugger.run_silent()?;
            debugger.try_run(&mut steps)?;

            let top_call = steps.parse();

            if self.json {
                println!(
                    "\n\nflamegraph data: {}\n\n",
                    serde_json::to_string_pretty(&top_call).unwrap()
                );
            } else {
                println!("\n\nflamegraph data: {:?}\n\n", top_call);
            }
        } else {
            let arena = test_result
                .traces
                .iter()
                .find_map(|(kind, arena)| {
                    if *kind == TraceKind::Execution {
                        Some(arena)
                    } else {
                        None
                    }
                })
                .unwrap();

            let nodes = arena.nodes();
            let decoder = outcome.decoder.as_ref().unwrap();
            let flamegraph = Flamegraph::from_call_trace(nodes, decoder).await;
            flamegraph.generate(format!("flamegraph-{}.svg", test_name));
        }

        Ok(outcome)
    }

    /// Run all tests that matches the filter predicate from a test runner
    pub async fn run_tests(
        &self,
        mut runner: MultiContractRunner,
        config: Config,
        verbosity: u8,
        filter: &ProjectPathsAwareFilter,
    ) -> eyre::Result<TestOutcome> {
        trace!(target: "forge::test", "running all tests");

        let num_filtered = runner.matching_test_functions(filter).count();
        if num_filtered != 1 {
            eyre::bail!(
                "{num_filtered} tests matched your criteria, but exactly 1 test must match in order to run the debugger.\n\n\
                 Use --match-contract and --match-path to further limit the search.\n\
                 Filter used:\n{filter}"
            );
        }

        // Set up trace identifiers.
        let known_contracts = runner.known_contracts.clone();
        let mut identifier = TraceIdentifiers::new().with_local(&known_contracts);

        // Run tests.
        let (tx, rx) = channel::<(String, SuiteResult)>();

        let handle = tokio::task::spawn_blocking({
            let filter = filter.clone();
            move || runner.test(&filter, tx)
        });

        // let mut gas_report = self
        //     .gas_report
        //     .then(|| GasReport::new(config.gas_reports, config.gas_reports_ignore));

        // Build the trace decoder.
        let mut builder = CallTraceDecoderBuilder::new()
            .with_known_contracts(&known_contracts)
            .with_verbosity(verbosity);
        // Signatures are of no value for gas reports.
        // if !self.gas_report {
        builder = builder.with_signature_identifier(SignaturesIdentifier::new(
            Config::foundry_cache_dir(),
            config.offline,
        )?);
        // }
        let mut decoder = builder.build();

        // We identify addresses if we're going to print *any* trace or gas report.
        let identify_addresses = true; //  verbosity >= 3 || self.gas_report || self.debug.is_some();

        let mut outcome = TestOutcome::empty(true);

        let mut any_test_failed = false;
        for (contract_name, suite_result) in rx {
            let tests = &suite_result.test_results;

            // Print suite header.
            println!();
            for warning in suite_result.warnings.iter() {
                eprintln!("{} {warning}", Paint::yellow("Warning:").bold());
            }
            if !tests.is_empty() {
                let len = tests.len();
                let tests = if len > 1 { "tests" } else { "test" };
                println!("Ran {len} {tests} for {contract_name}");
            }

            // Process individual test results, printing logs and traces when necessary.
            for (name, result) in tests {
                shell::println(result.short_result(name))?;

                // We only display logs at level 2 and above
                // if verbosity >= 2 {
                //     // We only decode logs from Hardhat and DS-style console events
                //     let console_logs = decode_console_logs(&result.logs);
                //     if !console_logs.is_empty() {
                //         println!("Logs:");
                //         for log in console_logs {
                //             println!("  {log}");
                //         }
                //         println!();
                //     }
                // }

                // We shouldn't break out of the outer loop directly here so that we finish
                // processing the remaining tests and print the suite summary.
                any_test_failed |= result.status == TestStatus::Failure;

                if result.traces.is_empty() {
                    continue;
                }

                // Clear the addresses and labels from previous runs.
                decoder.clear_addresses();
                decoder.labels.extend(
                    result
                        .labeled_addresses
                        .iter()
                        .map(|(k, v)| (*k, v.clone())),
                );

                // Identify addresses and decode traces.
                let mut decoded_traces = Vec::with_capacity(result.traces.len());
                for (kind, arena) in &result.traces {
                    if identify_addresses {
                        decoder.identify(arena, &mut identifier);
                    }

                    // verbosity:
                    // - 0..3: nothing
                    // - 3: only display traces for failed tests
                    // - 4: also display the setup trace for failed tests
                    // - 5..: display all traces for all tests
                    let should_include = match kind {
                        TraceKind::Execution => {
                            (verbosity == 3 && result.status.is_failure()) || verbosity >= 4
                        }
                        TraceKind::Setup => {
                            (verbosity == 4 && result.status.is_failure()) || verbosity >= 5
                        }
                        TraceKind::Deployment => false,
                    };

                    if should_include {
                        decoded_traces.push(render_trace_arena(arena, &decoder).await?);
                    }
                }

                // if !decoded_traces.is_empty() {
                //     shell::println("Traces:")?;
                //     for trace in &decoded_traces {
                //         shell::println(trace)?;
                //     }
                // }
            }

            // Print suite summary.
            // shell::println(suite_result.summary())?;

            // Add the suite result to the outcome.
            outcome.results.insert(contract_name, suite_result);
        }

        trace!(target: "forge::test", len=outcome.results.len(), %any_test_failed, "done with results");

        outcome.decoder = Some(decoder);

        // Reattach the task.
        if let Err(e) = handle.await {
            match e.try_into_panic() {
                Ok(payload) => std::panic::resume_unwind(payload),
                Err(e) => return Err(e.into()),
            }
        }

        Ok(outcome)
    }

    // /// Returns the flattened [`FilterArgs`] arguments merged with [`Config`].
    pub fn filter(&self, config: &Config) -> ProjectPathsAwareFilter {
        FilterArgs::default().clone().merge_with_config(config)
    }

    /// Returns whether `BuildArgs` was configured with `--watch`
    pub fn is_watch(&self) -> bool {
        false
    }

    // Returns the [`watchexec::InitConfig`] and [`watchexec::RuntimeConfig`] necessary to
    // bootstrap a new [`watchexe::Watchexec`] loop.
    // pub(crate) fn watchexec_config(&self) -> Result<(InitConfig, RuntimeConfig)> {
    //     self.watch.watchexec_config(|| {
    //         let config = Config::from(self);
    //         vec![config.src, config.test]
    //     })
    // }
}

impl Provider for FlamegraphArgs {
    fn metadata(&self) -> Metadata {
        Metadata::named("Core Build Args Provider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        Ok(Map::from([(Config::selected_profile(), Dict::default())]))
    }
}
