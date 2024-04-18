use crate::step::VecStep;
use alloy_primitives::Address;
use eyre::Result;
use foundry_common::compile::ContractSources;
use foundry_evm_core::{debug::DebugNodeFlat, utils::PcIcMap};
use revm::primitives::SpecId;
use std::collections::{BTreeMap, HashMap};

mod builder;
pub use builder::DebuggerBuilder;

mod context;
use context::DebuggerContext;

mod draw;

/// Debugger exit reason.
#[derive(Debug)]
pub enum ExitReason {
    /// Exit using 'q'.
    CharExit,
}

/// The TUI debugger.
pub struct Debugger {
    debug_arena: Vec<DebugNodeFlat>,
    identified_contracts: HashMap<Address, String>,
    /// Source map of contract sources
    contracts_sources: ContractSources,
    /// A mapping of source -> (PC -> IC map for deploy code, PC -> IC map for runtime code)
    pc_ic_maps: BTreeMap<String, (PcIcMap, PcIcMap)>,
}

impl Debugger {
    /// Creates a new debugger builder.
    #[inline]
    pub fn builder() -> DebuggerBuilder {
        DebuggerBuilder::new()
    }

    /// Creates a new debugger.
    pub fn new(
        debug_arena: Vec<DebugNodeFlat>,
        identified_contracts: HashMap<Address, String>,
        contracts_sources: ContractSources,
    ) -> Self {
        let pc_ic_maps = contracts_sources
            .entries()
            .filter_map(|(contract_name, _, contract)| {
                Some((
                    contract_name.to_owned(),
                    (
                        PcIcMap::new(SpecId::LATEST, contract.bytecode.bytes()?),
                        PcIcMap::new(SpecId::LATEST, contract.deployed_bytecode.bytes()?),
                    ),
                ))
            })
            .collect();

        Self {
            debug_arena,
            identified_contracts,
            contracts_sources,
            pc_ic_maps,
        }
    }

    /// Starts the debugger TUI. Terminates the current process on failure or user exit.
    // pub fn run_exit(mut self) -> ! {
    //     let code = match self.try_run(&mut vec![]) {
    //         Ok(ExitReason::CharExit) => 0,
    //         Err(e) => {
    //             println!("{e}");
    //             1
    //         }
    //     };
    //     std::process::exit(code)
    // }

    /// Starts the debugger TUI.
    pub fn try_run(&mut self, acc: &mut VecStep) -> Result<ExitReason> {
        eyre::ensure!(!self.debug_arena.is_empty(), "debug arena is empty");
        self.try_run_real(acc)
    }

    fn try_run_real(&mut self, acc: &mut VecStep) -> Result<ExitReason> {
        // Create the context.
        let mut cx = DebuggerContext::new(self);

        cx.init();
        cx.draw(acc)?;

        // Start the event loop.
        loop {
            cx.handle_event();

            if cx.draw_memory.inner_call_index == cx.debug_arena().len() - 1
                && cx.current_step == cx.opcode_list.len() - 1
            {
                return Ok(ExitReason::CharExit);
            }

            cx.draw(acc)?;
        }
    }
}
