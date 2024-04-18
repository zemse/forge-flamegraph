use super::context::DebuggerContext;
use foundry_compilers::sourcemap::SourceElement;
use foundry_evm_core::debug::DebugStep;
use revm_inspectors::tracing::types::CallKind;
use std::io;

pub struct Acc {
    pub source_element: SourceElement,
    pub source_code: String,
    pub current_step: DebugStep,
}

impl std::fmt::Debug for Acc {
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

impl std::cmp::PartialEq for Acc {
    fn eq(&self, other: &Self) -> bool {
        self.source_element == other.source_element && self.source_code == other.source_code
    }
}

impl DebuggerContext<'_> {
    /// Draws the TUI layout and subcomponents to the given terminal.
    pub(crate) fn draw(&self, acc: &mut Vec<Acc>) -> io::Result<()> {
        self.draw_layout(acc);
        Ok(())
    }

    #[inline]
    fn draw_layout(&self, acc: &mut Vec<Acc>) {
        self.horizontal_layout(acc);
    }

    fn horizontal_layout(&self, acc: &mut Vec<Acc>) {
        self.draw_src(acc);
    }

    fn draw_src(&self, acc: &mut Vec<Acc>) {
        self.src_text(acc);
    }

    fn src_text(&self, acc: &mut Vec<Acc>) {
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

        let new_acc = Acc {
            source_element: source_element.clone(),
            source_code: source_code[actual_start..actual_end].to_string(),
            current_step: self.current_step().clone(),
        };
        acc.push(new_acc);
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
