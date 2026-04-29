use crate::api::ShellOutput;
use std::io::Write;

pub(crate) struct DefaultShellOutput;

impl ShellOutput for DefaultShellOutput {
    fn write_bytes(&self, data: &[u8]) {
        let _ = std::io::stdout().write_all(data);
        let _ = std::io::stdout().flush();
    }
}
