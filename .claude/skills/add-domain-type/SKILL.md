---
name: add-domain-type
description: Add a new concept or value to the model in crates/domain - an id, a measure, a name, a new entity - or give an existing value its own type. Use when a signature would otherwise take a bare number, string or bool, or when two places describe the same thing differently.
---

# Add a domain type

1. Look first: `api/domain.api` and the Concepts table in CLAUDE.md. If the concept has a
   type, use it; if the code already names it with another word, use that word.
2. The type, in the domain module it belongs to (`text`, `index`, `map`):
   - a single value: a tuple struct with a private field;
   - a constructor that refuses invalid values (`-> Option<Self>` or `-> Result<Self, ...>`),
     or `From` when every value is valid;
   - only the operations the meaning allows (`Line + LineOffset -> Line`, never
     `Line + Line`), as operator trait impls where they are arithmetic;
   - accessors for the inner value, used at I/O edges only.
3. The glossary: a row in the Concepts table of CLAUDE.md for a concept a reader of the map meets.
4. Replace every bare use it stands for (`cargo xtask lint` points at them), with unit tests
   for the constructor's refusals and each operation.
5. `cargo xtask api`, then `finish`.
