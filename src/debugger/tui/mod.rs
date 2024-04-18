//! The TUI implementation.

use alloy_primitives::Address;
// use crossterm::{
//     event::{
//         self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
//     },
//     execute,
//     terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
// };
use eyre::Result;
use foundry_common::{compile::ContractSources, evm::Breakpoints};
use foundry_compilers::sourcemap::SourceElement;
use foundry_evm_core::{debug::DebugNodeFlat, utils::PcIcMap};
// use ratatui::{
//     backend::{Backend, CrosstermBackend},
//     Terminal,
// };
use revm::primitives::SpecId;
use std::{
    collections::{BTreeMap, HashMap},
    io,
    ops::ControlFlow,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

mod builder;
pub use builder::DebuggerBuilder;

mod context;
use context::DebuggerContext;

mod draw;
pub use draw::Acc;

// type DebuggerTerminal = Terminal<CrosstermBackend<io::Stdout>>;

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
    breakpoints: Breakpoints,
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
        breakpoints: Breakpoints,
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
            breakpoints,
        }
    }

    /// Starts the debugger TUI. Terminates the current process on failure or user exit.
    pub fn run_exit(mut self) -> ! {
        let code = match self.try_run(&mut vec![]) {
            Ok(ExitReason::CharExit) => 0,
            Err(e) => {
                println!("{e}");
                1
            }
        };
        std::process::exit(code)
    }

    /// Starts the debugger TUI.
    pub fn try_run(&mut self, acc: &mut Vec<Acc>) -> Result<ExitReason> {
        eyre::ensure!(!self.debug_arena.is_empty(), "debug arena is empty");

        // let backend = CrosstermBackend::new(io::stdout());
        // let terminal = Terminal::new(backend)?;
        // TerminalGuard::with(terminal, |terminal| self.try_run_real(terminal, acc))
        self.try_run_real(acc)
    }

    // #[instrument(target = "debugger", name = "run", skip_all, ret)]
    fn try_run_real(
        &mut self,
        // terminal: &mut DebuggerTerminal,
        mut acc: &mut Vec<Acc>,
    ) -> Result<ExitReason> {
        // Create the context.
        let mut cx = DebuggerContext::new(self);

        cx.init();

        // Create an event listener in a different thread.
        // let (tx, rx) = mpsc::channel();
        // thread::Builder::new()
        //     .name("event-listener".into())
        //     .spawn(move || Self::event_listener(tx))
        //     .expect("failed to spawn thread");

        // Draw the initial state.
        cx.draw(acc)?;

        // Start the event loop.
        loop {
            // let ke = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
            // match cx.handle_event(Event::Key(ke)) {
            //     // match cx.handle_event(rx.recv()?) {
            //     ControlFlow::Continue(()) => {}
            //     ControlFlow::Break(reason) => return Ok(reason),
            // }

            cx.handle_event();

            if cx.draw_memory.inner_call_index == cx.debug_arena().len() - 1
                && cx.current_step == cx.opcode_list.len() - 1
            {
                // println!("acc {}", acc.len());
                return Ok(ExitReason::CharExit);
            }

            cx.draw(acc)?;
        }
    }

    // fn event_listener(tx: mpsc::Sender<Event>) {
    //     // This is the recommend tick rate from `ratatui`, based on their examples
    //     let tick_rate = Duration::from_millis(200);

    //     let mut last_tick = Instant::now();
    //     loop {
    //         // Poll events since last tick - if last tick is greater than tick_rate, we
    //         // demand immediate availability of the event. This may affect interactivity,
    //         // but I'm not sure as it is hard to test.
    //         if event::poll(tick_rate.saturating_sub(last_tick.elapsed())).unwrap() {
    //             let event = event::read().unwrap();
    //             if tx.send(event).is_err() {
    //                 return;
    //             }
    //         }

    //         // Force update if time has passed
    //         if last_tick.elapsed() > tick_rate {
    //             last_tick = Instant::now();
    //         }
    //     }
    // }
}

type PanicHandler = Box<dyn Fn(&std::panic::PanicInfo<'_>) + 'static + Sync + Send>;
