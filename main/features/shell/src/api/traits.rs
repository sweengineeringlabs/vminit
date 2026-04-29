/// Output sink for shell commands.
pub trait ShellOutput: Send {
    fn write_bytes(&self, data: &[u8]);

    fn write_str(&self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    fn write_line(&self, s: &str) {
        self.write_str(s);
        self.write_bytes(b"\n");
    }
}

/// Input source for the REPL.
pub trait ShellInput: Send {
    fn read_byte(&self) -> Option<u8>;
}
