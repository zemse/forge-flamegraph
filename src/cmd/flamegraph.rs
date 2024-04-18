// use super::watch::WatchArgs;

use super::{install, test::ProjectPathsAwareFilter};
use crate::debugger::{Acc, Debugger};
use alloy_primitives::{Address, U256};
use clap::Parser;
use eyre::Result;
use forge::{
    decode::decode_console_logs,
    gas_report::GasReport,
    inspectors::CheatsConfig,
    multi_runner::matches_contract,
    result::{SuiteResult, TestOutcome, TestStatus},
    revm::primitives::SpecId,
    traces::{identifier::SignaturesIdentifier, CallTraceDecoderBuilder, TraceKind},
    utils::PcIcMap,
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
use foundry_compilers::{
    artifacts::output_selection::OutputSelection,
    sourcemap::{Jump, SourceElement},
    utils::source_files_iter,
};
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
use serde::Serialize;
use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    path::PathBuf,
    rc::{Rc, Weak},
    sync::mpsc::channel,
    time::Instant,
};
use tracing::trace;
// use watchexec::config::{InitConfig, RuntimeConfig};
use yansi::Paint;

pub use super::test::FilterArgs;
use forge::traces::render_trace_arena;

foundry_config::merge_impl_figment_convert!(FlamegraphArgs, opts, evm_opts);

/// CLI arguments for `forge test`.
#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Flamegraph options")]
pub struct FlamegraphArgs {
    // #[arg(long = "match-test", visible_alias = "mt", value_name = "REGEX")]
    // pub test_pattern: Option<Regex>,
    #[arg(long, value_name = "TEST_FUNCTION")]
    debug: Option<Regex>,

    // /// Print a gas report.
    // #[arg(long, env = "FORGE_GAS_REPORT")]
    // gas_report: bool,

    // /// Exit with code 0 even if a test fails.
    // #[arg(long, env = "FORGE_ALLOW_FAILURE")]
    // allow_failure: bool,

    // /// Output test results in JSON format.
    #[arg(long, short, help_heading = "Output format")]
    json: bool,

    #[arg(long, short, help_heading = "Print entire trace")]
    steps: bool,

    // /// Stop running tests after the first failure.
    // #[arg(long)]
    // pub fail_fast: bool,

    // /// The Etherscan (or equivalent) API key.
    // #[arg(long, env = "ETHERSCAN_API_KEY", value_name = "KEY")]
    // etherscan_api_key: Option<String>,

    // /// List tests instead of running them.
    // #[arg(long, short, help_heading = "Display options")]
    // list: bool,

    // /// Set seed used to generate randomness during your fuzz runs.
    // #[arg(long)]
    // pub fuzz_seed: Option<U256>,

    // #[arg(long, env = "FOUNDRY_FUZZ_RUNS", value_name = "RUNS")]
    // pub fuzz_runs: Option<u64>,

    // /// File to rerun fuzz failures from.
    // #[arg(long)]
    // pub fuzz_input_file: Option<String>,

    // #[command(flatten)]
    // filter: FilterArgs,
    #[command(flatten)]
    evm_opts: EvmArgs,

    #[command(flatten)]
    opts: CoreBuildArgs,
    // #[command(flatten)]
    // pub watch: WatchArgs,

    // /// Print test summary table.
    // #[arg(long, help_heading = "Display options")]
    // pub summary: bool,

    // /// Print detailed test summary table.
    // #[arg(long, help_heading = "Display options", requires = "summary")]
    // pub detailed: bool,
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
        let verbosity = evm_opts.verbosity;
        // if self.gas_report && evm_opts.verbosity < 3 {
        //     evm_opts.verbosity = 3;
        // }

        let env = evm_opts.evm_env().await?;

        // Prepare the test builder
        let should_debug = self.debug.is_some();

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

