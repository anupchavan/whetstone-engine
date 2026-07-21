//! The classroom wire protocol: versioned JSON over WebSocket, identical
//! over LAN and relay transports. Every host→client message is idempotent
//! and self-sufficient, so a reconnecting client renders from any point.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;

/// One quiz question as the classroom sees it. Options arrive pre-shuffled
/// PER CLIENT (anti-copying); `answer` therefore never travels in `stage`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StagedQuestion {
    pub index: usize,
    pub total: usize,
    pub question_text: String,
    #[serde(default)]
    pub question_type: String,
    pub options: Vec<String>,
    /// Seconds the question stays open once revealed.
    pub time_limit_secs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_pdf: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMessage {
    /// `token` present on rejoin: the player keeps identity and score.
    Join {
        v: u8,
        room: String,
        name: String,
        #[serde(default)]
        token: Option<String>,
    },
    Answer {
        v: u8,
        q: usize,
        choice: u8,
        /// Client-clock ms, display only; scoring uses host arrival time.
        #[serde(default)]
        sent_at: u64,
    },
    Ping { v: u8, id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreLine {
    pub name: String,
    pub score: u32,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum HostMessage {
    Welcome {
        v: u8,
        token: String,
        name: String,
        /// Full current room snapshot: render from scratch on (re)join.
        phase: PhaseSnapshot,
        players: usize,
    },
    /// Payload delivery, ahead of time; render nothing yet.
    Stage { v: u8, question: StagedQuestion },
    /// The tiny flip: open the staged question until `deadline_ms`
    /// (host-clock epoch ms; clients translate via their ping offset).
    Go { v: u8, q: usize, deadline_ms: u64 },
    Ack { v: u8, q: usize, accepted: bool },
    /// Question closed: correct index IN THIS CLIENT'S shuffled order.
    Closed {
        v: u8,
        q: usize,
        correct: u8,
        tally: Vec<u32>,
        you: Option<ScoreLine>,
    },
    Leaderboard {
        v: u8,
        top: Vec<ScoreLine>,
        you: Option<ScoreLine>,
    },
    Podium { v: u8, top: Vec<ScoreLine> },
    Pong { v: u8, id: u64, host_ms: u64 },
    Error { v: u8, reason: String },
    /// Live tally for the TEACHER screen only, coalesced at ~10Hz.
    HostTally { v: u8, q: usize, answered: usize, players: usize },
}

/// Where the room is right now — enough to render any screen cold.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum PhaseSnapshot {
    Lobby,
    Staged { question: StagedQuestion },
    Open { question: StagedQuestion, deadline_ms: u64 },
    Closed { q: usize, correct: u8, tally: Vec<u32> },
    Podium { top: Vec<ScoreLine> },
}
