---
layout: doc
title: The PAVED Manifesto
---

# The PAVED Manifesto 📜

> What's the meta for software documentation in the AI agent era?

## The New Meta: Docs as Interfaces 🔌

Treat documentation like you treat APIs:

- **Precise contracts** - not vague prose
- **Small surfaces** - one concept per doc
- **Versioned** - track changes over time
- **Validated** - enforce quality rules
- **Optimized for retrieval + execution**

Agents don't need prose. They need **ground truth, constraints, examples, and how to verify**.

---

## PAVED: An Agent-Native Framework 🤖

### P - Purpose

What is this thing? What problem does it solve?

- 1-3 sentences
- Include non-goals

**Example:**

> "This service runs scheduled background jobs. It does not do real-time event processing."

---

### A - API / Interface

How do you use it? What are the entry points?

- CLI commands
- HTTP endpoints
- Library calls
- Config keys
- File formats

Agents thrive on tables and schemas here.

---

### V - Verification

How do you know it's working?

This is the #1 missing thing in most docs, and it's everything for agents.

Include:

- "Golden" commands
- Expected output snippets
- Healthcheck endpoints
- Invariants
- How to run tests
- "Common failure → diagnosis"

---

### E - Examples

Concrete copy/paste examples, ideally minimal:

- 1 happy path
- 1 realistic path
- 1 failure path

Agents use examples as "shape matching" to produce correct output.

---

### D - Decisions

Short rationale + constraints:

- Why this design exists
- What must not change
- Tradeoffs

This prevents agents from "refactoring your intent away."

---

## The Authoring Meta: Leaf Docs + Index Docs 🗂️

Agents do best when docs are:

**Leaf docs** (small + atomic):
- One concept
- One workflow
- One component
- One decision

**Index docs** (routing + map):
- "Start here"
- Links to leaf docs
- A 30-second mental model

This mirrors how agents retrieve: they want a map, then a target chunk.

---

## The Formatting Meta: Docs That Compile ✅

If you want agents to reliably author and maintain docs, make the format lintable.

**Recommended structure per doc file:**
- Frontmatter / metadata (optional)
- Sections with stable headings
- Bulleted constraints
- Code blocks that run

**Add doc quality gates:**
- "Every doc must include Verification"
- "Every public module must have one Example"
- "No doc > 300 lines; split instead"
- "Every example has expected output"

Agents can follow rules like these extremely well.

---

## The Killer Trick: Make Docs Executable 🚀

If you do only one thing, do this:

**Put commands in docs that actually run:**

```bash
make test
make lint
make smoke
./scripts/verify_<thing>.sh
curl localhost:8080/health
```

Agents can:
- Propose changes
- Then validate them
- Then update docs

Without verification hooks, they hallucinate correctness.

---

## The Wall-Worthy Quote 🖼️

> "Write docs like you're training a careful junior engineer who can run commands but can't read minds."

That's basically the AI agent era.
