/// Named parameter of an EVM opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OpcodeParam {
    /// The name of the parameter.
    pub(crate) name: &'static str,
    /// The index of the parameter on the stack. This is relative to the top of the stack.
    pub(crate) index: usize,
}
