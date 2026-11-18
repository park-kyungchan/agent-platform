# Plan of intent widgets

## The intent

Outcome: Every invented widget is listed once.
First user: An invented reader of the widget list.
Not included: No prices of widgets.
Constraints: One line per widget.

Criteria:
- The list names every widget. — widgets list | wc -l equals the count
- Each widget appears once. — widgets list | sort | uniq -d is empty

## Units

### a: plain, lane objective, invented/widgets

The invented widget list shows each widget once.
Write set: src/widgets.rs
Estimate: 40 lines
Implementer: claude opus high
Verifier: claude opus xhigh

### b: plain, lane objective, invented/widgets

The invented widget list shows each widget once.
Depends on: a
Write set: src/widgets.rs
Specifications: specs/widgets.md#R1
Estimate: 40 lines
Implementer: claude opus high
Verifier: claude opus xhigh

### g: plain, lane small, invented/widgets

The invented widget list shows each widget once.
Write set: src/widgets.rs
Estimate: 12 lines
Implementer: claude opus high
Verifier: claude opus xhigh

These units depend on each other in a cycle:

### c: plain, lane objective, no repository

The invented widget list shows each widget once.
Depends on: d
Write set: src/widgets.rs
Estimate: 40 lines
Implementer: claude opus high
Verifier: claude opus xhigh

### d: plain, lane objective, invented/widgets

The invented widget list shows each widget once.
Depends on: c, a
Write set: src/widgets.rs
Implementer: claude opus high
Verifier: claude opus xhigh
