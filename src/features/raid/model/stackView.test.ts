import { describe, expect, it } from 'vitest';
import type { RaidArray, RaidArrayMember, RaidStack } from '../../../lib/ipc/raid';
import { arrayStatus, diskLabelsFor, exportBaseName, missingSlotFor, orderedMembers, relevantRaidArrays, relevantRaidVolumeGroups } from './stackView';

const member: RaidArrayMember = { dev_number: 0, slot: 0, device_uuid: 'member-a', data_size_sectors: 1000, reconstructed: false, origin: { kind: 'whole', image_id: 'a', partition_id: 1, byte_offset: 0, byte_length: 512000 } };
const array: RaidArray = { set_uuid: 'array-a', set_name: 'array/a', level: 5, layout: 0, chunksize_sectors: 128, raid_disks: 3, data_offset_sectors: 0, assembled_bytes: 1024000, members: [member], assembled_image_id: null, heuristic: false };
const stack: RaidStack = { images: [], volume_groups: [{ vg_uuid: 'vg-a', vg_name: 'vg', extent_size_sectors: 10, members: [{ image_id: 'a', partition_id: 1, pv_local_id: 'pv-a', pv_uuid: 'pv', pe_start_sectors: 0, partition_byte_offset: 0, partition_byte_length: 512000 }], logical_volumes: [] }], arrays: [array, { ...array, set_uuid: 'lv-array', members: [{ ...member, origin: { kind: 'lv', vg_uuid: 'vg-a', vg_name: 'vg', lv_name: 'lv', size_bytes: 512000 } }] }, { ...array, set_uuid: 'assembled', assembled_image_id: 'synthetic', members: [] }], filesystems: [], diagnostics: [] };

describe('RAID view projections', () => {
  it('includes direct members, PV-backed LV arrays and the assembled image itself', () => {
    expect(relevantRaidArrays(stack, 'a').map(item => item.set_uuid)).toEqual(['array-a', 'lv-array']);
    expect(relevantRaidArrays(stack, 'synthetic').map(item => item.set_uuid)).toEqual(['assembled']);
    expect(relevantRaidArrays(stack, 'unrelated')).toEqual([]); expect(relevantRaidVolumeGroups(stack, 'a')).toEqual(stack.volume_groups);
  });
  it('orders only assigned columns and keeps reconstructed and missing slots in the diagram', () => {
    const reconstructed = { ...member, device_uuid: 'rebuilt', slot: 2, reconstructed: true };
    const arranged = { ...array, members: [reconstructed, { ...member, device_uuid: 'spare', slot: null }, member] };
    expect(orderedMembers(arranged)).toEqual([member, reconstructed]); expect(missingSlotFor(arranged)).toBe(2);
    expect(diskLabelsFor(arranged, new Map([['a', 'D:/member.dd']]))).toEqual(['member.dd', null, 'member.dd']);
  });
  it('distinguishes assembled, incomplete, unsupported and failed geometry', () => {
    expect(arrayStatus(array)).toEqual({ label: '成员不全 1/3', sev: 'warn' });
    expect(arrayStatus({ ...array, assembled_image_id: 'assembled' })).toEqual({ label: '已重组', sev: 'success' });
    expect(arrayStatus({ ...array, raid_disks: 1, level: 1 })).toEqual({ label: 'RAID1 暂不支持重组', sev: 'info' });
    expect(arrayStatus({ ...array, raid_disks: 1, layout: 4 })).toEqual({ label: 'RAID5 布局 4 暂不支持重组', sev: 'info' });
    expect(arrayStatus({ ...array, raid_disks: 1 })).toEqual({ label: '未能重组，见诊断', sev: 'warn' });
    expect(exportBaseName(array)).toBe('array_a_RAID5');
  });
});
