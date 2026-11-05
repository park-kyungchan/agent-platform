# Standalone scroll narrative

Use only for requested standalone HTML or scroll-driven explanations. Follow
`SKILL.md`: Korean prose, original English programming terms, concrete first-use
definitions and no metaphor or analogy.

Produce a local `.html` file with semantic prose and embedded CSS/JavaScript.
Keep essential text, diagrams and navigation available offline and without
JavaScript. Do not add remote assets unless requested.

Let the page scroll normally. An adjacent diagram may highlight the current
input, executor, operation or result as its explanation enters view. Distinguish
illustrated execution from verified live behavior. Show parallelism, retries
and failures only when the actual system has them; label evidence and examples.

Provide keyboard navigation, visible focus, readable contrast and selectable
text. Do not communicate state with color alone. Respect `prefers-reduced-motion`,
preserve mobile reading without whole-page horizontal overflow, and retain a
complete print view. If a browser is available, verify those behaviors and the
JavaScript-disabled fallback; report only checks actually performed.

A local explanation does not authorize deployment, running illustrated commands
or changing runtime configuration.
