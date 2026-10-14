import { hash } from "../../src/backend/review/source-coverage.ts";
// Deliberately synthetic future-format ID, not a captured private native session.
export function reviewFixture() {
  const content = "A😀Z";
  const request = "Synthetic source review request.";
  const document = {
    id: "synthetic-document",
    source: "synthetic-source.ts",
    content,
    sha256: hash(content),
  };
  const packet = JSON.stringify({ review_id: "synthetic-review", documents: [document] });
  const expected = {
    packetSha256: hash(packet),
    sessionId: "20990101_000000_000000",
    requestSha256: hash(request),
    model: "synthetic-model",
    provider: "synthetic-provider",
  };
  const args = { document_id: document.id, offset: 0, limit: 3 };
  const page = {
    review_id: "synthetic-review",
    packet_sha256: hash(packet),
    document_id: document.id,
    source: document.source,
    source_sha256: document.sha256,
    offset: 0,
    content,
    total_chars: 3,
    has_more: false,
  };
  const observation: any = {
    session: {
      id: expected.sessionId,
      model: expected.model,
      billing_provider: expected.provider,
      ended_at: 2,
      message_count: 4,
      tool_call_count: 1,
    },
    messages: [
      { id: 1, session_id: expected.sessionId, role: "user", content: request },
      {
        id: 2,
        session_id: expected.sessionId,
        role: "assistant",
        content: "",
        tool_calls: JSON.stringify([
          {
            id: "synthetic-call",
            function: { name: "ocp_review_read", arguments: JSON.stringify(args) },
          },
        ]),
      },
      {
        id: 3,
        session_id: expected.sessionId,
        role: "tool",
        tool_call_id: "synthetic-call",
        tool_name: "ocp_review_read",
        content: JSON.stringify(page),
      },
      {
        id: 4,
        session_id: expected.sessionId,
        role: "assistant",
        content: JSON.stringify({
          review_id: "synthetic-review",
          verdict: "revise",
          findings: [],
          limits: ["Synthetic fixture; not an actual independent review."],
        }),
      },
    ],
  };
  return { packet, expected, args, page, observation };
}
