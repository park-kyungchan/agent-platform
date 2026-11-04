---
name: explain-code-clearly
description: Explain how a requested tool, command or execution mechanism works in clear Korean, with concrete inputs, executor and result. Use for tool explanations or explicit invocation; routine execution does not require a tutorial.
---

# Explain code clearly

Answer the precise question in Korean unless the user asks otherwise. A short clarification can be a few sentences and one small example; do not force titles, numbered sections, quizzes, a question after each action or a staged wait. A full requested report may use structure suited to its scope.

For a new mechanism, briefly name its actual components and connections. Then explain the relevant causal flow:

`user request → tool and arguments → actual executor → operation → returned result`

Distinguish a model-facing tool from an action inside its arguments, a helper in submitted code, and the process or service that executes it. Include only layers established by the source. Name unfamiliar terms plainly and consistently; do not substitute metaphors for concrete files, values and processes.

Use a small example in the language the real executor accepts. Explain relevant lines in execution order, including conditions or repetition when they answer the confusion. Label invented inputs and expected outputs as examples. State material side effects and permissions together with the action; explanations and examples grant no execution authority.

Use native source/help and relevant official documentation for version-dependent behavior. Keep documented support, effective configuration, available tools and observed execution distinct. A check's input, comparison and actual output should establish its limited conclusion. A timeout or interruption does not prove effects never occurred.

For exhaustive tool comparisons or detailed schemas, read only the relevant part of [tool-explanation.json](references/tool-explanation.json). For requested standalone HTML or scroll-driven explanations, use [scroll-narrative.md](references/scroll-narrative.md). Do not create those artifacts for ordinary chat.

During active work, answer a local explanation question and continue its authorized objective. Do not turn teaching into a new prerequisite or change task ownership, goals or native execution rules.
