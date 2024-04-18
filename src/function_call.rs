use crate::step::{Step, VecStep};
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
        let contract_name =
            get_contract_name_from_acc(&acc_arr[0]).expect("source code should be of contract");
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
                let function_name = get_name(acc); //.expect("source code should be of function");
                let function_name_next = get_function_name_from_acc(acc_next); //.expect("source code should be of function");
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
                if let Some(contract_name) = get_contract_name_from_acc(acc_next) {
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
                let name = get_name(acc).unwrap();
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

pub fn get_contract_name_from_acc(acc: &Step) -> Option<String> {
    // get_name(acc)
    get_next(&acc.source_code, "contract ", vec![' ', '{'])
        .or_else(|| get_next(&acc.source_code, "abstract contract ", vec![' ', '{']))
}

pub fn get_function_name_from_acc(acc: &Step) -> Option<String> {
    // get_name(acc)
    get_next(&acc.source_code, "function ", vec![' ', '('])
}

pub fn get_name(acc: &Step) -> Option<String> {
    get_next(&acc.source_code, "contract ", vec![' ', '{'])
        .or_else(|| get_next(&acc.source_code, "abstract contract ", vec![' ', '{']))
        .or_else(|| get_next(&acc.source_code, "function ", vec![' ', '(']))
        .or_else(|| get_after_dot(&acc.source_code, vec!['(']))
        .or_else(|| get_next(&acc.source_code, "", vec!['(']))
}

// replace these by regular expressions
pub fn get_next(str: &str, prepend: &str, breakers: Vec<char>) -> Option<String> {
    if str.starts_with(prepend) {
        let start = prepend.len();
        let mut end = start;
        loop {
            let nth = &str.chars().nth(end);
            if nth.is_none() {
                return None;
            }
            if breakers.contains(&nth.unwrap()) {
                break;
            }
            end += 1;
        }
        Some(str[start..end].to_owned())
    } else {
        None
    }
}

// replace these by regular expressions
pub fn get_after_dot(str: &str, breakers: Vec<char>) -> Option<String> {
    // cases
    // uint256(0x0000000000000000000000000000000000000000000000000000000000000000).toField()
    let mut start = 0;
    let mut dot_found = false;
    let mut end = start;
    loop {
        let nth = &str.chars().nth(end);
        if nth.is_none() {
            return None;
        }
        if nth.unwrap() == '.' {
            start = end + 1;
            dot_found = true;
        }
        if dot_found && breakers.contains(&nth.unwrap()) {
            break;
        }
        end += 1;
    }
    Some(str[start..end].to_owned())
}
