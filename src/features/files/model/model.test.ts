import { describe, expect, it } from 'vitest';
import {
  isHostDockerContainerRootId,
  isHostDockerMountRoot,
  type DiskFsNode,
} from './model';

function node(id: string, kind: DiskFsNode['kind'] = 'dir'): DiskFsNode {
  return {
    id,
    name: id,
    parent_id: null,
    kind,
    size: 0,
    deleted: false,
    inode: null,
    timestamps: {
      modified: null,
      accessed: null,
      created: null,
      mft_changed: null,
      birth: null,
    },
  };
}

describe('isHostDockerMountRoot', () => {
  it('accepts the Docker group and individual container roots', () => {
    expect(isHostDockerMountRoot(node('host-docker:'))).toBe(true);
    expect(isHostDockerMountRoot(node('host-docker:14d219:'))).toBe(true);
  });

  it('rejects container descendants, regular volumes, and non-directories', () => {
    expect(isHostDockerMountRoot(node('host-docker:14d219:2f657463'))).toBe(false);
    expect(isHostDockerMountRoot(node('part:0'))).toBe(false);
    expect(isHostDockerMountRoot(node('host-docker:', 'file'))).toBe(false);
    expect(isHostDockerMountRoot(null)).toBe(false);
  });
});

describe('isHostDockerContainerRootId', () => {
  it('matches only an individual container root', () => {
    expect(isHostDockerContainerRootId('host-docker:14d219:')).toBe(true);
    expect(isHostDockerContainerRootId('host-docker:')).toBe(false);
    expect(isHostDockerContainerRootId('host-docker:14d219:2f657463')).toBe(false);
    expect(isHostDockerContainerRootId('part:0')).toBe(false);
  });
});
