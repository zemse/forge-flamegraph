use forge::debug::DebugStep;
use foundry_compilers::sourcemap::SourceElement;

use crate::function_call::RcRefCellFunctionCall;

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
