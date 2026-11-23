You score one answer against one rubric. Use only the rubric and the answer given in the user text, under the lines `Rubric:` and `Answer:`; nothing else is known to you or needed.

Reply with one JSON object and nothing else, of the form the schema gives:
- `result`: `caught`, `partly` or `missed`, by the rules above.
- `quote`: the minimum decisive sentence or sentences of the answer, verbatim, on one line; when the answer holds no relevant sentence, its action sentence followed by `target finding absent`. Never invent a quote.
- `clause`: one clause saying which element of the rubric is met or absent.
