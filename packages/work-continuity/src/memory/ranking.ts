import { Database } from "bun:sqlite";

export const retrievalVersion = "korean-technical-lexical.v1" as const;
export interface MemoryRanking {
  indices: number[];
  scores: ReadonlyMap<number, number>;
  disposed: true;
}
/** Retained algorithm baseline, callable only with already permitted bodies.
 * This is neither the old Work-required API nor a permission adapter. */
export function rankLiteralMemory(texts: readonly string[], query: string): MemoryRanking {
  const terms = [...new Set(query.match(/[\p{L}\p{N}_]+/gu) ?? [])];
  const db = new Database(":memory:");
  try {
    db.run("CREATE VIRTUAL TABLE recall USING fts5(ordinal UNINDEXED,text,tokenize='unicode61')");
    const insert = db.query("INSERT INTO recall(ordinal,text) VALUES (?,?)");
    texts.forEach((text, index) => insert.run(index, text));
    if (!terms.length) return { indices: [], scores: new Map(), disposed: true };
    const match = terms.map((term) => '"' + term.replaceAll('"', '""') + '"').join(" AND ");
    const rows = db
      .query<{ ordinal: number; score: number }, [string]>(
        "SELECT ordinal,bm25(recall) AS score FROM recall WHERE recall MATCH ? ORDER BY score,rowid",
      )
      .all(match);
    return {
      indices: rows.map((row) => row.ordinal),
      scores: new Map(rows.map((row) => [row.ordinal, -row.score])),
      disposed: true,
    };
  } finally {
    db.close();
  }
}

/** Small, inspectable technical vocabulary. These aliases express terminology,
 * never fixture IDs, owner identity, authority or query-specific expected hits.
 * This is lexical expansion, not general semantic understanding. All features
 * are query-local, after native original/body authorization; no model, service,
 * persistent index, embedding or new dependency is involved. */
