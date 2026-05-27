Rust GPUI WYSIWIG maximally-performance-oriented word processor for competitive debate. You NEVER assume contexts for competitive debate, and you SHOULD always run 'ask.'

You SHOULD prefer gpui-component solutions to ground-up gpui solutions when a component is applicable. When no component is applicable or sufficient, you SHOULD either interact with the gpui-component vendor to fix it, or implement ground-up gpui only if the former fails.

You SHOULD consider if there is a pre-existing crate to handle something. If there are multiple options, you SHOULD run 'ask' while outlining pros and cons. You SHOULD use CLI cargo commands to handle crates and NEVER do it manually.

You SHOULD write readable and elegant code. You NEVER hack. You SHOULD keep the codebase aggressively modularized and crated when it is logical. If you find yourself pushing an individual file over 1,000 LOC, you SHOULD consider modularizing.

You NEVER invent APIs. If a first attempt fails, you SHOULD verify against examples or documentation, then retry.

You AVOID 'cargo check,' 'cargo build,' 'cargo run,' and 'cargo fmt' with ONE EXCEPTION. If you are a sub-agent, you NEVER fall under this exception. You SHOULD run 'cargo check' only when you are a top-level agent that has finished all edits you intend, and are about to return the prompt to the human. If an error or significant warning appears, you SHOULD fix it then cargo check again, until success, then finish up.

You NEVER call on an oracle, plan, or reviewer subagent without running 'ask' for human consent, or with an explicit human prompt. You SHOULD feel obligated to liberally use other applications of subagents.
