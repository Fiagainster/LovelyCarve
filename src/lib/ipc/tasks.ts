// 插件任务调度 IPC。Typed wrappers
// over the `sched_*` backend commands plus the `sched-task-event` stream.
// Interfaces mirror the Rust serde structs in `core::task_scheduler`
// (camelCase on the wire).
//
// 生命周期:created → queued → running → success | failed | cancel;
// pause / offline 为可恢复的中间态。任务行持久化在案件台账 `case.db` 的
// `task` 表,重开案件后仍可见;进程内只保留运行句柄。时间戳是 RFC 3339 UTC。
import { invokeCmd } from './client';
import { onEvent, type UnlistenFn } from './events';

/** Rust `SchedTaskStatus`(serde lowercase)。 */
export type SchedTaskStatus =
  | 'created'
  | 'queued'
  | 'running'
  | 'success'
  | 'failed'
  | 'cancel'
  | 'pause'
  | 'offline';

/** Rust `TaskTrigger`(serde snake_case)。 */
export type SchedTaskTrigger = 'auto' | 'manually';

export interface SchedTaskParams {
  nodeId: string | null;
  fileName: string | null;
  /** 快速分析选定的平台。 */
  platform: string | null;
  /** 快速分析勾选的功能子项;空 = 全部。 */
  subItems: string[];
}

export interface SchedTaskInfo {
  pluginId: string;
  analyzer: string;
  imageId: string;
  trigger: SchedTaskTrigger;
  parentTid: number | null;
  error: string | null;
}

export interface SchedTaskResult {
  summary: string | null;
  artifactCount: number;
  timelineCount: number;
  findingCount: number;
  runId: string | null;
}

export interface SchedTaskRecord {
  id: number;
  /** function 名 `<plugin id>/<analyzer>`。 */
  name: string;
  displayName: string;
  typeName: string;
  /** 台账检材主键。 */
  evidenceId: number;
  /** 0–100。 */
  progress: number;
  params: SchedTaskParams;
  result: SchedTaskResult | null;
  dependence: string;
  status: SchedTaskStatus;
  info: SchedTaskInfo;
  canPause: boolean;
  singleInstance: boolean;
  internal: boolean;
  spendTimeSecs: number | null;
  remainTimeSecs: number | null;
  startAt: string | null;
  createAt: string;
  updateAt: string;
}

export interface SchedFunction {
  function: string;
  pluginId: string;
  pluginName: string;
  pluginVersion: string;
  analyzer: string;
  displayName: string;
  typeName: string;
  dependence: string | null;
  trigger: SchedTaskTrigger;
  internal: boolean;
  singleInstance: boolean;
  canPause: boolean;
  extensions: string[];
  mimeTypes: string[];
  priority: number;
  /** 适用平台;null = all。 */
  platform: string | null;
  /** 功能子项。 */
  support: string[];
  /** 适用检材类型;空 = 全部。 */
  evidenceTypes: string[];
  /** 检材级 function:任务不指向文件,插件自行遍历检材。 */
  evidenceLevel: boolean;
}

/** Rust `SchedNoticeKind`(serde camelCase)。 */
export type SchedNoticeKind = 'taskOffline' | 'taskTimeout' | 'allTasksFinished' | 'postActionFailed';

export interface SchedNotice {
  kind: SchedNoticeKind;
  taskId: number | null;
  evidenceId: number | null;
  message: string;
}

/** Rust `SchedEvent`(内部标签 `kind`)。 */
export type SchedEvent =
  | { kind: 'taskChanged'; task: SchedTaskRecord }
  | { kind: 'taskRemoved'; taskId: number; evidenceId: number }
  | { kind: 'notice'; notice: SchedNotice };

export interface SchedTaskCreateResponse {
  task: SchedTaskRecord;
  /** false:命中单实例约束,返回的是已存在的任务。 */
  created: boolean;
}

export const SCHED_TASK_EVENT = 'sched-task-event';

export function schedTaskFunctions(): Promise<SchedFunction[]> {
  return invokeCmd<SchedFunction[]>('sched_task_functions');
}

export function schedTaskList(args: {
  imageId?: string | null;
  includeInternal?: boolean;
} = {}): Promise<SchedTaskRecord[]> {
  return invokeCmd<SchedTaskRecord[]>('sched_task_list', {
    imageId: args.imageId ?? null,
    includeInternal: args.includeInternal ?? false,
  });
}

export function schedTaskCreate(args: {
  function: string;
  imageId: string;
  nodeId?: string | null;
  fileName?: string | null;
  platform?: string | null;
  subItems?: string[];
}): Promise<SchedTaskCreateResponse> {
  return invokeCmd<SchedTaskCreateResponse>('sched_task_create', {
    function: args.function,
    imageId: args.imageId,
    nodeId: args.nodeId ?? null,
    fileName: args.fileName ?? null,
    platform: args.platform ?? null,
    subItems: args.subItems ?? [],
  });
}

export function schedTaskCancel(taskId: number): Promise<SchedTaskRecord> {
  return invokeCmd<SchedTaskRecord>('sched_task_cancel', { taskId });
}

export function schedTaskPause(taskId: number): Promise<SchedTaskRecord> {
  return invokeCmd<SchedTaskRecord>('sched_task_pause', { taskId });
}

export function schedTaskStart(taskId: number): Promise<SchedTaskRecord> {
  return invokeCmd<SchedTaskRecord>('sched_task_start', { taskId });
}

export function schedTaskRestart(taskId: number): Promise<SchedTaskRecord> {
  return invokeCmd<SchedTaskRecord>('sched_task_restart', { taskId });
}

export function schedTaskRemove(taskId: number): Promise<SchedTaskRecord> {
  return invokeCmd<SchedTaskRecord>('sched_task_remove', { taskId });
}

export function schedEvidencePostActionsSet(args: {
  imageId: string;
  actionsJson: string | null;
}): Promise<void> {
  return invokeCmd<void>('sched_evidence_post_actions_set', args);
}

export function onSchedTaskEvent(
  handler: Parameters<typeof onEvent<SchedEvent>>[1],
): Promise<UnlistenFn> {
  return onEvent<SchedEvent>(SCHED_TASK_EVENT, handler);
}
