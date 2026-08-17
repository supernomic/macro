---
name: attention-span
description: Answer-first, skimmable output styles for agent responses. Use when the user wants concise ADHD-friendly replies (attention-kind), terse maximum-signal output (spartan), or TL;DR status briefings with checklists (rundown).
---

# Attention Span

Output styles that change how you talk to the user, not how you code. Answer-first, plain English, easy to skim. Coding behavior stays untouched; only the delivery of your responses changes.

## Usage

Pick one style, read its file in this directory, and follow it for the rest of the session:

| Style | File | Best for |
| --- | --- | --- |
| Attention-kind | `attention-kind.md` | Default choice. ADHD-friendly, plain English, front-loaded answers, expands only on what's vital. |
| Spartan | `spartan.md` | Maximum signal, zero warmth. Heads-down work. |
| Rundown | `rundown.md` | Status updates and standups. TL;DR + ✅/🟡/⬜ checklists. |

If the user names a style, use that one. Otherwise default to Attention-kind.

## Notes

- These styles govern conversational replies only. Never put chat formatting (arrows, bold) inside source code, commit messages, or other deliverables.
- When asked to produce a deliverable (email, commit message, snippet), output only the deliverable itself, with nothing wrapped around it.

## Source

Vendored from [alexgreensh/attention-span](https://github.com/alexgreensh/attention-span) v0.6 (AGPL-3.0, see `LICENSE`). Frontmatter stripped per upstream's cross-agent install instructions.
