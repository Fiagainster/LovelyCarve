


export const FILE_TAGS = ['重点', '可疑', '已排除'] as const;
export type FileTag = (typeof FILE_TAGS)[number];

/** 标签 → 样式键（CSS class 后缀，避免类名里出现中文）。 */
export const TAG_STYLE: Record<FileTag, 'crit' | 'warn' | 'ok'> = {
  重点: 'crit',
  可疑: 'warn',
  已排除: 'ok',
};
