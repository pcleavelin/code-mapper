---
name: debrief
description: Comb the current session for every place the owner had to step in - a correction, an overridden recommendation, a rework of something delivered, an interrupt, a rejected tool call, a mid-turn redirect, a restated rule, a question that was not needed - and turn each into a change to this repo's skills, checks, map or CLAUDE.md, so the next agent builds what the owner meant without being steered. Use when the owner asks for a debrief, says they had to micromanage, or at the end of a long session.
---

# Debrief: make the repo carry what the owner had to say

The measure is how often the owner had to step in per session. Every step-in is something
the repo failed to tell the agent at the moment it acted. The fix goes into the repo, never
into one agent's memory: a lesson only one agent remembers does not reach the next one.

## 1. Collect the events

Read the transcript, not your recollection: after a compaction the early steering is gone
from context. Find the file by a phrase from the owner's latest message (a session started
in a workspace has its own project directory, so search them all):

```
ls -t ~/.claude/projects/*/*.jsonl | head -30 | xargs grep -l -F '<phrase>' | head -1
```

A `/clear` starts a new file, so this file is the session. Extract the events:

```
jq -rs -f .claude/skills/debrief/extract.jq <transcript>
```

Each event prints what you had just said and what the owner did: `prompt`, `mid-turn message`
(typed while you worked), `answer to a question` (each question with your recommendation,
the choice, `OVERRODE` or `took the recommendation`, and any notes), `interrupt`,
`rejected tool call` (with the reason when one was given). Read around an event in the
transcript with `jq` or `grep` when it needs more context, never the whole file.

## 2. Classify each event

- **Task**: the opening request, or a new request after the last one was delivered that
  does not change it. Skip it.
- **Correction**: the owner changed what you did or proposed: an overridden recommendation,
  an answer that rejects the question's premise, an interrupt, a rejected tool call, a "no",
  a mid-turn redirect, or a request after delivery that reworks what was delivered ("that is
  not what I meant", "make it X instead").
- **Missing fact**: the owner supplied a preference or fact nothing in the repo states.
- **Unneeded question**: the owner took your recommendation. One alone is fine; the same kind
  of question taken as recommended in earlier debriefs (step 4) means the skill that led to
  it should say the agent decides it.
- **Repeat**: the owner restated something a skill, a check, the map or CLAUDE.md already
  says. The repo told the agent and the agent still missed it; this kind matters most.
- **Stall**: the owner had to nudge you on ("keep going", "just do it"), or answer something
  you could have looked up.

## 3. Find the cause of each non-task event

For each: what you believed or did; where it came from (a default, a guess, a misreading of
a named skill, CLAUDE.md line or note, a step you skipped); and the sentence that, read at
that moment, would have produced what the owner meant. Write the sentence for a moment the
agent will recognise ("when a feature stores a new kind of data, ...") and give its reason.
Quote the owner's words as written; never widen them into a stronger rule than they stated.

A one-off product decision that the code, the map or a commit now records has no lesson
beyond itself; say so and move on.

## 4. Check what the repo already says

Search the skills (`.claude/skills/`), CLAUDE.md, `codemap . notes <regex>`, and the earlier
debriefs: `jj log -r 'description(glob:"debrief:*")' --no-graph -T description`. When the
lesson is already there, the event is a repeat: the text was too vague, too far from the
moment, or somewhere the agent does not look. Strengthen and move it (step 5); never add a
second copy. When the agent's memory for this project holds a lesson about this repo, move
it into the repo the same way and delete the memory.

## 5. Put each lesson where the agent meets the moment

| The lesson is about | Its home |
|---|---|
| Reading the owner's request: its scope, which reading was meant, what only the owner decides | `understand` |
| Sketching, comparing or choosing designs; which kind of design the owner prefers | `prototype` |
| How the GUI looks and behaves | `add-gui-element` |
| A step of any other procedure | that skill's step |
| The shape of code, where a tool can see it | a check, through `tripped` |
| What code does or why it is built so | the map (the tour note) or CLAUDE.md, "Why it is built this way" |
| How every session works in this repo | CLAUDE.md |

Prefer the place read at the moment of the mistake over the one always loaded, and a check
over a sentence when a tool can see the mistake. A repeat moves up one level: a sentence into
the skill step where the moment happens, a skill step into a check. Each lesson is one or two
sentences in the target's own voice, with its reason when the reason is not obvious; edit
the sentence that covers the topic rather than adding a near-duplicate, and delete what the
session proved wrong.

## 6. Commit the debrief

Run `finish`. The commit's message is the debrief's record, read by the next debrief:

```
debrief: corrections N, missing facts N, unneeded questions N, repeats N, stalls N

- <the owner's words, short> -> <lesson in one line> -> <file> (new | strengthened | moved up)
- <the owner's words, short> -> no lesson: <why>
```

## 7. Report

One row per non-task event: what happened (the owner's words), the cause, the lesson, and
where it went or why nowhere. Then the counts beside the previous debriefs' counts, and the
commit.
