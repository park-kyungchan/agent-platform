import { expect, test } from 'bun:test';
import { validateIndependentReviewReceipt } from '../ci/source-check.ts';
const head = 'a'.repeat(40), tree = 'b'.repeat(40);
const receipt = { schema: 'native-independent-review.v1', headSha: head, reviewedTreeSha: tree,
  author: { taskId: 'synthetic-author' }, reviewer: { runtime: 'native-codex', taskId: 'synthetic-independent-reviewer' },
  conclusion: 'pass', unresolvedFindings: 0, provenance: { owner: 'hermes-harness-engineering', resultRef: 'synthetic-owner-result', sha256: 'c'.repeat(64) } };
test('native review receipt binds exact head and tree independently of receipt commit', () => {
  expect(() => validateIndependentReviewReceipt(receipt, head, tree)).not.toThrow();
  for (const patch of [{ headSha: 'd'.repeat(40) }, { reviewedTreeSha: 'e'.repeat(40) }, { unresolvedFindings: 1 }, { conclusion: 'fail' }, { reviewer: { runtime: 'native-codex', taskId: receipt.author.taskId } }, { reviewer: { runtime: 'assistant-summary', taskId: 'another' } }, { provenance: { owner: 'other', resultRef: 'synthetic', sha256: 'c'.repeat(64) } }]) {
    expect(() => validateIndependentReviewReceipt({ ...receipt, ...patch }, head, tree)).toThrow();
  }
});
test('missing exact commit tree and missing provenance cannot satisfy review', () => {
  expect(() => validateIndependentReviewReceipt({ ...receipt, reviewedTreeSha: undefined }, head, undefined as unknown as string)).toThrow();
  expect(() => validateIndependentReviewReceipt({ ...receipt, provenance: undefined }, head, tree)).toThrow();
});
