//! # foundry-debugger
//!
//! Interactive Solidity TUI debugger.

#![warn(unused_crate_dependencies, unreachable_pub)]

// #[macro_use]
// extern crate tracing;

pub mod op;

mod tui;
pub use tui::{Acc, Debugger, DebuggerBuilder, ExitReason};
