//! Indexing pipeline.
//! Walks sources → parsers → extractors → storage.

pub mod indexer;
pub mod parsers;
pub mod extractor;

pub use indexer::{Indexer, IndexStats};
pub use extractor::{ExtractedInsights, extract_for_session, extract_from_turn, extract_from_message};
