import { Database } from "bun:sqlite";

export const retrievalVersion = "literal-unicode-and.v1" as const;
/** Baseline matches the retained FTS5 literal-token AND exposure. No semantic
 * representation is created here. Caller must qualify every input first. */
export function rankMemory(texts: readonly string[], query: string): { indices: number[]; disposed: true } {
  const terms = [...new Set(query.match(/[\p{L}\p{N}_]+/gu) ?? [])];
  const db = new Database(":memory:");
  try {
    db.run("CREATE VIRTUAL TABLE recall USING fts5(ordinal UNINDEXED,text,tokenize='unicode61')");
    const insert = db.query("INSERT INTO recall(ordinal,text) VALUES (?,?)");
    texts.forEach((text, index) => insert.run(index, text));
    if (!terms.length) return { indices: [], disposed: true };
    const match = terms.map(term => '"' + term.replaceAll('"', '""') + '"').join(" AND ");
    return { indices: db.query<{ ordinal: number }, [string]>("SELECT ordinal FROM recall WHERE recall MATCH ? ORDER BY bm25(recall),rowid").all(match).map(row => row.ordinal), disposed: true };
  } finally { db.close(); }
}