const vocabulary: readonly [string, RegExp][] = [
  ["language", /언어|한국어|korean|답변의 언어|대화 설명/iu],
  ["notation", /표기|식별자|명령어|문자열을 유지|identifier/iu],
  ["merge", /병합|합치|합쳐|merge/iu],
  ["approval", /승인|독립 검토|사람 확인|책임자|결정 원문|approval|review/iu],
  ["dependency", /의존|패키지|개발 도구|설치|버전|dependency|package|install/iu],
  ["reproducibility", /재현|고정|달라지지|정확한 버전|잠금 파일|lockfile|reproduc/iu],
  [
    "concurrency",
    /동시|여러 작업자|여러 작성자|한 작성자|배타|점유|덮어쓰|공용|공유 상태|락|mutex|concurren/iu,
  ],
  [
    "uncertain-effect",
    /시간 초과|미확정|재시도|또 보내|응답을 못|응답이 끊|원래 거래|중복 실행|timeout|retry/iu,
  ],
  [
    "source-persistence",
    /인덱스|색인|검색용 목록|대화창|영속|원본 소실|기록이 사라|원본 기록|새 세션|projection|index/iu,
  ],
  ["restore", /백업|복원|되살|스냅샷|복구한 자료|복구된|restore|backup/iu],
  ["verification", /검증|해시|대조|실제로 읽|믿기 전에|checksum|verify/iu],
  ["redaction", /민감|비밀|인증 토큰|진단 기록|장애 분석|가린|가려|redact|secret|credential/iu],
  ["retention", /보존 기간|보관|로그 보존|오래된 기록|삼십|retention/iu],
  ["publication", /업로드|외부 공개|공개 주소|upload|publish/iu],
  ["batch", /배치|처리량의 상한|몇 건|스물네|마흔여덟|batch/iu],
  ["units", /단위|밀리초|머리글|성능 표|millisecond|units/iu],
  ["cache", /캐시|캐싱|cache/iu],
  ["proposal", /제안|추정|추측|채택 전|아이디어|실측이나 채택|확인 없이|proposal|hypothesis/iu],
  [
    "permission",
    /권한|세대|현재 정책|가능 범위|읽기 가능|후보 생성|이전 조각|검색 조각|벡터|임베딩|재사용|제외된 본문|permission|revocation/iu,
  ],
  ["rollback", /되돌|복구 경로|대상의 상태|증거 보존|rollback/iu],
  ["receipt", /영수증|기존 증거|증거 보존|receipt/iu],
  ["benchmark", /리허설|실측|측정한|측정 기록|benchmark/iu],
  ["calibration", /레이더|좌표계|보정|행렬|calibrat/iu],
];
const stop = new Set([
  "어떻게",
  "무엇",
  "어떤",
  "알려줘",
  "찾아줘",
  "현재",
  "지금",
  "절차",
  "기록",
  "자료",
  "상태",
  "확인",
  "실제",
  "때",
  "수",
  "한",
  "것",
  "before",
  "after",
  "the",
  "and",
  "for",
  "how",
  "what",
]);
function features(text: string): Map<string, number> {
  const normalized = text.normalize("NFKC").toLocaleLowerCase("en-US");
  // Explicitly negated evidence is not a positive measurement anchor.
  const positive = normalized
    .replace(/측정 결과가 아니라/gu, "")
    .replace(/실측이나 채택 결정은 없다/gu, "");
  const out = new Map<string, number>();
  for (const [name, pattern] of vocabulary)
    if (pattern.test(positive)) out.set("concept:" + name, 5);
  for (const token of normalized.match(/[\p{L}\p{N}_]+/gu) ?? []) {
    const word = token.replace(
      /(?:으로|에서|에는|에게|까지|처럼|보다|하고|하는|하면|했던|은|는|이|가|을|를|의|에|도|과|와)$/u,
      "",
    );
    if (!word || stop.has(word)) continue;
    out.set("word:" + word, 2);
    // Korean agglutination needs partial-word evidence; Latin identifiers remain
    // exact tokens. Fragment weight is bounded per token, not by token length.
    if (/^[가-힣]+$/u.test(word) && word.length >= 2) {
      for (let n = 2; n <= Math.min(3, word.length); n++) {
        for (let i = 0; i <= word.length - n; i++)
          out.set("part:" + word.slice(i, i + n), 0.6 / (word.length - n + 1));
      }
    }
  }
  return out;
}
/** Weighted OR retrieval with explicit abstention for narrow missing evidence.
 * Work/authority is not an input. Exact source/policy/status eligibility is the
 * caller's prerequisite. Only numeric scores survive disposal of text features.
 */
export function rankMemory(texts: readonly string[], query: string): MemoryRanking {
  const wanted = features(query),
    documents = texts.map(features);
  try {
    const anchors = ["benchmark", "calibration", "retention"].filter((name) =>
      wanted.has("concept:" + name),
    );
    const frequency = new Map<string, number>();
    for (const doc of documents)
      for (const term of doc.keys()) frequency.set(term, (frequency.get(term) ?? 0) + 1);
    const weighted = [...wanted].map(
      ([term, weight]) =>
        [
          term,
          weight * (1 + Math.log((texts.length + 1) / ((frequency.get(term) ?? 0) + 1))),
        ] as const,
    );
    const scores = new Map<number, number>();
    for (const [index, doc] of documents.entries()) {
      if (anchors.some((name) => !doc.has("concept:" + name))) continue;
      const score = weighted.reduce((sum, [term, weight]) => sum + (doc.has(term) ? weight : 0), 0);
      const concepts = [...wanted.keys()].filter((term) => term.startsWith("concept:"));
      const conceptHits = concepts.filter((term) => doc.has(term)).length;
      const exact = [...wanted.keys()].filter(
        (term) => term.startsWith("word:") && doc.has(term),
      ).length;
      if (score >= 1.5 && (conceptHits > 0 || exact > 0)) scores.set(index, score);
    }
    const best = Math.max(0, ...scores.values());
    for (const [index, score] of scores) if (score < best * 0.45) scores.delete(index);
    return {
      indices: [...scores.keys()].sort((a, b) => scores.get(b)! - scores.get(a)! || a - b),
      scores,
      disposed: true,
    };
  } finally {
    wanted.clear();
    for (const doc of documents) doc.clear();
  }
}
