/// Input source for the REPL.
pub trait ShellInput: Send {
    fn read_byte(&self) -> Option<u8>;
}
