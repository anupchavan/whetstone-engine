pub mod protocol;
pub mod room;

use serde::{Deserialize, Serialize};

/// The one quiz item shape every host understands - native app, hosted
/// server, or (later) a WASM tab. Deliberately tiny and self-contained.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QuizItem {
    pub stem: String,
    pub options: Vec<String>,
    pub answer_index: u8,
    #[serde(default)]
    pub question_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_pdf: Option<String>,
}
