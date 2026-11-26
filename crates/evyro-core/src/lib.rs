//! Evyro core: the GUI-independent continuity engine.
//!
//! Everything durable lives here: the project registry, task graph, session
//! lifecycle, checkpoints, verification receipts, memory, and the handoff
//! (continuation contract) engine. The CLI, MCP server and the desktop app
//! are thin clients over this crate.

pub mod acceptance;
pub mod adapters;
pub mod agent_check;
pub mod backup;
pub mod brand;
pub mod checkout;
pub mod checkpoints;
pub mod comparison;
pub mod context;
pub mod contract;
pub mod crash;
pub mod custom_agents;
pub mod db;
pub mod decisions;
pub mod doctor;
pub mod error;
pub mod events;
pub mod git;
pub mod handoff;
pub mod handover;
pub mod health;
pub mod init;
pub mod memory;
pub mod ownership;
pub mod paths;
pub mod plans;
pub mod pr_feedback;
pub mod projects;
pub mod quota;
pub mod redact;
pub mod remote;
pub mod retention;
pub mod review;
pub mod sample;
pub mod search;
pub mod sessions;
pub mod skills;
pub mod split;
pub mod targets;
pub mod tasks;
pub mod util;
pub mod verify;

pub use db::Store;
pub use error::{Error, Result};
