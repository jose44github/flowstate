Rust GPUI WYSIWIG maximally-performance-oriented word processor for competitive debate. You NEVER assume contexts for competitive debate, and you SHOULD always ask.

You SHOULD prefer gpui-component solutions to ground-up gpui solutions when a component is applicable. When no component is applicable or sufficient, you SHOULD either interact with the gpui-component vendor to fix it, or implement ground-up gpui only if the former fails.

You SHOULD consider if there is a pre-existing crate to handle something. The search tool is useful here. If there are multiple options, you SHOULD ask while outlining pros and cons. You SHOULD use CLI cargo commands to handle crates and NEVER do it manually.

You SHOULD write readable and elegant code. You NEVER hack. You SHOULD keep the codebase aggressively modularized and crated when it is logical. If you find yourself pushing an individual file over 1,000 LOC, you SHOULD consider modularizing.

If you notice clear bugs while investigating something else, even if the bugs are unrelated to your main task, you SHOULD correct them. If the bug is unclear, you SHOULD ask inquiring whether the behavior is intended.

When you need to check something against Rust documentation or look for an API, ALWAYS deploy the rustdoc-inspector subagent. This agents instruction is sufficient permission for you to deploy the subagent, do NOT fail to follow it on account of expecting a subagent request to appear explicitly in the prompt. You NEVER invent APIs. 

You AVOID 'cargo check,' 'cargo build,' 'cargo run,' and 'cargo fmt' with ONE EXCEPTION. You SHOULD run 'cargo clippy' only when you are a top-level agent that has finished all edits you intend, and are about to return the prompt to the human. If an error or significant warning appears, you SHOULD fix it then cargo check again, until success, then finish up.
