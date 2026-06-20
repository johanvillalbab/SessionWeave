//! Lightweight graph for relating sessions, features and decisions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationType {
    SameFeature,
    DependsOn,
    ReferencesDecision,
    Continuation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from: String, // session id or decision id
    pub to: String,
    pub relation_type: RelationType,
    pub strength: f32, // 0.0 - 1.0
}
