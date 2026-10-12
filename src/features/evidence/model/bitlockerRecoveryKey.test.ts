import { describe, expect, it } from 'vitest';
import { checkRecoveryKey, normalizeRecoveryKey } from './bitlockerRecoveryKey';

// Recovery password of the vendored m8005 oracle image.
const KEY = '031174-056914-397793-502348-055847-196306-284306-262174';

describe('normalizeRecoveryKey', () => {
  it('turns spaces and other separators into single dashes', () => {
    expect(normalizeRecoveryKey(' 031174 056914–397793  502348_055847 196306 284306 262174 ')).toBe(KEY);
  });

  it('groups a separator-free run of digits six at a time', () => {
    expect(normalizeRecoveryKey(KEY.replaceAll('-', ''))).toBe(KEY);
    expect(normalizeRecoveryKey('03117405691')).toBe('031174-05691');
  });

  it('keeps typed groups as they are once separators are present', () => {
    expect(normalizeRecoveryKey('0311740-56914')).toBe('0311740-56914');
  });
});

describe('checkRecoveryKey', () => {
  it('accepts a complete key', () => {
    expect(checkRecoveryKey(KEY)).toEqual({ normalized: KEY, validGroups: 8, groupError: null, formatError: null });
  });

  it('reports what is still missing without flagging it as wrong', () => {
    expect(checkRecoveryKey('')).toEqual({ normalized: '', validGroups: 0, groupError: null, formatError: '还差 8 组（共 8 组 6 位数字）' });
    expect(checkRecoveryKey('031174-0569')).toEqual({
      normalized: '031174-0569',
      validGroups: 1,
      groupError: null,
      formatError: '第 2 组只有 4 位，应为 6 位',
    });
    expect(checkRecoveryKey('031174-056914').formatError).toBe('还差 6 组（共 8 组 6 位数字）');
  });

  it('flags a finished group that fails the divisible-by-11 checksum', () => {
    expect(checkRecoveryKey('031174-056914-397794')).toEqual({
      normalized: '031174-056914-397794',
      validGroups: 2,
      groupError: '第 3 组「397794」校验失败（应能被 11 整除），请检查是否输错',
      formatError: null,
    });
  });

  it('flags a group whose quotient does not fit in 16 bits', () => {
    // 65536 × 11: divisible by 11 but one past the largest valid group.
    expect(checkRecoveryKey('720896').groupError).toBe('第 1 组「720896」超出有效范围（不大于 720885）');
  });

  it('flags letters, overlong groups and extra groups', () => {
    expect(checkRecoveryKey('03117a').groupError).toBe('只能输入数字，第 1 组含有其他字符');
    expect(checkRecoveryKey('0311740-056914').groupError).toBe('第 1 组超过 6 位，请检查分隔');
    expect(checkRecoveryKey(`${KEY}-031174`).groupError).toBe('恢复密钥只有 8 组，请删除多出的第 9 组');
  });
});
