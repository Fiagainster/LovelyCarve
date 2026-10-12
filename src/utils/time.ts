/** Normalize ISO-8601-ish timestamps coming back from the Rust backend
 *  ("2026-04-03T11:10:48+00:00", "2024-02-06T09:21:43.123Z") to a
 *  human-readable "YYYY-MM-DD HH:MM:SS" — the form the disk-image
 *  table and detail panel display. Returns "—" for null / empty so
 *  callers can pipe straight into the template. */
export function formatTimestamp(ts: string | null | undefined): string {
  if (!ts) return '—';
  const m = ts.match(/^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}:\d{2})/);
  if (m) return `${m[1]} ${m[2]}`;
  return ts;
}
