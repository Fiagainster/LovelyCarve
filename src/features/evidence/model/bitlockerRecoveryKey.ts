/**
 * BitLocker recovery passwords are 48 digits in eight dash-separated groups of
 * six. Every group is divisible by 11 and its quotient fits in 16 bits — the
 * rules the vendored decryptor (`recovery_key_hash`) enforces — so a typo can
 * be pointed out in the field instead of after an unlock round trip.
 */

const GROUP_COUNT = 8;
const GROUP_DIGITS = 6;
/** 65535 × 11: the largest group whose quotient still fits in 16 bits. */
const MAX_GROUP_VALUE = 720_885;

export interface RecoveryKeyCheck {
  /** Canonical `xxxxxx-…-xxxxxx` form to send to the backend. */
  normalized: string;
  /** Complete groups that pass the checksum. */
  validGroups: number;
  /** A finished group that can never be right (letters, checksum, range,
   *  extra group): worth showing while the user is still typing. */
  groupError: string | null;
  /** What is still missing (a short group, too few groups): show once the
   *  user submits or leaves the field. */
  formatError: string | null;
}

/** Pasted keys often arrive with spaces, en dashes or no separators at all.
 *  A run of digits without any separator is grouped six at a time. */
export function normalizeRecoveryKey(raw: string): string {
  const joined = raw.trim().replace(/[^0-9A-Za-z]+/g, '-').replace(/^-+|-+$/g, '');
  return /^\d+$/.test(joined) ? joined.replace(/(\d{6})(?=\d)/g, '$1-') : joined;
}

export function checkRecoveryKey(raw: string): RecoveryKeyCheck {
  const normalized = normalizeRecoveryKey(raw);
  const groups = normalized ? normalized.split('-') : [];
  let validGroups = 0;
  let groupError: string | null = null;
  let formatError: string | null = null;
  for (const [index, group] of groups.entries()) {
    const position = index + 1;
    if (!/^\d+$/.test(group)) {
      groupError = `只能输入数字，第 ${position} 组含有其他字符`;
      break;
    }
    if (index >= GROUP_COUNT) {
      groupError = `恢复密钥只有 ${GROUP_COUNT} 组，请删除多出的第 ${position} 组`;
      break;
    }
    if (group.length > GROUP_DIGITS) {
      groupError = `第 ${position} 组超过 ${GROUP_DIGITS} 位，请检查分隔`;
      break;
    }
    if (group.length < GROUP_DIGITS) {
      formatError ??= `第 ${position} 组只有 ${group.length} 位，应为 ${GROUP_DIGITS} 位`;
      continue;
    }
    const value = Number(group);
    if (value % 11 !== 0) {
      groupError = `第 ${position} 组「${group}」校验失败（应能被 11 整除），请检查是否输错`;
      break;
    }
    if (value > MAX_GROUP_VALUE) {
      groupError = `第 ${position} 组「${group}」超出有效范围（不大于 ${MAX_GROUP_VALUE}）`;
      break;
    }
    validGroups += 1;
  }
  if (groupError === null && formatError === null && groups.length < GROUP_COUNT) {
    formatError = `还差 ${GROUP_COUNT - groups.length} 组（共 ${GROUP_COUNT} 组 ${GROUP_DIGITS} 位数字）`;
  }
  return { normalized, validGroups, groupError, formatError };
}
