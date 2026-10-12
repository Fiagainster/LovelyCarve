import { invokeCmd } from './client';
import { onEvent, type UnlistenFn } from './events';

export type CarveScope = 'whole' | 'unallocated' | 'partition';
export type CarveSessionStatus = 'queued' | 'running' | 'paused' | 'completed' | 'cancelled' | 'failed';
export type CarveCategory = 'image' | 'document' | 'archive' | 'database' | 'audio' | 'video';

export interface CarveFormatInfo {
  id: string;
  extension: string;
  label: string;
  category: CarveCategory;
  description: string;
  signatures: string[];
}

export interface CarveStartOptions {
  format_ids: string[];
  scope: CarveScope;
  partition_id: number | null;
  max_file_size: number;
  max_results: number;
}

export interface CarveStartResult {
  scan_id: string;
  task_id: string;
}

export interface CarveArtifact {
  id: string;
  name: string;
  format_id: string;
  extension: string;
  category: CarveCategory;
  offset: number;
  length: number;
  confidence: number;
  region: string;
  read_fault: boolean;
}

export interface CarveScanSnapshot {
  scan_id: string;
  task_id: string;
  image_id: string;
  created_unix_ms: number;
  status: CarveSessionStatus;
  scope_label: string;
  bytes_scanned: number;
  bytes_total: number;
  signatures_found: number;
  artifacts_found: number;
  current_offset: number;
  truncated: boolean;
  error: string | null;
}

export interface CarveResultFilter {
  format_id?: string | null;
  category?: CarveCategory | null;
  min_confidence?: number | null;
}

export interface CarveResultsPage {
  items: CarveArtifact[];
  next_cursor: string | null;
  /** Last stable result key returned even while a background scan is growing. */
  resume_cursor: string | null;
  total: number;
  truncated: boolean;
  scan_finished: boolean;
}

export interface CarveRecoveryResult {
  recovered: number;
  failed: number;
  bytes_written: number;
  synthetic_artifacts: number;
  report_path: string | null;
  report_error: string | null;
  errors: string[];
  errors_truncated: boolean;
}

export const CARVE_SCAN_PROGRESS_EVENT = 'carve-scan-progress';

export function carveFormats(): Promise<CarveFormatInfo[]> {
  return invokeCmd<CarveFormatInfo[]>('carve_formats');
}

export function carveStart(args: {
  imageId: string;
  options: CarveStartOptions;
}): Promise<CarveStartResult> {
  return invokeCmd<CarveStartResult>('carve_start', args);
}

export function carveScanStatus(args: { scanId: string }): Promise<CarveScanSnapshot> {
  return invokeCmd<CarveScanSnapshot>('carve_scan_status', args);
}

export function carveScanPause(args: { scanId: string }): Promise<CarveScanSnapshot> {
  return invokeCmd<CarveScanSnapshot>('carve_scan_pause', args);
}

export function carveScanResume(args: { scanId: string }): Promise<CarveScanSnapshot> {
  return invokeCmd<CarveScanSnapshot>('carve_scan_resume', args);
}

export function carveLatestScan(args: { imageId: string }): Promise<CarveScanSnapshot | null> {
  return invokeCmd<CarveScanSnapshot | null>('carve_latest_scan', args);
}

export function carveResultsPage(args: {
  scanId: string;
  cursor?: string | null;
  limit?: number;
  filter?: CarveResultFilter | null;
}): Promise<CarveResultsPage> {
  return invokeCmd<CarveResultsPage>('carve_results_page', args);
}

/** Read a bounded carved-file prefix as a Tauri raw binary response. */
export function carvePreview(args: {
  scanId: string;
  artifactId: string;
  maxBytes?: number;
}): Promise<ArrayBuffer> {
  return invokeCmd<ArrayBuffer>('carve_preview', args);
}

export function carveRecover(args: {
  scanId: string;
  options: {
    artifact_ids: string[] | null;
    destination: string;
    task_id: string;
  };
}): Promise<CarveRecoveryResult> {
  return invokeCmd<CarveRecoveryResult>('carve_recover', args);
}

export function onCarveScanProgress(
  handler: Parameters<typeof onEvent<CarveScanSnapshot>>[1],
): Promise<UnlistenFn> {
  return onEvent<CarveScanSnapshot>(CARVE_SCAN_PROGRESS_EVENT, handler);
}

/** Retained carving sessions for the current evidence, newest first. */
export function carveScanList(args: { imageId: string }): Promise<CarveScanSnapshot[]> {
  return invokeCmd<CarveScanSnapshot[]>('carve_scan_list', args);
}

export function carveScanRemove(args: { imageId: string; scanId: string }): Promise<void> {
  return invokeCmd<void>('carve_scan_remove', args);
}
