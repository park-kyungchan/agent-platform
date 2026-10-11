import { object } from './source-coverage.ts';
import { dataSnapshot } from './data-snapshot.ts';
export type Handoff = { verdict: 'PASS'|'FAIL'|'NO_VERDICT'; findings: string[]; limits: string[] };
export const codePoints = (value: string) => [...value].length;
// Python str.strip whitespace, matching the actual producer (not JS trim's BOM rule).
const hasNonWhitespace = /[^\u0009-\u000d\u001c-\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/u;
export const validSummary = (v: unknown): v is string => typeof v === 'string' && hasNonWhitespace.test(v) && codePoints(v) <= 6000;
// Capture own data once. Accessors, inherited indices and caller iterators are not input.
const ownData = (v: object, key: PropertyKey): unknown => {
  const descriptor = Object.getOwnPropertyDescriptor(v,key);
  return descriptor && Object.hasOwn(descriptor,'value') ? descriptor.value : undefined;
};
const texts = (v: unknown): string[] | null => {
  if (!Array.isArray(v)) return null;
  const length = ownData(v,'length');
  if (typeof length !== 'number' || !Number.isInteger(length) || length < 0 || length > 12) return null;
  const result: string[] = [];
  for (let i = 0; i < length; i++) {
    const item = ownData(v,String(i));
    if (typeof item !== 'string' || codePoints(item) > 1500) return null;
    result.push(item);
  }
  return result;
};
/** Required own fields; omitted/null is not an observed empty list. Native enrichment
 * is checked at the task/run binding, not silently reclassified as worker input. */
export function readHandoff(value: unknown, proposed = false): Handoff | null {
  try { value = dataSnapshot(value); } catch { return null; }
  if (!object(value) ||
      (proposed && Reflect.ownKeys(value).some(k => typeof k !== 'string' || !['verdict','findings','limits'].includes(k)))) return null;
  const verdict = ownData(value,'verdict');
  if (verdict !== 'PASS' && verdict !== 'FAIL' && verdict !== 'NO_VERDICT') return null;
  const findings = texts(ownData(value,'findings')), limits = texts(ownData(value,'limits'));
  if (findings === null || limits === null) return null;
  return {verdict,findings,limits};
}
