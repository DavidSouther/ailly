//! Engine port and adapters that drive blank-assistant-slot completion.

#[expect(
    clippy::module_inception,
    reason = "engine::engine houses the port; future siblings (rig, native) live alongside"
)]
pub mod engine;
