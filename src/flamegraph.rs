use foundry_evm_traces::{CallTraceDecoder, CallTraceNode, DecodedCallTrace};
use inferno::flamegraph;
use std::{fs, path::Path};

pub struct Flamegraph<'a> {
    folded_stack_lines: Vec<String>,
    pub options: flamegraph::Options<'a>,
}

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
                line.push(get_display(el));
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

    pub fn generate(&self, file_name: String) {
        let mut options = flamegraph::Options::default();

        if Path::new(&file_name).exists() {
            fs::remove_file(&file_name).unwrap();
        }

        let file = fs::File::create(&file_name).unwrap();

        flamegraph::from_lines(
            &mut options,
            self.folded_stack_lines.iter().map(|s| s.as_str()),
            file,
        )
        .unwrap();
    }
}

fn get_display(el: &(&CallTraceNode, DecodedCallTrace)) -> String {
    format!(
        "{contract_name}.{func_name}",
        contract_name =
            el.1.contract
                .as_ref()
                .unwrap_or(&"<unknown-contract>".to_string()),
        func_name =
            el.1.func
                .as_ref()
                .map(|f| &f.signature)
                .unwrap_or(&"<unknown-function>".to_string())
    )
}
