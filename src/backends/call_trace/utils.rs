use foundry_evm_traces::{CallTraceNode, DecodedCallTrace};

pub fn get_display(el: &(&CallTraceNode, DecodedCallTrace)) -> String {
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
