


export interface RaidMemberQueueItem {
  path: string;
  id: string | null;
  sizeBytes: number | null;
  status: 'loading' | 'loaded' | 'failed';
  message: string | null;
  preExisting: boolean;
  isMember: boolean;
}