        if let Some(debug_test_pattern) = &self.debug {
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

        if should_debug {
            // Get first non-empty suite result. We will have only one such entry
            let Some((suite_result, test_result)) = outcome
                .results
                .iter()
                .find(|(_, r)| !r.test_results.is_empty())
                .map(|(_, r)| (r, r.test_results.values().next().unwrap()))
            else {
                return Err(eyre::eyre!("no tests were executed"));
            };

            // println!("debug {:?}", test_result.debug);

            let sources = ContractSources::from_project_output(
                output_clone.as_ref().unwrap(),
                project.root(),
                &suite_result.libraries,
            )?;
            // println!("sources {:?}", sources.ids_by_name);
            // println!("test_result.breakpoints {:?}", test_result.breakpoints);

            // println!("pc_ic_maps {:?}", pc_ic_maps);
            // Run the debugger.
            let mut builder = Debugger::builder()
                .debug_arenas(test_result.debug.as_slice())
                .sources(sources)
                .breakpoints(test_result.breakpoints.clone());
            // identified contracts are set here
            if let Some(decoder) = &outcome.decoder {
                builder = builder.decoder(decoder);
            }
            let mut debugger = builder.build();

            let mut acc: Vec<Acc> = vec![];
            // debugger.run_silent()?;
            debugger.try_run(&mut acc)?;

            // println!("acc {:#?}", acc);
            let top_call = process_acc(&acc);

            if self.steps {
                println!("acc_arr {:#?}", acc);
            }

            if self.json {
                println!(
                    "\n\nflamegraph data: {}\n\n",
                    serde_json::to_string_pretty(&top_call).unwrap()
                );
            } else {
                println!("\n\nflamegraph data: {:?}\n\n", top_call);
            }
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
        // if self.list {
        //     return list(runner, filter, self.json);
        // }

        trace!(target: "forge::test", "running all tests");

        let num_filtered = runner.matching_test_functions(filter).count();
        if num_filtered != 1 {
            eyre::bail!(
                "{num_filtered} tests matched your criteria, but exactly 1 test must match in order to run the debugger.\n\n\
                 Use --match-contract and --match-path to further limit the search.\n\
                 Filter used:\n{filter}"
            );
        }

        // if self.json {
        //     let results = runner.test_collect(filter);
        //     println!("{}", serde_json::to_string(&results)?);
        //     return Ok(TestOutcome::new(results, self.allow_failure));
        // }

        // Set up trace identifiers.
        let known_contracts = runner.known_contracts.clone();
        let remote_chain_id = runner.evm_opts.get_remote_chain_id();
        let mut identifier = TraceIdentifiers::new().with_local(&known_contracts);

        // Run tests.
        let (tx, rx) = channel::<(String, SuiteResult)>();
        let timer = Instant::now();
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
                if verbosity >= 2 {
                    // We only decode logs from Hardhat and DS-style console events
                    let console_logs = decode_console_logs(&result.logs);
                    if !console_logs.is_empty() {
                        println!("Logs:");
                        for log in console_logs {
                            println!("  {log}");
                        }
                        println!();
                    }
                }

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

                if !decoded_traces.is_empty() {
                    shell::println("Traces:")?;
                    for trace in &decoded_traces {
                        shell::println(trace)?;
                    }
                }

                // if let Some(gas_report) = &mut gas_report {
                //     gas_report
                //         .analyze(result.traces.iter().map(|(_, arena)| arena), &decoder)
                //         .await;

                //     for trace in result.gas_report_traces.iter() {
                //         decoder.clear_addresses();

                //         // Re-execute setup and deployment traces to collect identities created in
                //         // setUp and constructor.
                //         for (kind, arena) in &result.traces {
                //             if !matches!(kind, TraceKind::Execution) {
                //                 decoder.identify(arena, &mut identifier);
                //             }
                //         }

                //         for arena in trace {
                //             decoder.identify(arena, &mut identifier);
                //             gas_report.analyze([arena], &decoder).await;
                //         }
                //     }
                // }
            }

            // Print suite summary.
            shell::println(suite_result.summary())?;

            // Add the suite result to the outcome.
            outcome.results.insert(contract_name, suite_result);

            // Stop processing the remaining suites if any test failed and `fail_fast` is set.
            // if self.fail_fast && any_test_failed {
            //     break;
            // }
        }
        let duration = timer.elapsed();

        trace!(target: "forge::test", len=outcome.results.len(), %any_test_failed, "done with results");

        outcome.decoder = Some(decoder);

        // if let Some(gas_report) = gas_report {
        //     let finalized = gas_report.finalize();
        //     shell::println(&finalized)?;
        //     outcome.gas_report = Some(finalized);
        // }

        // if !outcome.results.is_empty() {
        //     shell::println(outcome.summary(duration))?;

        //     if self.summary {
        //         let mut summary_table = TestSummaryReporter::new(self.detailed);
        //         shell::println("\n\nTest Summary:")?;
        //         summary_table.print_summary(&outcome);
        //     }
        // }

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
        let mut dict = Dict::default();

