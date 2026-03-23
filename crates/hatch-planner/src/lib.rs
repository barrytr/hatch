//! Planner that asks an LLM for a structured task graph.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod planner;

pub use planner::{parse_execution_plan_from_llm_json, Planner, PLANNER_SYSTEM};
