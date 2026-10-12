


export interface FileListNavigator {
  imageId: string;
  openFileByNodeId: (nodeId: string, offset?: number, length?: number) => Promise<void>;
  openFileBySource: (rootId: string, path: string) => Promise<void>;
}

/** A directory breadcrumb also remembers the position to restore on return. */
export interface FileNavigationFrame {
  id: string;
  name: string;
  scrollTop?: number;
}

export interface FileScrollRestore {
  scrollTop: number;
  childId: string | null;
  openDetail?: boolean;
}

export interface FileByteFocus {
  nodeId: string;
  offset: number;
  length: number;
}
