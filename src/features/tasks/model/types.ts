import type { SchedTaskStatus, SchedTaskTrigger } from '../../../lib/ipc/tasks';



export type TaskHubTab = 'tasks' | 'quick';


export interface BuiltinTask {
  id: string;
  kind: 'docker' | 'search' | 'timeline' | 'carve' | 'filesystem' | 'index' | 'autodetect' | 'encryptedUnlock';
  imageId: string;
  title: string;
  subject: string;
  /** Platform detection also runs on its own when evidence first joins a case. */
  trigger: SchedTaskTrigger;
  status: SchedTaskStatus;
  progress: number | null;
  message: string;
  error: string | null;
  started: number;
  finished: number | null;
  cancelId: string | null;
  cancelling: boolean;
  results: number;
  timelineRows: number;
}
