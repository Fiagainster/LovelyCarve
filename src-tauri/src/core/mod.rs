//! Forensic engines organized by responsibility. Application state and IPC live outside this layer.

pub mod analysis;
pub mod carving;
pub mod casework;
pub mod catalog;
pub mod common;
pub mod evidence;
pub mod extensions;
pub mod filesystem;
pub mod platform;
pub mod preview;
pub mod search;
pub mod timeline;
