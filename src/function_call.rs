use crate::{step::VecStep, utils::get_next};
use foundry_compilers::sourcemap::Jump;
use serde::Serialize;

#[derive(Clone, Serialize)]
struct FunctionCall {
    title: String,
    name: String,
    gas_start: u64,
    gas_end: Option<u64>,
    color: String,
    #[serde(rename = "children")]
    calls: Vec<Rc<RefCell<FunctionCall>>>,
    #[serde(skip)]
    parent: Option<Weak<RefCell<FunctionCall>>>,
}

#[derive(Clone, Serialize, Debug)]
pub struct RcRefCellFunctionCall(Rc<RefCell<FunctionCall>>);

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

impl RcRefCellFunctionCall {
    pub fn from_vec_step(vec_step: &VecStep) -> RcRefCellFunctionCall {
        let acc_arr = &vec_step.0;
        assert_eq!(
            acc_arr[0].current_step.total_gas_used, 0,
            "this should be the start"
        );
        let contract_name = acc_arr[0]
            .get_contract_name()
            .expect("source code should be of contract");
        let top_call = Rc::new(RefCell::new(FunctionCall {
            title: format!("{contract_name}.fallback"),
            name: contract_name,
            gas_start: 0,
            gas_end: None,
            color: String::new(),
            calls: vec![],
            parent: None,
        }));
        let mut ptr = Rc::clone(&top_call);

        for (i, acc) in acc_arr.iter().enumerate() {
            if i == 0 {
                // we have handled the first one already
                continue;
            }

            if acc.source_element.jump == Jump::In {
                let acc_next = &acc_arr[i + 1];
                let function_name = acc.get_name(); //.expect("source code should be of function");
                let function_name_next = acc_next.get_function_name(); //.expect("source code should be of function");
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

                    gas_start: acc.current_step.total_gas_used,
                    gas_end: None,
                    color: String::new(),
                    calls: vec![],
                    parent: Some(ptr_weak),
                }));
                ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                ptr = new_call;
            }

            // CALL or STATICCALL
            if acc.current_step.instruction == 0xF1 || acc.current_step.instruction == 0xFA {
                let ptr_weak = Rc::downgrade(&ptr);
                let acc_next = &acc_arr[i + 1];
                if let Some(contract_name) = acc_next.get_contract_name() {
                    let new_call = Rc::new(RefCell::new(FunctionCall {
                        title: format!("{contract_name}.fallback"),
                        name: contract_name,
                        gas_start: acc.current_step.total_gas_used,
                        gas_end: None,
                        color: String::new(),
                        calls: vec![],
                        parent: Some(ptr_weak),
                    }));
                    ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                    ptr = new_call;
                } else {
                    let function_name =
                        get_next(&acc.source_code, "", vec!['(']).expect("vm call native code");
                    let new_call = Rc::new(RefCell::new(FunctionCall {
                        title: format!("{function_name} nativecode"),
                        name: function_name,
                        gas_start: acc.current_step.total_gas_used,
                        gas_end: Some(acc_next.current_step.total_gas_used),
                        color: String::new(),
                        calls: vec![],
                        parent: Some(ptr_weak),
                    }));
                    ptr.borrow_mut().calls.push(Rc::clone(&new_call));
                };
            }

            // internal function call ends
            if acc.source_element.jump == Jump::Out {
                let name = acc.get_name().unwrap();
                // let acc_next = &acc_arr[i + 1];
                // if !acc_next.source_code.contains(&name) {
                //     continue;
                // }

                let ptr_weak = Rc::downgrade(&ptr);
                let return_dummy_call = Rc::new(RefCell::new(FunctionCall {
                    title: format!(
                        "return {name} pc: {}, total_gas_used: {}",
                        acc.current_step.pc, acc.current_step.total_gas_used
                    ),
                    name: "return".to_string(),
                    gas_start: 0,
                    gas_end: Some(0),
                    color: String::new(),
                    calls: vec![],
                    parent: Some(ptr_weak),
                }));
                ptr.borrow_mut().calls.push(return_dummy_call);
                let parent_ptr = if let Some(ptr) = ptr.borrow_mut().parent.as_ref() {
                    Weak::clone(ptr)
                } else {
                    println!("no parent found for {}", acc.source_code);
                    break;
                };

                ptr.borrow_mut().gas_end = Some(acc.current_step.total_gas_used);

                ptr = parent_ptr.upgrade().unwrap();
            }

            // call ends
            if acc.current_step.instruction == 0xF3
                || acc.current_step.instruction == 0xFD
                || acc.current_step.instruction == 0x00
            {
                let parent_ptr = Weak::clone(ptr.borrow_mut().parent.as_ref().unwrap());

                let acc_next = &acc_arr[i + 1];
                ptr.borrow_mut().gas_end = Some(acc_next.current_step.total_gas_used);

                ptr = parent_ptr.upgrade().unwrap();
            }
        }

        RcRefCellFunctionCall(top_call)
    }
}
