//! Indexing pipeline.
//! Walks sources → parsers → extractors → storage.

pub mod extractor;
pub mod indexer;
pub mod parsers;

pub use extractor::{
    extract_for_session, extract_from_message, extract_from_turn, ExtractedInsights,
};
pub use indexer::{IndexStats, Indexer};
