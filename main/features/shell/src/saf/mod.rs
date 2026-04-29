pub use crate::api::*;
pub use crate::spi::builtins;
pub use crate::spi::io;
pub use crate::spi::repl;
pub use crate::spi::shell;

/// Wire the thread-local I/O sinks to the concrete default implementations.
///
/// Call once before `repl::run()` in environments that have not configured
/// custom I/O (i.e. not running under the test harness).
pub fn wire_default_io() {
    let out = crate::core::output::DefaultShellOutput;
    let inp = crate::core::input::DefaultShellInput;
    crate::spi::io::set_output(move |data| {
        use crate::api::ShellOutput;
        out.write_bytes(data);
    });
    crate::spi::io::set_input(move || {
        use crate::api::ShellInput;
        inp.read_byte()
    });
}
