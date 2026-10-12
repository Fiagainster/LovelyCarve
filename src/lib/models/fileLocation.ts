


export type FileLocationTarget = {
  imageId: string;
  offset?: number;
  length?: number;
} & ({ nodeId: string; rootId?: never; path?: never } | { rootId: string; path: string; nodeId?: never });
