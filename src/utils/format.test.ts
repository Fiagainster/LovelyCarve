import { describe, expect, it } from 'vitest';
import { formatBinaryBytes, formatCaptureTimestamp, formatEpochSeconds } from './format';

describe('formatBinaryBytes', () => {
  it.each([
    [0, '0 B'], [1023, '1023 B'], [1024, '1.00 KiB'],
    [9437184, '9.00 MiB'], [32183943168, '29.97 GiB'], [1024 ** 4, '1.00 TiB'],
  ])('labels %i bytes with IEC units', (bytes, label) => {
    expect(formatBinaryBytes(bytes)).toBe(label);
  });
});

describe('formatEpochSeconds', () => {
  it('formats UTC epoch seconds with millisecond precision', () => {
    expect(formatEpochSeconds(1_700_000_000.123, {
      timezone: 'utc',
      zeroAsMissing: false,
    })).toBe('2023-11-14 22:13:20.123');
  });

  it('supports second precision', () => {
    expect(formatEpochSeconds(1_700_000_000.123, {
      timezone: 'utc',
      precision: 'seconds',
      zeroAsMissing: false,
    })).toBe('2023-11-14 22:13:20');
  });

  it('treats zero as missing by default for capture timestamps', () => {
    expect(formatEpochSeconds(0)).toBe('-');
    expect(formatEpochSeconds(0, {
      timezone: 'utc',
      zeroAsMissing: false,
    })).toBe('1970-01-01 00:00:00.000');
  });

  it('formats UTC+8 without relying on host timezone', () => {
    expect(formatEpochSeconds(1_700_000_000, {
      timezone: 'utc8',
      suffix: true,
      zeroAsMissing: false,
    })).toBe('2023-11-15 06:13:20.000 +08');
  });
});

describe('formatCaptureTimestamp', () => {
  it('keeps relative capture seconds in a stable seconds format', () => {
    expect(formatCaptureTimestamp(12.3456789)).toBe('12.345679s');
    expect(formatCaptureTimestamp(0)).toBe('0.000000s');
  });

  it('formats epoch capture seconds with the shared epoch formatter', () => {
    expect(formatCaptureTimestamp(1_700_000_000.123, {
      timezone: 'utc',
    })).toBe('2023-11-14 22:13:20.123');
  });

  it('uses the configured fallback for missing values', () => {
    expect(formatCaptureTimestamp(Number.NaN)).toBe('-');
    expect(formatCaptureTimestamp(undefined, { fallback: 'n/a' })).toBe('n/a');
  });
});
