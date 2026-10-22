import { test, expect } from "bun:test";
import { rankMemory, rankLiteralMemory } from "./ranking.ts";

test("literal algorithm stays independently callable and is not relabeled as a semantic baseline", () => {
  const texts = ["한국어로 짧게 설명한다", "다른 작업"];
  expect(rankLiteralMemory(texts, "설명 언어는 어떻게 정하나요").indices).toEqual([]);
  expect(rankMemory(texts, "설명 언어는 어떻게 정하나요").indices).toEqual([0]);
});
test("unseen Korean inflections, technical aliases and exact foreign identifiers use permitted texts only", () => {
  const texts = [
    "API_Z7 keeps the dependency version pinned",
    "여러 작성자의 동시 변경에는 mutex를 사용한다",
    "다른 unrelated sentence",
  ];
  expect(rankMemory(texts, "개발 도구 버전 고정").indices[0]).toBe(0);
  expect(rankMemory(texts, "동시에 쓰는 공용 자료의 락").indices[0]).toBe(1);
  expect(rankMemory(texts, "API_Z7").indices).toEqual([0]);
  expect(rankMemory(texts, "unknown_Q9").indices).toEqual([]);
});
test("narrow missing measurement/calibration/retention does not turn nearby procedures into facts", () => {
  const texts = [
    "복원 방법은 체크섬 대조",
    "성능 표는 millisecond 단위를 쓴다",
    "보관할 로그는 7일 retention",
  ];
  expect(rankMemory(texts, "복원 benchmark 실측 수치").indices).toEqual([]);
  expect(rankMemory(texts, "레이더 좌표계 보정").indices).toEqual([]);
  expect(rankMemory(texts, "로그 보관 기간").indices).toEqual([2]);
});
test("ranking is a lexical relevance calculation with no source identity or authority input", () => {
  const texts = ["언어 개인 설정", "언어 다른 Work", "언어 첫 프로젝트"];
  const ranked = rankMemory(texts, "언어");
  expect(ranked.indices).toEqual([0, 1, 2]);
  expect(new Set(ranked.scores.values()).size).toBe(1);
  expect(ranked.disposed).toBe(true);
});
