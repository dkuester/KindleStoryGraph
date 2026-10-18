//! KindleStoryGraph — StoryGraph sync for the native Kindle reader.
//!
//! Phase 1: read-only reader for the Kindle content catalog (`cc.db`).

pub mod config;
pub mod daemon;
pub mod error;
pub mod reader;
pub mod state;
pub mod storygraph;
pub mod sync;
