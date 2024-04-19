use super::super::super::step::{Step, VecStep};
use super::context::DebuggerContext;
use foundry_compilers::sourcemap::SourceElement;
use revm_inspectors::tracing::types::CallKind;
use std::io;

impl DebuggerContext<'_> {
    /// Draws the TUI layout and subcomponents to the given terminal.
    pub(crate) fn draw(&self, steps: &mut VecStep) -> io::Result<()> {
        self.draw_layout(steps);
        Ok(())
    }

    #[inline]
    fn draw_layout(&self, steps: &mut VecStep) {
        self.horizontal_layout(steps);
    }

    fn horizontal_layout(&self, steps: &mut VecStep) {
        self.draw_src(steps);
    }

    fn draw_src(&self, steps: &mut VecStep) {
        self.src_text(steps);
    }

    fn src_text(&self, steps: &mut VecStep) {
        let (source_element, source_code) = match self.src_map() {
            Ok(r) => r,
            Err(_) => return,
        };

        let offset = source_element.offset;
        let len = source_element.length;
        let max = source_code.len();

        // Split source into before, relevant, and after chunks, split by line, for formatting.
        let actual_start = offset.min(max);
        let actual_end = (offset + len).min(max);

        let new_step = Step {
            source_element: source_element.clone(),
            source_code: source_code[actual_start..actual_end].to_string(),
            current_step: self.current_step().clone(),
        };
        steps.push(new_step);
    }

    fn src_map(&self) -> Result<(SourceElement, &str), String> {
        let address = self.address();
        let Some(contract_name) = self.debugger.identified_contracts.get(address) else {
            return Err(format!("Unknown contract at address {address}"));
        };

        let Some(mut files_source_code) =
            self.debugger.contracts_sources.get_sources(contract_name)
        else {
            return Err(format!("No source map index for contract {contract_name}"));
        };

        let Some((create_map, rt_map)) = self.debugger.pc_ic_maps.get(contract_name) else {
            return Err(format!("No PC-IC maps for contract {contract_name}"));
        };

        let is_create = matches!(self.call_kind(), CallKind::Create | CallKind::Create2);
        let pc = self.current_step().pc;
        let Some((source_element, source_code)) =
            files_source_code.find_map(|(file_id, source_code, contract_source)| {
                let bytecode = if is_create {
                    &contract_source.bytecode
                } else {
                    contract_source.deployed_bytecode.bytecode.as_ref()?
                };
                let mut source_map = bytecode.source_map()?.ok()?;

                let pc_ic_map = if is_create { create_map } else { rt_map };
                let ic = pc_ic_map.get(pc)?;
                let source_element = source_map.swap_remove(ic);
                // if the source element has an index, find the sourcemap for that index
                source_element
                    .index
                    .and_then(|index|
                    // if index matches current file_id, return current source code
                    (index == file_id).then(|| (source_element.clone(), source_code)))
                    .or_else(|| {
                        // otherwise find the source code for the element's index
                        self.debugger
                            .contracts_sources
                            .sources_by_id
                            .get(&(source_element.index?))
                            .map(|source_code| (source_element.clone(), source_code.as_ref()))
                    })
            })
        else {
            return Err(format!("No source map for contract {contract_name}"));
        };

        Ok((source_element, source_code))
    }
}
