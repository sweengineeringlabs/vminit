pub mod io;

#[cfg(feature = "std")]
pub mod builtins;
#[cfg(feature = "std")]
pub mod repl;
#[cfg(feature = "std")]
pub mod shell;

#[cfg(not(feature = "std"))]
pub mod builtins {
    use alloc::string::String;
    pub fn dispatch(_args: &[String]) -> Option<i32> { None }
}

#[cfg(not(feature = "std"))]
pub mod repl {
    use alloc::string::String;
    pub fn run(_env: &[(String, String)]) -> i32 { 0 }
}
