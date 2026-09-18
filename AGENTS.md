## Game-facing documentation invariant

`dogwood_engine` is the supported facade for game code. The static manual in `documentation/` and the template project are part of the public game-facing interface and must remain synchronized with it.

When making a change, determine whether it affects:

- a public item exposed through `dogwood_engine`;
- `dogwood_engine` module structure or re-exports;
- behavior that game code relies on;
- actor/gameplay lifecycle or persistence;
- scene query/editing APIs;
- materials, physics, input, UI, rendering, save/load, or streaming APIs;
- game-supplied configuration;
- canonical usage demonstrated by the template project.

If so, update the relevant documentation in the SAME task and SAME change set. The template project is executable documentation: when recommended API usage changes, update its template demonstration as well as prose documentation. Document the public game-facing abstraction rather than unnecessarily exposing lower-level implementation details. Private refactors that preserve public behavior do not require documentation changes.

Before considering a game-facing API change complete:

1. Update relevant Rust doc comments where appropriate.
2. Update affected `documentation/` pages.
3. Update template examples if recommended usage changed.
4. Build/check the documentation site.
5. Verify documented identifiers/examples against current source.
6. Do not leave stale names, signatures, behavior, or module locations documented.

If a task introduces a new game-facing subsystem or capability, document enough that a gameplay programmer can discover and use it without reading the subsystem implementation. Treat stale game-facing documentation as a defect in the task that created the API change.
