export interface JsonExportEnvelope {
  exported_at: string;
  title: string;
  data: unknown;
  meta?: Record<string, unknown>;
}

const FILENAME_REPLACEMENT = '_';

export function sanitizeExportFilename(value: string, fallback = 'export', maxLength = 120): string {
  const safe = value
    .trim()
    .replace(/[\\/:*?"<>|]+/g, FILENAME_REPLACEMENT)
    .replace(/\s+/g, FILENAME_REPLACEMENT)
    .replace(/_+/g, FILENAME_REPLACEMENT)
    .replace(/^_+|_+$/g, '')
    .slice(0, maxLength);
  return safe || fallback;
}

export function stringifyExportValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint') {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function previewExportValue(value: unknown, maxLength = 160): string {
  const text = stringifyExportValue(value);
  if (maxLength <= 0 || text.length <= maxLength) return text;
  return `${text.slice(0, Math.max(0, maxLength - 3))}...`;
}

export function isExportValueTruncated(value: unknown, maxLength = 160): boolean {
  return maxLength > 0 && stringifyExportValue(value).length > maxLength;
}

export function buildJsonExport(title: string, data: unknown, meta?: Record<string, unknown>): string {
  const envelope: JsonExportEnvelope = {
    exported_at: new Date().toISOString(),
    title,
    data,
  };
  if (meta) envelope.meta = meta;
  return JSON.stringify(envelope, null, 2);
}

export function recordsToCsv(rows: Array<Record<string, unknown>>, columns?: string[]): string {
  const resolvedColumns = columns && columns.length > 0
    ? columns
    : Array.from(rows.reduce((set, row) => {
      for (const key of Object.keys(row)) set.add(key);
      return set;
    }, new Set<string>()));

  const lines = [resolvedColumns.map(csvEscape).join(',')];
  for (const row of rows) {
    lines.push(resolvedColumns.map(column => csvEscape(stringifyExportValue(row[column]))).join(','));
  }
  return lines.join('\n');
}

// 检材取值（文件名、路径、注册表/插件值）不可信：以 = + - @、制表符或回车开头的单元格会被
// Excel / WPS / LibreOffice 当作公式执行（=HYPERLINK(...)、DDE），按 OWASP CSV 注入建议前置 '
// 使其按文本打开。-12、-3.5 这类纯负数只会被读成数值、不会执行，保持原样以免数值列变成文本。
// 规则与后端 commands::quote_csv_field 一致。
const FORMULA_LEAD = /^[-=+@\t\r]/;
const PLAIN_NEGATIVE_NUMBER = /^-\d+(\.\d+)?$/;

function neutralizeFormula(value: string): string {
  return FORMULA_LEAD.test(value) && !PLAIN_NEGATIVE_NUMBER.test(value) ? `'${value}` : value;
}

function csvEscape(value: string): string {
  const cell = neutralizeFormula(value);
  if (/[",\r\n]/.test(cell)) {
    return `"${cell.replace(/"/g, '""')}"`;
  }
  return cell;
}

// 剪贴板上的制表符分隔文本（粘进 Excel / WPS 或笔录）沿用同一公式规则，但内嵌的制表符、回车、换行
// 不像 CSV 那样加引号，而是连同其余控制字符换成可见的控制符图形 ␉ ␍ ␊ 等：引号只对按 " 识别文本的
// 表格软件有效，Word、记事本等笔录载体不认，换行照样拆出新行，拆出的行首还绕过了公式规则。替换后
// 粘到哪里都是一行一条记录、列数固定，文件名里藏的控制字符也一眼可见。开头的 " 同样前置 '，否则
// Excel 会把 "=1+1" 当作引号字段，去掉引号后仍按公式执行。
const CONTROL_CHAR = /[\u0000-\u001f\u007f]/g;

export function tsvEscape(value: string): string {
  const cell = value.startsWith('"') ? `'${value}` : neutralizeFormula(value);
  return cell.replace(CONTROL_CHAR, char => {
    const code = char.charCodeAt(0);
    return String.fromCharCode(code === 0x7f ? 0x2421 : 0x2400 + code);
  });
}