        let mut fuzz_dict = Dict::default();
        // if let Some(fuzz_seed) = self.fuzz_seed {
        //     fuzz_dict.insert("seed".to_string(), fuzz_seed.to_string().into());
        // }
        // if let Some(fuzz_runs) = self.fuzz_runs {
        //     fuzz_dict.insert("runs".to_string(), fuzz_runs.into());
        // }
        // if let Some(fuzz_input_file) = self.fuzz_input_file.clone() {
        //     fuzz_dict.insert("failure_persist_file".to_string(), fuzz_input_file.into());
        // }
        dict.insert("fuzz".to_string(), fuzz_dict.into());

        // if let Some(etherscan_api_key) = self
        //     .etherscan_api_key
        //     .as_ref()
        //     .filter(|s| !s.trim().is_empty())
        // {
        //     dict.insert(
        //         "etherscan_api_key".to_string(),
        //         etherscan_api_key.to_string().into(),
        //     );
        // }

        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}

/// Lists all matching tests
fn list(
    runner: MultiContractRunner,
    filter: &ProjectPathsAwareFilter,
    json: bool,
) -> Result<TestOutcome> {
    let results = runner.list(filter);

    if json {
        println!("{}", serde_json::to_string(&results)?);
    } else {
        for (file, contracts) in results.iter() {
            println!("{file}");
            for (contract, tests) in contracts.iter() {
                println!("  {contract}");
                println!("    {}\n", tests.join("\n    "));
            }
        }
    }
    Ok(TestOutcome::empty(false))
}

#[derive(Clone, Serialize)]
struct FunctionCall {
    title: String,
    name: String,
    gas_start: u64,
    gas_end: Option<u64>,
    color: String,
    #[serde(rename = "children")]
    calls: Vec<Rc<RefCell<FunctionCall>>>,
    #[serde(skip)]
    parent: Option<Weak<RefCell<FunctionCall>>>,
}

use std::fmt::Debug;
impl Debug for FunctionCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _self = self.to_owned();
        let _ref = Rc::new(RefCell::new(_self));
        writeln!(f).unwrap();
        print_call(&_ref, 0, f);
        Ok(())
    }
}

pub fn print_call(call: &Rc<RefCell<FunctionCall>>, depth: usize, f: &mut std::fmt::Formatter<'_>) {
    let call = call.borrow_mut();
    writeln!(
        f,
        "{:indent$}{} (gas: {})",
        "",
        call.title,
        (call.gas_end.unwrap_or(0) as i64) - (call.gas_start as i64),
        // call.gas_end.unwrap_or(0),
        // call.gas_start,
        indent = depth * 2
    )
    .unwrap();
    for c in &call.calls {
        print_call(c, depth + 1, f);
    }
}

