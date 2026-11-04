# Standalone Scroll Narrative

Read this reference only when the requested deliverable is standalone HTML or a scroll-driven explanation. Retain the no-analogy and beginner-language requirements in `SKILL.md`.

## Complete explanation

Produce one local `.html` file with embedded CSS and JavaScript. Keep required text, diagrams, and navigation available offline. Use inline SVG, HTML, CSS, or ASCII for diagrams; avoid remote fonts, scripts, libraries, and other network dependencies unless the user explicitly requires them.

Write the full explanation into semantic HTML. JavaScript may enhance visualization but must not be required to reveal essential prose. Do not replace explanations with brief cards, animations, or unexplained labels.

## Scrolling and execution

Let the document scroll normally. As each explanation step enters view, update an adjacent or sticky diagram to show the relevant code, input, executing component, comparison, result, or failure state. Label the currently explained state in text.

Use actual execution relationships. Scrolling selects which step is explained; it does not prove or perform that execution. Do not animate fictional activity as verified live behavior.

- Show sequential work, parallel branches, joins, failures, and retries only when present in the described system.
- Keep source-version identities and evidence classes readable when they matter.
- Retain the user's requested depth across the whole end-to-end flow.
- Provide section navigation and a static diagram or textual equivalent that explains the same relationships without animation.

## Accessibility and responsive behavior

Use semantic headings, labeled navigation, keyboard-visible focus, readable contrast, and selectable text. Do not communicate pass/fail or active state through color alone.

Support `prefers-reduced-motion`; remove scroll-linked motion and transitions while retaining active-step information. Avoid forced scroll snapping and scroll interception.

On narrow screens, place diagrams beside or before their explanation without hiding text or requiring a desktop viewport. Ensure code and diagrams do not cause whole-page horizontal overflow. Preserve a readable print view with the complete prose.

If a browser verification tool is available and authorized, check desktop and mobile reading, section navigation, scroll-selected state, reduced motion, and the JavaScript-disabled fallback. Report only checks actually performed.

Creating a local document does not authorize publishing it, deploying a service, executing the illustrated commands, or changing runtime configuration.
