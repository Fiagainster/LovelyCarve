//! In-memory state for background carving sessions.

use serde::Serialize;

use crate::core::carving::carver::{CarveArtifact, CarveScanProgress};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CarveSessionStatus {
    Queued,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl CarveSessionStatus {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug)]
pub struct CarveScanSession {
    pub scan_id: String,
    pub task_id: String,
    pub image_id: String,
    pub created_unix_ms: i64,
    pub status: CarveSessionStatus,
    pub scope_label: String,
    pub bytes_scanned: u64,
    pub bytes_total: u64,
    pub signatures_found: u64,
    pub artifacts: Vec<CarveArtifact>,
    pub current_offset: u64,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CarveScanSnapshot {
    pub scan_id: String,
    pub task_id: String,
    pub image_id: String,
    pub created_unix_ms: i64,
    pub status: CarveSessionStatus,
    pub scope_label: String,
    pub bytes_scanned: u64,
    pub bytes_total: u64,
    pub signatures_found: u64,
    pub artifacts_found: u64,
    pub current_offset: u64,
    pub truncated: bool,
    pub error: Option<String>,
}

impl CarveScanSession {
    pub fn new(
        scan_id: String,
        task_id: String,
        image_id: String,
        scope_label: String,
        bytes_total: u64,
    ) -> Self {
        Self {
            scan_id,
            task_id,
            image_id,
            created_unix_ms: chrono::Utc::now().timestamp_millis(),
            status: CarveSessionStatus::Queued,
            scope_label,
            bytes_scanned: 0,
            bytes_total,
            signatures_found: 0,
            artifacts: Vec::new(),
            current_offset: 0,
            truncated: false,
            error: None,
        }
    }

    pub fn snapshot(&self) -> CarveScanSnapshot {
        CarveScanSnapshot {
            scan_id: self.scan_id.clone(),
            task_id: self.task_id.clone(),
            image_id: self.image_id.clone(),
            created_unix_ms: self.created_unix_ms,
            status: self.status,
            scope_label: self.scope_label.clone(),
            bytes_scanned: self.bytes_scanned,
            bytes_total: self.bytes_total,
            signatures_found: self.signatures_found,
            artifacts_found: self.artifacts.len() as u64,
            current_offset: self.current_offset,
            truncated: self.truncated,
            error: self.error.clone(),
        }
    }

    pub fn apply_progress(&mut self, progress: &CarveScanProgress) {
        self.bytes_scanned = progress.bytes_scanned;
        self.bytes_total = progress.bytes_total;
        self.signatures_found = progress.signatures_found;
        self.current_offset = progress.current_offset;
    }
}
