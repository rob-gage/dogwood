# Style Guide for Engine Internals

## General
- All variable declarations should specify an explicit type for that variable rather than leaving it to be inferred.
- Functions should not have empty lines within their bodies.
- Documentation should be one-line comments on most items, and these header comments for types/fields/functions should
not be sentences unless necessary, and when they aren't they shouldn't be punctuated like one.
- Comments within function bodies should not start with capital letters unless they are sentences, which they should
generally not be.
- Comments within function bodies should usually be one line, only inserted between separate sections of functions that
are large enough to warrant them, for smaller functions or ones where the logic is obvious, no internal comments
should be added.

## Rust
- `impl` blocks should contain an empty line before the first item they contain after the opening brace, and an empty
line after the last item before the closing brace.
- Each separate type or non-method free function should be stored in its own file with a matching name.
- `use` imports should be structured as follows:
  + The order of imports from top to bottom should be: `super`, `crate`, other workspace crates (alphabetical), `std`,
  third party crates.
  + No containing module or crate should be imported more than once, if multiple children are imported they should all
  be imported through it using braces.
  + Each separate imported item should be on a new line.
  + Example:
    ```rust
    use super::{
        ItemA,
        ItemB,
    };
    use crate::{
        module_x::ItemX,
        module_y::{
            ItemY,
            ItemZ,
        }
    };
    use other_workspace_crate::function;
    use std::collections::HashMap;
    use third_party_crate::Stuff;
    ```

## WGSL