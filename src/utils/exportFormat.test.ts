import { describe, expect, it } from 'vitest';
import {
  buildJsonExport,
  isExportValueTruncated,
  previewExportValue,
  recordsToCsv,
  sanitizeExportFilename,
  stringifyExportValue,
  tsvEscape,
} from './exportFormat';

describe('exportFormat', () => {
  it('sanitizes filenames without dropping the extension stem', () => {
    expect(sanitizeExportFilename('HTTP: /admin/login?.json')).toBe('HTTP_admin_login_.json');
    expect(sanitizeExportFilename('   ', 'fallback')).toBe('fallback');
  });

  it('keeps full values available while producing bounded previews', () => {
    const value = { token: 'abcdef', nested: { count: 2 } };
    expect(stringifyExportValue(value)).toContain('"token": "abcdef"');
    expect(previewExportValue('0123456789', 6)).toBe('012...');
    expect(isExportValueTruncated('0123456789', 6)).toBe(true);
  });

  it('exports row records as quoted CSV', () => {
    expect(recordsToCsv([
      { id: 1, text: 'a,b' },
      { id: 2, text: 'line\nbreak' },
    ])).toBe('id,text\n1,"a,b"\n2,"line\nbreak"');
  });

  it('prefixes formula-leading cells so spreadsheets open them as text', () => {
    expect(recordsToCsv([
      { value: '=1+1' },
      { value: '+x' },
      { value: '-x' },
      { value: '@SUM(A1)' },
      { value: '\t=1+1' },
      { value: '\r=1+1' },
      { value: "=cmd|' /C calc'!A0" },
      { value: '=SUM(1,2)' },
      { value: '=HYPERLINK("http://x","y")' },
    ])).toBe([
      'value',
      "'=1+1",
      "'+x",
      "'-x",
      "'@SUM(A1)",
      "'\t=1+1",
      `"'\r=1+1"`,
      "'=cmd|' /C calc'!A0",
      `"'=SUM(1,2)"`,
      `"'=HYPERLINK(""http://x"",""y"")"`,
    ].join('\n'));
  });

  it('keeps plain negative numbers numeric and escapes record-derived headers', () => {
    expect(recordsToCsv([
      { '=label': 'x', offset: -12, ratio: '-3.5', expr: '-1+2', partial: '-3.' },
    ])).toBe("'=label,offset,ratio,expr,partial\nx,-12,-3.5,'-1+2,'-3.");
  });

  it('prefixes formula-leading TSV cells with the CSV rule', () => {
    expect(tsvEscape('=1+1')).toBe("'=1+1");
    expect(tsvEscape('+x')).toBe("'+x");
    expect(tsvEscape('-x')).toBe("'-x");
    expect(tsvEscape('@SUM(A1)')).toBe("'@SUM(A1)");
    expect(tsvEscape('\t=1+1')).toBe("'␉=1+1");
    expect(tsvEscape('\r=1+1')).toBe("'␍=1+1");
    expect(tsvEscape('"=1+1"')).toBe(`'"=1+1"`);
  });

  it('shows embedded TSV delimiters as control pictures so they cannot open a cell or row', () => {
    expect(tsvEscape('a\t=HYPERLINK("http://x","y")')).toBe('a␉=HYPERLINK("http://x","y")');
    expect(tsvEscape('a\n=1+1')).toBe('a␊=1+1');
    expect(tsvEscape('a\r\n=1+1')).toBe('a␍␊=1+1');
    expect(tsvEscape('\n=1+1')).toBe('␊=1+1');
    expect(tsvEscape('a\u0000b\u000bc\u007f')).toBe('a␀b␋c␡');
    expect(['/x/a\t=1+1', '文件', '-12'].map(tsvEscape).join('\t')).toBe('/x/a␉=1+1\t文件\t-12');
  });

  it('keeps plain negative numbers and ordinary TSV cells unchanged', () => {
    expect(tsvEscape('-12')).toBe('-12');
    expect(tsvEscape('-3.5')).toBe('-3.5');
    expect(tsvEscape('-1+2')).toBe("'-1+2");
    expect(tsvEscape('-3.')).toBe("'-3.");
    expect(tsvEscape('/home/user/say "hi".txt')).toBe('/home/user/say "hi".txt');
    expect(tsvEscape('')).toBe('');
  });

  it('wraps JSON exports with stable metadata', () => {
    const parsed = JSON.parse(buildJsonExport('Current Module', { rows: [1] }, { tab: 'dns' }));
    expect(parsed.title).toBe('Current Module');
    expect(parsed.data.rows).toEqual([1]);
    expect(parsed.meta.tab).toBe('dns');
    expect(typeof parsed.exported_at).toBe('string');
  });
});
