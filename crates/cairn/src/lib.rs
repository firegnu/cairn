//! cairn：终端 coding agent 的工作接续记忆。设计见 docs/DESIGN.md。

pub mod adopt;
pub mod cli;
mod commands;
pub mod facts;
pub mod hook;
pub mod ingest;
mod install;
mod install_json;
pub mod render;
pub mod save;
pub mod scope;
pub mod session;
pub mod spool;
mod status;
pub mod store;
pub mod turn;
