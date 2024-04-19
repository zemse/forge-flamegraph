use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct FunctionCall {
    pub title: String,
    pub name: String,
    pub gas_start: u64,
    pub gas_end: Option<u64>,
    pub color: String,
    // might be useful to resync call depth
    pub is_external_call: bool,
    #[serde(rename = "children")]
    pub calls: Vec<Rc<RefCell<FunctionCall>>>,
    #[serde(skip)]
    pub parent: Option<Weak<RefCell<FunctionCall>>>,
}

// #[derive(Clone, Serialize, Debug)]
// pub struct RcRefCellFunctionCall(pub Rc<RefCell<FunctionCall>>);

use std::{
    cell::RefCell,
    fmt::Debug,
    rc::{Rc, Weak},
};

impl Debug for FunctionCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _self = self.to_owned();
        let _ref = Rc::new(RefCell::new(_self));
        writeln!(f).unwrap();
        print_call(&_ref, 0, f);
        Ok(())
    }
}

fn print_call(call: &Rc<RefCell<FunctionCall>>, depth: usize, f: &mut std::fmt::Formatter<'_>) {
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

// impl FunctionCall {
// #[async_recursion]
// pub async fn from_node(
//     node: &CallTraceNode,
//     decoder: &CallTraceDecoder,
//     nodes: &[CallTraceNode],
// ) -> FunctionCall {
//     let decoded = decoder.decode_function(&node.trace).await;
//     let contract_name = decoded.label.unwrap_or("<unknown>".to_string());
//     let function_name = decoded.func.unwrap_or_default().signature;
//     let mut call = FunctionCall {
//         title: format!("{contract_name}.{function_name}"),
//         name: format!("{contract_name}.{function_name}"),
//         gas_start: 0,
//         gas_end: Some(0),
//         color: "".to_string(),
//         is_external_call: true,
//         calls: vec![],
//         parent: None,
//     };
//     for child_idx in node.children.iter() {
//         let child_node = &nodes[*child_idx];
//         let child_call = FunctionCall::from_node(child_node, decoder, nodes).await;
//         let child_call_ptr = Rc::new(RefCell::new(child_call));
//         call.calls.push(child_call_ptr);
//     }
//     call
// }
// }
