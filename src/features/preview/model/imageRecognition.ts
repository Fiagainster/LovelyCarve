import type { OcrOutcome } from '../../../lib/ipc/ocr';
import type { QrScanOutcome } from '../../../lib/ipc/qrcode';

/** Geometry and read-only display projection for the image recognition surface. */
export interface OcrTextItem {
  text: string; minX: number; minY: number; w: number; h: number;
}
export interface ImageRecognitionView {
  imageNatural: { w: number; h: number } | null;
  qrScanning: boolean;
  qrResult: QrScanOutcome | null;
  qrError: string | null;
  qrProgress: { attempts: number; total: number; found: number } | null;
  qrCopiedIndex: number | null;
  ocrScanning: boolean;
  ocrResult: OcrOutcome | null;
  ocrError: string | null;
  ocrCopiedAll: boolean;
  ocrFullText: string;
  ocrTextItems: { text: string; style: { left: string; top: string; fontSize: string } }[];
}
export function svgPoints(points: [number, number][]): string {
  return points.map(point => `${point[0]},${point[1]}`).join(' ');
}
