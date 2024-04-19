use std::{fs, path::Path};

pub use inferno::flamegraph::{self, Options};

pub struct Flamegraph<'a> {
    pub folded_stack_lines: Vec<String>,
    pub options: Options<'a>,
}

impl<'a> Flamegraph<'a> {
    pub fn generate(&mut self, file_name: String) {
        if Path::new(&file_name).exists() {
            fs::remove_file(&file_name).unwrap();
        }

        let file = fs::File::create(&file_name).unwrap();

        flamegraph::from_lines(
            &mut self.options,
            self.folded_stack_lines.iter().map(|s| s.as_str()),
            file,
        )
        .unwrap();
    }
}
