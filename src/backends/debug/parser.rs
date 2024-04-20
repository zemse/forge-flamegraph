use super::{debugger::Debugger, function_call::FunctionCall, step::VecStep, utils::get_next};
use crate::flamegraph::{self, Flamegraph};
use forge::result::TestResult;
use foundry_common::compile::ContractSources;
use foundry_compilers::sourcemap::Jump;
use foundry_evm_traces::CallTraceDecoder;
use revm::interpreter::OpCode;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

impl<'a> Flamegraph<'a> {
    pub fn from_debug_trace(
        sources: ContractSources,
        test_result: &TestResult,
        decoder: &CallTraceDecoder,
        merge_stacks: bool,
    ) -> eyre::Result<Self> {
        let builder = Debugger::builder()
            .debug_arenas(test_result.debug.as_slice())
            .sources(sources)
            .breakpoints(test_result.breakpoints.clone())
            .decoder(decoder);
        let mut debugger = builder.build();

        // collect the debug steps along with source mappings
        let mut steps = VecStep::default();
        debugger.try_run(&mut steps)?;

        // parse the debug steps into a call tree
        let top_call = parse_steps(&steps, merge_stacks);

        // parse the call tree into folded stack lines
        let mut flamegraph = Self {
            folded_stack_lines: vec![],
            options: flamegraph::Options::default(),
        };
        flamegraph.handle_call(&top_call, None);
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

        // the folded_stack_line is still incomplete, need to include gas
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

        // adding the gas in the folded_stack_line
        self.folded_stack_lines[idx] = format!("{} {}", self.folded_stack_lines[idx], gas_here);

        gas_used
    }
}

pub fn parse_steps(steps: &VecStep, merge_stacks: bool) -> Rc<RefCell<FunctionCall>> {
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

        // if stacks are merged, some ops like DUP1 get shown
        if !merge_stacks {
            let ptr_weak = Rc::downgrade(&ptr);
            let step_next = &steps.get(i + 1);
            let opcode = OpCode::new(step.current_step.instruction)
                .unwrap()
                .to_string();
            let new_call = Rc::new(RefCell::new(FunctionCall {
                title: opcode.clone(),
                name: opcode,
                gas_start: step.current_step.total_gas_used,
                gas_end: step_next.map(|step_next| step_next.current_step.total_gas_used),
                color: String::new(),
                is_external_call: false,
                calls: vec![],
                parent: Some(ptr_weak),
            }));
            ptr.borrow_mut().calls.push(Rc::clone(&new_call));
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

    top_call
}
