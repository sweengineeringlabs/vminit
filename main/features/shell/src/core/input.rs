use crate::api::ShellInput;
use std::io::Read;

pub(crate) struct DefaultShellInput;

impl ShellInput for DefaultShellInput {
    fn read_byte(&self) -> Option<u8> {
        let mut buf = [0u8; 1];
        match std::io::stdin().read(&mut buf) {
            Ok(1) => Some(buf[0]),
            _ => None,
        }
    }
}
