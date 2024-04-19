use std::{cell::RefCell, rc::Rc};

use forge::result::TestResult;
use foundry_common::compile::ContractSources;
use foundry_evm_traces::CallTraceDecoder;

use crate::flamegraph::{self, Flamegraph};

use self::{debugger::Debugger, function_call::FunctionCall, step::VecStep};

pub mod debugger;
mod function_call;
pub mod step;
mod utils;

impl<'a> Flamegraph<'a> {
    pub fn from_debug_trace(
        sources: ContractSources,
        test_result: &TestResult,
        decoder: &CallTraceDecoder,
    ) -> eyre::Result<Self> {
        let mut builder = Debugger::builder()
            .debug_arenas(test_result.debug.as_slice())
            .sources(sources)
            .breakpoints(test_result.breakpoints.clone())
            .decoder(decoder);

        let mut debugger = builder.build();

        let mut steps = VecStep::default();

        // println!("steps {:#?}", steps);

        debugger.try_run(&mut steps)?;

        let top_call = steps.parse();

        let flamegraph = Self {
            folded_stack_lines: vec![],
            options: flamegraph::Options::default(),
        };

        Ok(flamegraph)
    }

    fn handle_call(
        &mut self,
        call: &Rc<RefCell<FunctionCall>>,
        folded_stack_line_prepend: Option<&String>,
    ) -> i64 {
        let title = &call.borrow().title;
        let children = &call.borrow().calls;

        let folded_stack_line = folded_stack_line_prepend
            .map(|prepend| format!("{};{}", prepend, title))
            .unwrap_or_else(|| title.clone());

        let mut child_gas = 0;
        for child in children {
            child_gas += self.handle_call(child, Some(&folded_stack_line));
        }

        // let gas_used = call.borrow().gas_start - call.borrow().gas_end;
        // self.folded_stack_lines.push([folded_stack_line,call.borrow().value.to_string()].join(" "));]);

        0
    }
}
