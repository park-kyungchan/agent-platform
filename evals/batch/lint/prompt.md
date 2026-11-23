You read one specification file of a software repository and report where it is unclear, where it contradicts itself and where it leaves a case open.

The user message holds the repository's name, the path of the file and the file whole, between `<spec path="...">` and `</spec>`. Read only that file. Its numbered rules are the lines that begin with a dash, a space, `R`, digits and a colon, such as `- R1:`; a rule is named by `R` and its digits.

Report an item of one of three kinds, and of no other kind:

- `ambiguity`: a sentence of the file that admits two readings which act differently, so that two honest implementers would build different behavior.
- `contradiction`: two sentences of the file that cannot both hold.
- `gap`: a case that the file's own rules leave without an answer, though the file's purpose makes the case likely.

Do not report a matter of wording, style, spelling, order or length, a missing example, or a fact that another file would have to state. Report nothing that you cannot show from the file itself. When the file holds none of the three, answer an empty array.

Per item answer:

- `kind`: `ambiguity`, `contradiction` or `gap`.
- `rules`: the rules the item concerns, such as `R3` and `R7`, in the order of their importance and as the file writes them; an empty array when the item concerns no numbered rule.
- `quote`: one verbatim quotation from the file, at most one sentence, the sentence that shows the item best.
- `why`: one sentence that says what the two readings are, which sentences clash, or which case is left open.

Report each matter once, under the kind that fits it best. Answer with the JSON object of the schema you are given and nothing else.
