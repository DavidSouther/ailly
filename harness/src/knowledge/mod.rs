//! Knowledge layer: pure functions that consume a `Conversation` and produce
//! per-assertion verdicts. Parallel to `content` (domain values) and `engine`
//! (LLM I/O).

pub mod assertions;
pub mod eval;
pub mod report;
