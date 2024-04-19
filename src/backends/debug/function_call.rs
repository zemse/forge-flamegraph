use super::{step::VecStep, utils::get_next};
use foundry_compilers::sourcemap::Jump;
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

#[derive(Clone, Serialize, Debug)]
pub struct RcRefCellFunctionCall(pub Rc<RefCell<FunctionCall>>);

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

// TODO create a parser file and move this logic there
impl RcRefCellFunctionCall {
    pub fn parse_steps(steps: &VecStep) -> RcRefCellFunctionCall {
        let steps = &steps.0;
        assert_eq!(
            steps[0].current_step.total_gas_used, 0,
            "this should be the start"
        );
        let contract_name = steps[0]
            .get_contract_name()
            .expect("source code should be of contract");
        let top_call = Rc::new(RefCell::new(FunctionCall {
            title: format!("{contract_name}.fallback"),
            name: format!("{contract_name}.fallback"),
            gas_start: 0,
            gas_end: None,
            color: String::new(),
            is_external_call: true,
            calls: vec![],
            parent: None,
        }));
        let mut ptr = Rc::clone(&top_call);

        for (i, step) in steps.iter().enumerate() {
            if i == 0 {
                // we have handled the first one already
                continue;
            }

            if step.source_element.jump == Jump::In {
                let step_next = &steps[i + 1];
                let function_name = step.get_name(); //.expect("source code should be of function");
                let function_name_next = step_next.get_function_name(); //.expect("source code should be of function");
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
                    gas_start: step.current_step.total_gas_used,
                    gas_end: None,
                    color: String::new(),
                    is_external_call: false,
                    calls: vec![],
                    parent: Some(ptr_weak),
                }));
                ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                ptr = new_call;
            }

            // CALL or STATICCALL
            if step.current_step.instruction == 0xF1 || step.current_step.instruction == 0xFA {
                let ptr_weak = Rc::downgrade(&ptr);
                let step_next = &steps[i + 1];
                if let Some(contract_name) = step_next.get_contract_name() {
                    let new_call = Rc::new(RefCell::new(FunctionCall {
                        title: format!("{contract_name}.fallback"),
                        name: format!("{contract_name}.fallback"),
                        gas_start: step.current_step.total_gas_used,
                        gas_end: None,
                        color: String::new(),
                        is_external_call: true,
                        calls: vec![],
                        parent: Some(ptr_weak),
                    }));
                    ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                    ptr = new_call;
                } else {
                    let function_name = get_next(&step.source_code, "", vec!['(']);
                    let function_name_next = step_next.get_name();
                    let function_name = function_name.or(function_name_next);
                    if function_name.is_none() {
                        println!(
                            "no-name for native function at {:#?} \nnext step {:#?}",
                            step, step_next
                        );
                        break;
                    }
                    let function_name = function_name.unwrap();
                    let new_call = Rc::new(RefCell::new(FunctionCall {
                        title: format!("{function_name} nativecode"),
                        name: function_name,
                        gas_start: step.current_step.total_gas_used,
                        gas_end: Some(step_next.current_step.total_gas_used),
                        color: String::new(),
                        is_external_call: true,
                        calls: vec![],
                        parent: Some(ptr_weak),
                    }));
                    ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                };
            }

            // internal function call ends
            if step.source_element.jump == Jump::Out {
                // let name = step.get_name().unwrap_or("unknown".to_string());
                // let step_next = &steps[i + 1];
                // if !step_next.source_code.contains(&name) {
                //     continue;
                // }

                // let ptr_weak = Rc::downgrade(&ptr);
                // let return_dummy_call = Rc::new(RefCell::new(FunctionCall {
                //     title: format!(
                //         "return {name} pc: {}, total_gas_used: {}",
                //         step.current_step.pc, step.current_step.total_gas_used
                //     ),
                //     name: "return".to_string(),
                //     gas_start: 0,
                //     gas_end: Some(0),
                //     is_external_call: false,
                //     color: String::new(),
                //     calls: vec![],
                //     parent: Some(ptr_weak),
                // }));
                // ptr.borrow_mut().calls.push(return_dummy_call);
                let parent_ptr = if let Some(ptr) = ptr.borrow_mut().parent.as_ref() {
                    Weak::clone(ptr)
                } else {
                    println!("no parent found for {}", step.source_code);
                    break;
                };

                ptr.borrow_mut().gas_end = Some(step.current_step.total_gas_used);

                ptr = parent_ptr.upgrade().unwrap();
            }

            if step.current_step.instruction == 0xF3
                || step.current_step.instruction == 0xFD
                || step.current_step.instruction == 0x00
            {
                let parent_ptr = if let Some(ptr) = ptr.borrow_mut().parent.as_ref() {
                    Weak::clone(ptr)
                } else {
                    println!("no parent found for {}", step.source_code);
                    break;
                };

                ptr.borrow_mut().gas_end = Some(step.current_step.total_gas_used);

                ptr = parent_ptr.upgrade().unwrap();
            }

            // // external call ends
            // if step.current_step.instruction == 0xF3
            //     || step.current_step.instruction == 0xFD
            //     || step.current_step.instruction == 0x00
            // {
            //     loop {
            //         let parent_ptr = Weak::clone(ptr.borrow_mut().parent.as_ref().unwrap());

            //         let step_next = &steps[i + 1];
            //         ptr.borrow_mut().gas_end = Some(step_next.current_step.total_gas_used);

            //         // let prev_ptr = ptr;
            //         ptr = parent_ptr.upgrade().unwrap();

            //         // if prev_ptr.borrow_mut().is_external_call {
            //         //     let ptr_weak = Rc::downgrade(&ptr);
            //         //     let name = step
            //         //         .get_name()
            //         //         .unwrap_or_else(|| step.get_source_code_stripped(30));
            //         //     let return_dummy_call = Rc::new(RefCell::new(FunctionCall {
            //         //         title: format!(
            //         //             "returncall {name} pc: {}, total_gas_used: {}",
            //         //             step.current_step.pc, step.current_step.total_gas_used
            //         //         ),
            //         //         name: "return".to_string(),
            //         //         gas_start: 0,
            //         //         gas_end: Some(0),
            //         //         is_external_call: false,
            //         //         color: String::new(),
            //         //         calls: vec![],
            //         //         parent: Some(ptr_weak),
            //         //     }));
            //         //     ptr.borrow_mut().calls.push(return_dummy_call);
            //         //     break;
            //         // }
            //     }
            // }
        }

        RcRefCellFunctionCall(top_call)
    }
}
