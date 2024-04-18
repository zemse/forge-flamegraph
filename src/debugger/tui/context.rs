use super::{Debugger, ExitReason};
use alloy_primitives::Address;
use foundry_evm_core::debug::{DebugNodeFlat, DebugStep};
use revm_inspectors::tracing::types::CallKind;
use std::ops::ControlFlow;

/// This is currently used to remember last scroll position so screen doesn't wiggle as much.
#[derive(Default)]
pub(crate) struct DrawMemory {
    pub(crate) inner_call_index: usize,
}

pub(crate) struct DebuggerContext<'a> {
    pub(crate) debugger: &'a mut Debugger,

    /// Buffer for keys prior to execution, i.e. '10' + 'k' => move up 10 operations.
    pub(crate) key_buffer: String,
    /// Current step in the debug steps.
    pub(crate) current_step: usize,
    pub(crate) draw_memory: DrawMemory,
    pub(crate) opcode_list: Vec<String>,
    pub(crate) last_index: usize,
}

impl<'a> DebuggerContext<'a> {
    pub(crate) fn new(debugger: &'a mut Debugger) -> Self {
        DebuggerContext {
            debugger,

            key_buffer: String::with_capacity(64),
            current_step: 0,
            draw_memory: DrawMemory::default(),
            opcode_list: Vec::new(),
            last_index: 0,
        }
    }

    pub(crate) fn init(&mut self) {
        self.gen_opcode_list();
    }

    pub(crate) fn debug_arena(&self) -> &[DebugNodeFlat] {
        &self.debugger.debug_arena
    }

    pub(crate) fn debug_call(&self) -> &DebugNodeFlat {
        &self.debug_arena()[self.draw_memory.inner_call_index]
    }

    /// Returns the current call address.
    pub(crate) fn address(&self) -> &Address {
        &self.debug_call().address
    }

    /// Returns the current call kind.
    pub(crate) fn call_kind(&self) -> CallKind {
        self.debug_call().kind
    }

    /// Returns the current debug steps.
    pub(crate) fn debug_steps(&self) -> &[DebugStep] {
        &self.debug_call().steps
    }

    /// Returns the current debug step.
    pub(crate) fn current_step(&self) -> &DebugStep {
        &self.debug_steps()[self.current_step]
    }

    fn gen_opcode_list(&mut self) {
        self.opcode_list.clear();
        let debug_steps = &self.debugger.debug_arena[self.draw_memory.inner_call_index].steps;
        self.opcode_list
            .extend(debug_steps.iter().map(DebugStep::pretty_opcode));
    }
}

impl DebuggerContext<'_> {
    pub(crate) fn handle_event(&mut self) -> ControlFlow<ExitReason> {
        if self.last_index != self.draw_memory.inner_call_index {
            self.gen_opcode_list();
            self.last_index = self.draw_memory.inner_call_index;
        }

        self.handle_key_event()
    }

    fn handle_key_event(&mut self) -> ControlFlow<ExitReason> {
        for _ in 0..buffer_as_number(&self.key_buffer, 1) {
            if self.current_step < self.opcode_list.len() - 1 {
                self.current_step += 1;
            } else if self.draw_memory.inner_call_index < self.debug_arena().len() - 1 {
                self.draw_memory.inner_call_index += 1;
                self.current_step = 0;
            }
        }
        self.key_buffer.clear();
        ControlFlow::Continue(())
    }
}

/// Grab number from buffer. Used for something like '10k' to move up 10 operations
fn buffer_as_number(s: &str, default_value: usize) -> usize {
    match s.parse() {
        Ok(num) if num >= 1 => num,
        _ => default_value,
    }
}
