---
name: add-data-source
description: Add a new way for codemap to read or write data outside the process - a file format, a program it runs, a network or IPC protocol. Needs the owner, because the crate graph and io crates' APIs are owner-only.
---

# Add a data source

Each source of outside data has one `io-*` crate with one entry point per direction. The crate
graph (`xtask/src/arch.rs`) and the `api/io-*.api` files are changed only by the owner, so
this starts with a proposal.

1. Check whether an existing io crate owns the data already (`api/io-*.api`). If it does,
   extend it there instead; stop here and ask the owner to accept the API change.
2. Propose to the owner, in your reply: the crate name, what it reads and writes, its
   dependencies, its entry points (`Store::load`, `Store::save`, ...), and the domain types it
   returns. Wait for the owner to add the crate to the graph.
3. Build the crate:
   - `src/wire.rs`: types that mirror the external format exactly, and the parser/printer;
   - `src/convert.rs`: wire to domain and back; nothing else names a wire type;
   - `src/lib.rs`: the entry points and one error enum carrying data (the CLI's wire layer
     writes its messages).
   A process start goes through `io-process`, a file write through `io-store`.
4. Tests: a round trip over real samples of the format, byte for byte.
5. `cargo xtask api`, then `finish`.
