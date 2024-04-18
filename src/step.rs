use forge::debug::DebugStep;
use foundry_compilers::sourcemap::SourceElement;

use crate::{
    function_call::RcRefCellFunctionCall,
    utils::{get_after_dot, get_next},
};

pub struct Step {
    pub source_element: SourceElement,
    pub source_code: String,
    pub current_step: DebugStep,
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let max = 90;
        let source_code = if self.source_code.len() > max {
            &self.source_code[..max]
        } else {
            &self.source_code
        };
        write!(
            f,
            "Acc {{ 
                source_element: {:?}, 
                source_code: {:?}, 
                current_step: {:?} 
            }}",
            self.source_element, source_code, self.current_step
        )
    }
}

impl std::cmp::PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.source_element == other.source_element && self.source_code == other.source_code
    }
}

impl Step {
    pub fn get_contract_name(&self) -> Option<String> {
        // get_name(acc)
        get_next(&self.source_code, "contract ", vec![' ', '{'])
            .or_else(|| get_next(&self.source_code, "abstract contract ", vec![' ', '{']))
    }

    pub fn get_function_name(&self) -> Option<String> {
        // get_name(acc)
        get_next(&self.source_code, "function ", vec![' ', '('])
    }

    pub fn get_name(&self) -> Option<String> {
        get_next(&self.source_code, "contract ", vec![' ', '{'])
            .or_else(|| get_next(&self.source_code, "abstract contract ", vec![' ', '{']))
            .or_else(|| get_next(&self.source_code, "function ", vec![' ', '(']))
            .or_else(|| get_after_dot(&self.source_code, vec!['(']))
            .or_else(|| get_next(&self.source_code, "", vec!['(']))
    }
}

#[derive(Default, Debug)]
pub struct VecStep(pub Vec<Step>);

impl VecStep {
    pub fn into_call_tree(&self) -> RcRefCellFunctionCall {
        RcRefCellFunctionCall::from_vec_step(self)
    }

    pub fn push(&mut self, step: Step) {
        self.0.push(step);
    }
}
