use crate::flamegraph::{self, Flamegraph};
use foundry_evm_traces::{CallTraceDecoder, CallTraceNode};

mod utils;

impl<'a> Flamegraph<'a> {
    pub async fn from_call_trace(nodes: &[CallTraceNode], decoder: &CallTraceDecoder) -> Self {
        let mut decoded = vec![];

        for node in nodes {
            let function = decoder.decode_function(&node.trace).await;
            decoded.push((node, function));
        }

        let mut folded_stack_lines = vec![];
        for current in &decoded {
            let mut stack = vec![];
            let mut ptr = current;
            loop {
                stack.push(ptr);
                if let Some(parent_idx) = ptr.0.parent {
                    ptr = &decoded[parent_idx];
                } else {
                    break;
                }
            }

            let mut line = vec![];
            while let Some(el) = stack.pop() {
                line.push(utils::get_display(el));
            }

            let mut gas = current.0.trace.gas_used as i64;
            for child_idx in &current.0.children {
                gas -= decoded[*child_idx].0.trace.gas_used as i64;
            }

            let line = [line.join(";"), gas.to_string()].join(" ");
            folded_stack_lines.push(line);
        }
        Self {
            folded_stack_lines,
            options: flamegraph::Options::default(),
        }
    }
}