pub fn process_acc(acc_arr: &Vec<Acc>) -> Rc<RefCell<FunctionCall>> {
    assert_eq!(
        acc_arr[0].current_step.total_gas_used, 0,
        "this should be the start"
    );
    let contract_name =
        get_contract_name_from_acc(&acc_arr[0]).expect("source code should be of contract");
    let top_call = Rc::new(RefCell::new(FunctionCall {
        title: format!("{contract_name}.fallback"),
        name: contract_name,
        gas_start: 0,
        gas_end: None,
        color: String::new(),
        calls: vec![],
        parent: None,
    }));
    let mut ptr = Rc::clone(&top_call);

    for (i, acc) in acc_arr.iter().enumerate() {
        if i == 0 {
            // we have handled the first one already
            continue;
        }

        if acc.source_element.jump == Jump::In {
            let acc_next = &acc_arr[i + 1];
            let function_name = get_name(acc); //.expect("source code should be of function");
            let function_name_next = get_function_name_from_acc(acc_next); //.expect("source code should be of function");
            if function_name.is_none() && function_name_next.is_none() {
                continue;
            }
            // if function_name != function_name_next {
            //     // panic!("function name mismatch {} {}", function_name, function_name_next);
            //     continue;
            // }
            let function_name = function_name.or(function_name_next).unwrap();

            let ptr_weak = Rc::downgrade(&ptr);
            let new_call = Rc::new(RefCell::new(FunctionCall {
                title: format!("{function_name} internal jump"),
                name: function_name,

                gas_start: acc.current_step.total_gas_used,
                gas_end: None,
                color: String::new(),
                calls: vec![],
                parent: Some(ptr_weak),
            }));
            ptr.borrow_mut().calls.push(Rc::clone(&new_call));
            ptr = new_call;
        }

        // CALL or STATICCALL
        if acc.current_step.instruction == 0xF1 || acc.current_step.instruction == 0xFA {
            let ptr_weak = Rc::downgrade(&ptr);
            let acc_next = &acc_arr[i + 1];
            if let Some(contract_name) = get_contract_name_from_acc(acc_next) {
                let new_call = Rc::new(RefCell::new(FunctionCall {
                    title: format!("{contract_name}.fallback"),
                    name: contract_name,
                    gas_start: acc.current_step.total_gas_used,
                    gas_end: None,
                    color: String::new(),
                    calls: vec![],
                    parent: Some(ptr_weak),
                }));
                ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                ptr = new_call;
            } else {
                let function_name =
                    get_next(&acc.source_code, "", vec!['(']).expect("vm call native code");
                let new_call = Rc::new(RefCell::new(FunctionCall {
                    title: format!("{function_name} nativecode"),
                    name: function_name,
                    gas_start: acc.current_step.total_gas_used,
                    gas_end: Some(acc_next.current_step.total_gas_used),
                    color: String::new(),
                    calls: vec![],
                    parent: Some(ptr_weak),
                }));
                ptr.borrow_mut().calls.push(Rc::clone(&new_call));
            };
        }

        // internal function call ends
        if acc.source_element.jump == Jump::Out {
            let name = get_name(acc).unwrap();
            let acc_next = &acc_arr[i + 1];
            // if !acc_next.source_code.contains(&name) {
            //     continue;
            // }

            let ptr_weak = Rc::downgrade(&ptr);
            let return_dummy_call = Rc::new(RefCell::new(FunctionCall {
                title: format!(
                    "return {name} pc: {}, total_gas_used: {}",
                    acc.current_step.pc, acc.current_step.total_gas_used
                ),
                name: "return".to_string(),
                gas_start: 0,
                gas_end: Some(0),
                color: String::new(),
                calls: vec![],
                parent: Some(ptr_weak),
            }));
            ptr.borrow_mut().calls.push(return_dummy_call);
            let parent_ptr = if let Some(ptr) = ptr.borrow_mut().parent.as_ref() {
                Weak::clone(ptr)
            } else {
                println!("no parent found for {}", acc.source_code);
                break;
            };

            ptr.borrow_mut().gas_end = Some(acc.current_step.total_gas_used);

            ptr = parent_ptr.upgrade().unwrap();
        }

        // call ends
        if acc.current_step.instruction == 0xF3
            || acc.current_step.instruction == 0xFD
            || acc.current_step.instruction == 0x00
        {
            let parent_ptr = Weak::clone(ptr.borrow_mut().parent.as_ref().unwrap());

            let acc_next = &acc_arr[i + 1];
            ptr.borrow_mut().gas_end = Some(acc_next.current_step.total_gas_used);

            ptr = parent_ptr.upgrade().unwrap();
        }
    }

    top_call
}

pub fn get_contract_name_from_acc(acc: &Acc) -> Option<String> {
    // get_name(acc)
    get_next(&acc.source_code, "contract ", vec![' ', '{'])
        .or_else(|| get_next(&acc.source_code, "abstract contract ", vec![' ', '{']))
}

pub fn get_function_name_from_acc(acc: &Acc) -> Option<String> {
    // get_name(acc)
    get_next(&acc.source_code, "function ", vec![' ', '('])
}

pub fn get_name(acc: &Acc) -> Option<String> {
    get_next(&acc.source_code, "contract ", vec![' ', '{'])
        .or_else(|| get_next(&acc.source_code, "abstract contract ", vec![' ', '{']))
        .or_else(|| get_next(&acc.source_code, "function ", vec![' ', '(']))
        .or_else(|| get_after_dot(&acc.source_code, vec!['(']))
        .or_else(|| get_next(&acc.source_code, "", vec!['(']))
}

// replace these by regular expressions
pub fn get_next(str: &str, prepend: &str, breakers: Vec<char>) -> Option<String> {
    if str.starts_with(prepend) {
        let start = prepend.len();
        let mut end = start;
        loop {
            let nth = &str.chars().nth(end);
            if nth.is_none() {
                return None;
            }
            if breakers.contains(&nth.unwrap()) {
                break;
            }
            end += 1;
        }
        Some(str[start..end].to_owned())
    } else {
        None
    }
}

// replace these by regular expressions
pub fn get_after_dot(str: &str, breakers: Vec<char>) -> Option<String> {
    // cases
    // uint256(0x0000000000000000000000000000000000000000000000000000000000000000).toField()
    let mut start = 0;
    let mut dot_found = false;
    let mut end = start;
    loop {
        let nth = &str.chars().nth(end);
        if nth.is_none() {
            return None;
        }
        if nth.unwrap() == '.' {
            start = end + 1;
            dot_found = true;
        }
        if dot_found && breakers.contains(&nth.unwrap()) {
            break;
        }
        end += 1;
    }
    Some(str[start..end].to_owned())
}
