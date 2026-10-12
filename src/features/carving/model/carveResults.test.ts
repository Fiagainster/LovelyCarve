import { describe, expect, it } from 'vitest';

import { mergeCarveArtifacts } from './carveResults';
import type { CarveArtifact } from '../../../lib/ipc/carve';

function artifact(id: string, offset: number): CarveArtifact {
  return {
    id,
    name: `f${offset}.png`,
    format_id: 'png',
    extension: 'png',
    category: 'image',
    offset,
    length: 20,
    confidence: 99,
    region: '整盘',
    read_fault: false,
  };
}

describe('mergeCarveArtifacts', () => {
  it('deduplicates overlapping pages and keeps image-offset order', () => {
    expect(mergeCarveArtifacts(
      [artifact('two', 20), artifact('one', 10)],
      [artifact('two', 20), artifact('three', 30)],
    ).map(item => item.id)).toEqual(['one', 'two', 'three']);
  });
});
