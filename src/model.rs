use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum SourcePayload {
    Pdf(Vec<u8>),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct SourceDocument {
    pub path: PathBuf,
    pub name: String,
    pub media_type: String,
    pub sha256: String,
    pub extracted_text: String,
    pub page_count: Option<u32>,
    pub domain: String,
    pub payload: SourcePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mechanism {
    pub name: String,
    pub domain: String,
    #[serde(default)]
    pub statement: String,
    #[serde(default)]
    pub why_it_bites: String,
    #[serde(default)]
    pub trap_recipe: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceRef {
    pub locator: String,
    pub support: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DifficultyFeatures {
    pub essential_inferences: u8,
    pub representation_changes: u8,
    pub cue_visibility: String,
    pub distractor_attractiveness: String,
    pub computational_burden: String,
}

/// One extracted, source-grounded starting point for composition: a worked
/// example, stated theorem/formula, or solved exercise with its answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedProblem {
    pub seed_id: String,
    /// worked_example | theorem_or_formula | solved_exercise | concept
    pub kind: String,
    pub statement: String,
    /// Givens/assumptions restated so the seed stands alone.
    pub givens: String,
    /// The seed's own answer/conclusion when the source states one.
    pub known_answer: String,
    pub locator: String,
}

/// The move composition the orchestrator assigned to an item slot. The author
/// model never chooses its own rung label; difficulty targeting is ours.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoveAssignment {
    pub seed_id: String,
    pub move_keys: Vec<String>,
    /// Composition operators applied on the top rung: lift | arm | cycle | dualize
    pub operators: Vec<String>,
    /// high | medium | low — how loudly the stem may signal the decisive move.
    pub cue_visibility: String,
    /// 1..=4, see moves::LADDER.
    pub rung: u8,
    /// What-if-not setup mutations (moves::MUTATIONS keys) applied to the
    /// seed's silent assumptions — the "layered lens" generator.
    #[serde(default)]
    pub mutations: Vec<String>,
    /// Couple the seed's law to standard prerequisite-topic machinery
    /// (vectors, kinematics, ratios...) — the "mirror × kinematics" generator.
    #[serde(default)]
    pub bridge: bool,
}

/// Deterministic verification evidence (architecture §9.2). `verdict` is only
/// ever proved | disproved | unsupported; unsupported never passes a gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Verification {
    /// sympy | numeric | none
    pub kind: String,
    /// Self-contained Python that recomputes the keyed answer from restated
    /// givens and asserts it matches. Run inside the AST-checked sandbox.
    pub script: String,
    /// proved | disproved | unsupported | not_run
    pub verdict: String,
    pub detail: String,
}

/// One blind-solve observation reinterpreted as an Elo match: the item "wins"
/// when the reference player answers incorrectly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRecord {
    pub player: String,
    pub player_rating: f64,
    pub player_correct: bool,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EloState {
    pub rating: f64,
    pub deviation: f64,
    /// Anchor-scale version; re-anchoring learner data may re-map old ratings.
    pub anchor_version: u8,
    pub provisional: bool,
    pub probes: Vec<ProbeRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CandidateQuestion {
    #[serde(default)]
    pub id: String,
    pub domain: String,
    pub topic: String,
    pub instructional_purpose: String,
    pub stem: String,
    /// mcq | multi | integer | decimal (defaults to mcq for older records)
    #[serde(default = "default_question_type")]
    pub question_type: String,
    pub diagram_svg: Option<String>,
    /// Dark-mode rendering of the same figure (400-shade palette).
    #[serde(default)]
    pub diagram_svg_dark: Option<String>,
    /// Base64 PDF renderings (macOS-native display; no poppler needed).
    #[serde(default)]
    pub diagram_pdf: Option<String>,
    #[serde(default)]
    pub diagram_pdf_dark: Option<String>,
    /// Source TikZ the diagram was rendered from (kept for audit/regeneration).
    #[serde(default)]
    pub diagram_tikz: String,
    /// none | block | wrap — author's layout hint for the figure.
    #[serde(default)]
    pub figure_placement: String,
    /// Per-option rendered SVGs (same order as options), empty when unused.
    #[serde(default)]
    pub option_svgs: Vec<String>,
    /// Exact numeric answer for integer/decimal items.
    #[serde(default)]
    pub numeric_answer: String,
    /// All correct option indices for multi items (answer_index = first).
    #[serde(default)]
    pub correct_indices: Vec<u8>,
    pub options: Vec<String>,
    pub answer_index: u8,
    pub worked_solution: String,
    pub decisive_insight: String,
    pub distractor_rationales: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
    pub difficulty: DifficultyFeatures,
    #[serde(default = "source_truth_status")]
    pub truth_status: String,
    #[serde(default)]
    pub source_name: String,
    #[serde(default)]
    pub source_hash: String,
    #[serde(default)]
    pub validation: ItemValidation,
    #[serde(default)]
    pub moves: MoveAssignment,
    #[serde(default)]
    pub verification: Verification,
    #[serde(default)]
    pub elo: Option<EloState>,
}

fn default_question_type() -> String {
    "mcq".to_owned()
}

fn source_truth_status() -> String {
    "source_faithful_only".to_owned()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItemValidation {
    pub local_gate: bool,
    pub blind_answer_index: Option<u8>,
    pub blind_confidence: Option<f64>,
    pub blind_issue: Option<String>,
    pub fidelity_gate: bool,
    pub construct_gate: bool,
    pub presentation_gate: bool,
    pub novelty_gate: bool,
    pub reviewer_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindResult {
    pub item_id: String,
    pub answer_index: u8,
    /// The solver's answer for non-option types (numeric value, or
    /// comma-separated indices for multi).
    #[serde(default)]
    pub answer_text: String,
    pub solvable: bool,
    pub confidence: f64,
    pub issue: String,
    /// What a source-blind reader could NOT pin down from the stem alone
    /// (undefined object, ambiguous term, unclear asked quantity). Empty
    /// when the stem parses in one read.
    #[serde(default)]
    pub parse_issues: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub item_id: String,
    pub fidelity: bool,
    pub correctness: bool,
    pub construct_quality: bool,
    pub presentation: bool,
    pub novelty: bool,
    pub diagram_consistent: bool,
    pub accept: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub phase: String,
    pub model: String,
    pub source_hash: Option<String>,
    pub usage: Option<Usage>,
    pub actual_cost_usd: f64,
    pub uncertain_reservation_usd: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    pub path: String,
    pub name: String,
    pub media_type: String,
    pub sha256: String,
    pub extracted_chars: usize,
    pub page_count: Option<u32>,
    pub domain: String,
    pub requested: usize,
    pub submitted: usize,
    pub accepted: usize,
    #[serde(default)]
    pub seeds_extracted: usize,
    #[serde(default)]
    pub oracle_proved: usize,
    pub rejection_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLedger {
    pub job_id: String,
    pub idempotency_key: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub requested_count: usize,
    pub accepted_count: usize,
    pub budget_cap_usd: f64,
    /// Lifetime committed spend already paid before the current authorized budget epoch.
    #[serde(default)]
    pub budget_baseline_usd: f64,
    pub actual_spend_usd: f64,
    pub uncertain_spend_usd: f64,
    /// Worst-case reservations for calls currently in flight. Never persisted:
    /// the budget gate checks committed + inflight so concurrent calls cannot
    /// jointly breach the cap.
    #[serde(skip)]
    pub inflight_reservation_usd: f64,
    pub author_model: String,
    pub validator_model: String,
    pub sources: Vec<SourceRecord>,
    pub calls: Vec<CallRecord>,
    pub output_markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedInventory {
    pub complete: bool,
    pub requested_count: usize,
    pub accepted_count: usize,
    pub learner_release_allowed: bool,
    pub items: Vec<CandidateQuestion>,
}

impl JobLedger {
    pub fn lifetime_committed_spend(&self) -> f64 {
        self.actual_spend_usd + self.uncertain_spend_usd
    }

    pub fn committed_spend(&self) -> f64 {
        (self.lifetime_committed_spend() - self.budget_baseline_usd).max(0.0)
    }
}
