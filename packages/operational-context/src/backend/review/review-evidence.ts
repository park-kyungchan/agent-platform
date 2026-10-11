import { SourceCoverage, object, parse, nonempty, type ReviewBinding } from './source-coverage.ts';
import { reviewTrace } from './native-trace.ts';
import { dataSnapshot } from './data-snapshot.ts';
export type { ReviewBinding } from './source-coverage.ts';

/** Legacy CLI-review API. Shared transport/exposure, separate domain verdict.
 * Pure evidence, not execution permission, semantic acceptance or an OS sandbox.
 * Unknown native result annotations fail closed; they are never cut away. */
export function assessReview(packetText: string, expected: ReviewBinding, observation: any) {
  const issues = new Set<string>();
  const check = (condition: any, code: string) => { if (!condition) issues.add(code); };
  let coverage: { documentId: string; coveredChars: number; totalChars: number }[] = [];
  let pageReads = 0, distinctToolCalls = 0;
  let nativeFinalMessageId: number | null = null, verdict: string | null = null, reviewId: string | null = null;
  try {
    expected = dataSnapshot(expected); observation = dataSnapshot(observation);
    const source = new SourceCoverage(packetText, expected, check); reviewId = source.reviewId;
    const trace = reviewTrace(observation, expected, 'ocp_review_read', [], check);
    distinctToolCalls = trace.calls.size;
    for (const [key, call] of trace.calls) {
      const row = trace.results.get(key);
      if (call.name === 'ocp_review_read' && row) source.read(call.args, parse(row.content));
    }
    coverage = source.finish(); pageReads = source.pageReads;
    const final = trace.messages.at(-1);
    if (final.role !== 'assistant' || (parse(final.tool_calls) ?? []).length ||
        typeof final.content !== 'string' || final.content.length > 16000) throw new Error('final');
    const output = JSON.parse(final.content.trim().replace(/^```json\s*\n([\s\S]*?)\n```$/, '$1'));
    check(object(output) && output.review_id === reviewId && ['pass','revise','reject'].includes(output.verdict) &&
      Array.isArray(output.findings) && output.findings.every((f: any) => object(f) &&
        ['high','medium','low'].includes(f.severity) && ['location','issue','reproduce','recommendation'].every(k => nonempty(f[k]))) &&
      Array.isArray(output.limits) && output.limits.every((v: any) => typeof v === 'string'), 'FINAL_OUTPUT_INCOMPLETE');
    nativeFinalMessageId = final.id; verdict = typeof output.verdict === 'string' ? output.verdict : null;
  } catch { issues.add('INVALID_EVIDENCE'); }
  const binding = issues.size === 0 && reviewId !== null ? { sessionId: expected.sessionId, reviewId,
    packetSha256: expected.packetSha256, requestSha256: expected.requestSha256, model: expected.model, provider: expected.provider } : null;
  return { status: issues.size ? 'INSUFFICIENT' : 'COMPLETE_READER_EVIDENCE', issues: [...issues].sort(),
    nativeFinalMessageId, verdict, distinctToolCalls, pageReads, coverage, binding };
}
