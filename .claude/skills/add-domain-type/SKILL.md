---
name: add-domain-type
description: Add a new concept or value to the model in crates/domain - an id, a measure, a name, a new entity - or give an existing value its own type. Use when a signature would otherwise take a bare number, string or bool, or when two places describe the same thing differently.
---

# Add a domain type

1. Look first: `api/domain.api` and `cargo xtask words`. If the concept has a type, use it;
   if it has a word in `vocabulary.txt` under another name, use that name.
2. The type, in the domain module it belongs to (`text`, `index`, `map`):
   - a single value: a tuple struct with a private field;
   - a constructor that refuses invalid values (`-> Option<Self>` or `-> Result<Self, ...>`),
     or `From` when every value is valid;
   - only the operations the meaning allows (`Line + LineOffset -> Line`, never
     `Line + Line`), as operator trait impls where they are arithmetic;
   - accessors for the inner value, used at I/O edges only.
3. The word: add it to `vocabulary.txt` if it is new. If it replaces a synonym already in use,
   add a `synonym -> word` line only with the owner's agreement.
4. The glossary: a row in design.md section 4 for a concept a reader of the map meets.
5. Replace every bare use it stands for (`cargo xtask lint` points at them), with unit tests
   for the constructor's refusals and each operation.
6. `cargo xtask api`, then `finish`.
