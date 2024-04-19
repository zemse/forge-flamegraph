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
        let builder = Debugger::builder()
            .debug_arenas(test_result.debug.as_slice())
            .sources(sources)
            .breakpoints(test_result.breakpoints.clone())
            .decoder(decoder);

        let mut debugger = builder.build();

        let mut steps = VecStep::default();

        // println!("steps {:#?}", steps);

        debugger.try_run(&mut steps)?;

        let top_call = steps.parse();

        let mut flamegraph = Self {
            folded_stack_lines: vec![],
            options: flamegraph::Options::default(),
        };
        flamegraph.options.flame_chart = true;
        flamegraph.handle_call(&top_call.0, None);

        flamegraph.folded_stack_lines.reverse();

        Ok(flamegraph)
    }

    fn handle_call(
        &mut self,
        call: &Rc<RefCell<FunctionCall>>,
        folded_stack_line_prepend: Option<&String>,
    ) -> i64 {
        let name = &call.borrow().name;
        let children = &call.borrow().calls;

        let folded_stack_line = folded_stack_line_prepend
            .map(|prepend| format!("{};{}", prepend, name))
            .unwrap_or_else(|| name.clone());

        // we still have to add gass here
        let idx = self.folded_stack_lines.len();
        self.folded_stack_lines.push(folded_stack_line.clone());

        let mut child_gas = 0;
        for child in children {
            child_gas += self.handle_call(child, Some(&folded_stack_line));
        }

        let gas_start = call.borrow().gas_start;
        let gas_end = call.borrow().gas_end;
        let gas_used = gas_end
            .map(|gas_end| (gas_end as i64) - (gas_start as i64))
            .unwrap_or(0);
        let mut gas_here = gas_used - child_gas;
        if gas_here < 0 {
            // because some issues with flamegraph
            gas_here = 0;
        }

        self.folded_stack_lines[idx] = format!("{} {}", self.folded_stack_lines[idx], gas_here);

        gas_used
    }
}
