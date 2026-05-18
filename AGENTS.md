This is a Rust project using GPUI and GPUI-component.

REMEMBER you can use GPUI-component and SHOULD ALWAYS when there is a valid component available. When you forget this, you try to craft solutions in pure GPUI that are usually suboptimal, but GPUI-component components have parity and are optimal.

ALWAYS check if there is a pre-existing module or library to handle something that you're wanting to do. ALWAYS. If you want to pause and present different options to me before proceeding, do so.

The human assistant on this project is an absolute Rust novice and would appreciate comments throughout explaining what particular areas do.

The code ought to be as simple and readable as possible, and solutions ought to be elegant.

Before making architectural decisions, use Context7 MCP to inspect GPUI and GPUI-component documentation pertaining to the versions found in Cargo.toml.

Do not invent GPUI APIs. Verify method names, trait implementations, app/window setup, entity/state patterns, event handlers, and layout APIs against documentation or source examples before changing code.

After all edits are complete, run:

cargo check

Fix compiler errors before finishing.

Do not launch the project after making edits unless specifically requested.

The locally generated cargo docs and local crate source are authoritative for this project. If Context7 disagrees with local cargo docs or Cargo.lock, use the local docs/source.

Context7 is a lookup aid, not the source of truth. Cargo.toml + Cargo.lock + docs.rs for the exact version + local crate source are the source of truth.
