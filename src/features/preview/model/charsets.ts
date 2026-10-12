export interface CharsetOption {
  id: string;
  label: string;
  decoder: string;
  singleByte: boolean;
}

export const CHARSETS: CharsetOption[] = [
  { id: 'ascii', label: 'ASCII', decoder: 'ascii', singleByte: true },
  { id: 'ansi', label: 'ANSI (1252)', decoder: 'windows-1252', singleByte: true },
  { id: 'latin1', label: 'Latin-1', decoder: 'iso-8859-1', singleByte: true },
  { id: 'utf-8', label: 'UTF-8', decoder: 'utf-8', singleByte: false },
  { id: 'utf-16le', label: 'UTF-16 LE', decoder: 'utf-16le', singleByte: false },
  { id: 'utf-16be', label: 'UTF-16 BE', decoder: 'utf-16be', singleByte: false },
  { id: 'gbk', label: 'GBK', decoder: 'gbk', singleByte: false },
  { id: 'gb18030', label: 'GB18030', decoder: 'gb18030', singleByte: false },
  { id: 'big5', label: 'Big5', decoder: 'big5', singleByte: false },
  { id: 'shift-jis', label: 'Shift_JIS', decoder: 'shift_jis', singleByte: false },
  { id: 'euc-jp', label: 'EUC-JP', decoder: 'euc-jp', singleByte: false },
  { id: 'euc-kr', label: 'EUC-KR', decoder: 'euc-kr', singleByte: false },
  { id: 'koi8-r', label: 'KOI8-R', decoder: 'koi8-r', singleByte: true },
  { id: 'ibm866', label: 'IBM866', decoder: 'ibm866', singleByte: true },
  { id: 'macintosh', label: 'Mac Roman', decoder: 'macintosh', singleByte: true },
];
