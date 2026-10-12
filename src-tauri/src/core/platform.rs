//! platform domain modules.

#[cfg(target_os = "windows")]
pub mod dokan_mount;
pub mod ffi;
pub mod resources;
pub mod windows_share;
