use std::{fs, io::Read, path::Path};

pub use inferno::flamegraph::{self, Options};

pub struct Flamegraph<'a> {
    pub folded_stack_lines: Vec<String>,
    pub options: Options<'a>,
}

impl<'a> Flamegraph<'a> {
    pub fn generate(&mut self, file_name: &String) {
        if Path::new(&file_name).exists() {
            fs::remove_file(file_name).unwrap();
        }

        self.options.title = file_name.clone();

        let file = fs::File::create(file_name).unwrap();

        flamegraph::from_lines(
            &mut self.options,
            self.folded_stack_lines.iter().map(|s| s.as_str()),
            file,
        )
        .unwrap();

        let mut buf = String::new();
        let mut file = fs::File::open(file_name).unwrap();
        file.read_to_string(&mut buf)
            .expect("failed to read flamegraph file");
        let buf = buf.replace("samples", "gas");
        fs::write(file_name, buf).expect("failed to write flamegraph file");
    }
}
