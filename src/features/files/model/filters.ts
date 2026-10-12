/** Pure filter input/applied-state contracts shared by query builders and controls. */
export type SizeUnit = 'B' | 'KB' | 'MB' | 'GB' | 'TB';

export const SIZE_UNITS: SizeUnit[] = ['B', 'KB', 'MB', 'GB', 'TB'];

export interface SizeFilterState {
  active: boolean;
  leEnabled: boolean;
  leValue: string;
  leUnit: SizeUnit;
  geEnabled: boolean;
  geValue: string;
  geUnit: SizeUnit;
  modEnabled: boolean;
  modValue: string;
  modRemainder: string;
  appliedLe: number | null;
  appliedGe: number | null;
  appliedMod: { n: number; r: number } | null;
}

export const TIME_COLUMNS = ['created', 'accessed', 'modified'] as const;
export type TimeColumnKey = (typeof TIME_COLUMNS)[number];
export type TimeFilterEdge = 'from' | 'to';

export interface TimeFilterState {
  active: boolean;
  fromEnabled: boolean;
  fromValue: string;
  toEnabled: boolean;
  toValue: string;
  appliedFrom: string | null;
  appliedTo: string | null;
}


/** Above this count, client filter/sort updates first yield visible feedback. */
export const LARGE_FILE_LIST = 100_000;
