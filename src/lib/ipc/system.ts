import { invokeCmd } from './client';

export interface SystemPathInfo {
  exists: boolean;
  is_file: boolean;
  is_directory: boolean;
  is_symlink: boolean;
}

export function systemPathInfo(path: string): Promise<SystemPathInfo> {
  return invokeCmd<SystemPathInfo>('system_path_info', { path });
}
