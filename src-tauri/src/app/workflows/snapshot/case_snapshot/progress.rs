//! progress implementation.
use super::*;

pub(crate) fn emit_progress(app: &AppHandle, payload: SnapshotProgressPayload) {
    if let Err(err) = app.emit(SNAPSHOT_PROGRESS_EVENT, payload) {
        tracing::debug!("case: emit snapshot progress failed: {err}");
    }
}

pub(crate) fn emit_unified_snapshot_progress(
    tasks: &TaskManager,
    payload: &SnapshotProgressPayload,
) {
    if payload.task_id.is_empty() {
        return;
    }
    let message = payload
        .message
        .clone()
        .or_else(|| (payload.phase != "walking").then(|| format!("snapshot:{}", payload.phase)));
    tasks.emit_progress(
        &payload.task_id,
        TaskProgressUnit::Entries,
        payload.entries,
        None,
        message,
    );
}

/// Emits one progress event to both the task center and the snapshot chip.
pub(crate) fn report(app: &AppHandle, tasks: &TaskManager, payload: SnapshotProgressPayload) {
    emit_unified_snapshot_progress(tasks, &payload);
    emit_progress(app, payload);
}
