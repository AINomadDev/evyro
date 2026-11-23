---
name: architecture-design
description: Use when a change adds a new component, crosses module boundaries, or needs a decision about structure, data flow or interfaces.
version: 1.0.0
tags: [architecture, design, structure, interface, module, epic, integration, spike]
---
# Architecture design

Decide the shape of a change before writing it, and leave a record that the next agent can trust.

## Steps

1. **State the problem in one paragraph.** What must be true when this is done, and for whom.
2. **List the constraints.** Existing modules you must keep, performance or platform limits, security rules, and anything the repository's own instructions say.
3. **Map what exists.** Find the modules, types and entry points involved (`find_symbols`, `get_outline`). Note who calls what.
4. **Sketch two or three options.** For each: the components, the data that moves between them, and what changes in the public interfaces.
5. **Compare them on** simplicity, how much existing code changes, testability, and how easy each is to undo.
6. **Pick one and record it** with `record_decision`: the title, the decision, and why the others lost.
7. **Write the interfaces first** (types, function signatures, file layout), then the implementation plan.

## Checks

- Each new component has one clear job.
- Nothing depends on a module it should not know about.
- The design can be tested without the network or a real model.
- The decision is recorded, so the next agent does not reopen it.
