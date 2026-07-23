//! Sidecar mode: newline-delimited JSON over stdin/stdout, speaking the
//! protocol the Whetstone SwiftUI frontend already uses:
//!
//!   get_state / set_config          → StateSnapshot
//!   scan_library                    → [NoteMeta]
//!   mastery_status                  → [MasterySnapshot]
//!   start_session / preparation_status → StartSessionResponse
//!   record_results                  → {deltas}
//!   record_attempts                 → {updated, missing}
//!   record_feedback                 → {updated}
//!   sync_note_changes               → change counts
//!
//! Count fidelity contract: the delivered QuestionSet is built verbatim from
//! the pipeline's validated inventory — the same artifact the CLI ships. The
//! UI renders that set as-is, so it can never disagree with the backend about
//! what was generated.

use crate::anthropic::AnthropicClient;
use crate::model::{CandidateQuestion, JobLedger, ValidatedInventory};
use crate::pipeline::{PipelineConfig, new_ledger, run_pipeline};
use crate::render::{write_inventory_checkpoint, write_ledger, write_outputs};
use crate::source::collect_sources;
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use walkdir::WalkDir;

const SUPPORTED: &[&str] = &["md", "markdown", "txt", "pdf"];
/// Not a user-facing cap (those were removed): a fixed runaway stop so a
/// pathological job can never spend without bound on the user's key.
const DEFAULT_JOB_BUDGET_USD: f64 = 25.0;

// ---------------------------------------------------------------------------
// Persistent state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    library_root: Option<String>,
    #[serde(default = "default_quality_tier")]
    quality_tier: String,
    #[serde(default = "default_challenge")]
    challenge_target: String,
    #[serde(default = "default_true")]
    verify_answers: bool,
    #[serde(default)]
    max_job_cost_microusd: Option<u64>,
    #[serde(default)]
    max_monthly_cost_microusd: Option<u64>,
    /// Questions delivered per calendar month, for the quota card.
    #[serde(default)]
    monthly_used: BTreeMap<String, u32>,
    /// Questions delivered per day. Groundwork for the hosted tier's fair
    /// use (at least 50/day always allowed); NEVER enforced for BYOK —
    /// the user's own provider bill is their limit.
    #[serde(default)]
    daily_used: BTreeMap<String, u32>,
    /// Which model provider generates questions: anthropic | gemini | openai.
    #[serde(default = "default_provider")]
    provider: String,
    /// Keep practice progress in a portable .whetstone file inside the
    /// notes folder so it survives reinstalls and moves between machines.
    #[serde(default = "default_true")]
    vault_state: bool,
    /// Model alias for the Claude Code provider: sonnet | opus | fable.
    /// Sonnet is plenty and burns subscription limits slowest.
    #[serde(default = "default_cli_model")]
    cli_model: String,
    /// Model alias for the Codex provider: sol | terra | luna.
    #[serde(default = "default_codex_model")]
    codex_model: String,
    #[serde(default = "default_ollama_model")]
    ollama_model: String,
    /// YAML frontmatter key holding a note's creation date (e.g.
    /// "created: 2026-07-15"); daily practice prioritizes recent notes
    /// because forgetting is steepest right after first exposure.
    #[serde(default = "default_created_key")]
    created_frontmatter_key: String,
}

fn default_created_key() -> String {
    "created".into()
}

fn default_provider() -> String {
    "anthropic".into()
}

fn default_cli_model() -> String {
    "sonnet".into()
}

fn default_ollama_model() -> String {
    "llama3.1".into()
}

fn default_codex_model() -> String {
    "terra".into()
}

fn default_quality_tier() -> String {
    "olympiad_studio".into()
}
fn default_challenge() -> String {
    "adaptive".into()
}
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_root: None,
            quality_tier: default_quality_tier(),
            challenge_target: default_challenge(),
            verify_answers: true,
            // Retained for config-file compatibility; no longer enforced
            // (the user-facing caps were removed).
            max_job_cost_microusd: None,
            max_monthly_cost_microusd: None,
            monthly_used: BTreeMap::new(),
            daily_used: BTreeMap::new(),
            provider: default_provider(),
            vault_state: true,
            cli_model: default_cli_model(),
            codex_model: default_codex_model(),
            ollama_model: default_ollama_model(),
            created_frontmatter_key: default_created_key(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NoteState {
    #[serde(default)]
    skill: f64,
    #[serde(default)]
    observations: u32,
    /// Unix seconds of the last practice touching this note.
    #[serde(default)]
    last_practiced: Option<f64>,
    #[serde(default)]
    interval_days: f64,
}

/// Lifetime provider usage across all jobs, for Settings' spend meter and
/// the monthly-ceiling guard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UsageTotals {
    #[serde(default)]
    calls: u64,
    #[serde(default)]
    estimated_cost_microusd: u64,
    #[serde(default)]
    uncertain_cost_microusd: u64,
    /// Committed spend (actual + uncertain) per "YYYY-MM" month.
    #[serde(default)]
    monthly_cost_microusd: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LearnerState {
    #[serde(default)]
    notes: BTreeMap<String, NoteState>,
    #[serde(default)]
    revision: u64,
}

struct RunningJob {
    job_id: String,
    requested: usize,
    ledger_path: PathBuf,
    inventory_path: PathBuf,
    /// Rel-paths per source sha (a grouped envelope spans several notes).
    rel_by_hash: BTreeMap<String, Vec<String>>,
    result: Arc<Mutex<Option<std::result::Result<Vec<CandidateQuestion>, String>>>>,
}

pub struct ServeContext {
    data_dir: PathBuf,
    config: Config,
    learner: LearnerState,
    /// Opaque frontend blob (practice history, streaks) that rides along
    /// in the portable vault file.
    app_state: Value,
    job: Option<RunningJob>,
    classroom: Option<crate::classroom::host::HostHandle>,
}

/// Everything a learner would grieve losing, in one portable file. It
/// deliberately excludes credentials and login state, which stay in the
/// keychain and Supabase.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VaultFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    learner: LearnerState,
    #[serde(default)]
    app: Value,
}

const VAULT_MAGIC: &[u8] = b"WSTN1";
const VAULT_KEY: &[u8] = b"whetstone-progress-v1";

/// Obfuscation, not encryption: the file is app bookkeeping, and a learner
/// hand-editing their own ratings would only sabotage their practice.
fn vault_encode(plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(VAULT_MAGIC.len() + plain.len());
    out.extend_from_slice(VAULT_MAGIC);
    out.extend(
        plain
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ VAULT_KEY[i % VAULT_KEY.len()]),
    );
    out
}

fn vault_decode(raw: &[u8]) -> Option<Vec<u8>> {
    let body = raw.strip_prefix(VAULT_MAGIC)?;
    Some(
        body.iter()
            .enumerate()
            .map(|(i, b)| b ^ VAULT_KEY[i % VAULT_KEY.len()])
            .collect(),
    )
}

fn total_observations(learner: &LearnerState) -> u64 {
    learner.notes.values().map(|n| n.observations as u64).sum()
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Per-platform application data directory.
fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Whetstone");
        }
    }
    dirs_home().join("Library/Application Support/Whetstone")
}

fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{}/{rest}", dirs_home().display());
    }
    path.to_owned()
}

fn load_json<T: Default + serde::de::DeserializeOwned>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn store_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl ServeContext {
    pub fn new(_mock: bool) -> Result<Self> {
        let data_dir = std::env::var_os("WHETSTONE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(default_data_dir);
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("creating {}", data_dir.display()))?;
        let config: Config = load_json(&data_dir.join("config.json"));
        let learner: LearnerState = load_json(&data_dir.join("learner.json"));
        let app_state: Value = load_json(&data_dir.join("app-state.json"));
        let mut context = Self {
            data_dir,
            config,
            learner,
            app_state,
            job: None,
            classroom: None,
        };
        context.adopt_vault_if_richer();
        Ok(context)
    }

    fn vault_path(&self) -> Option<PathBuf> {
        if !self.config.vault_state {
            return None;
        }
        let root = self.config.library_root.as_ref()?;
        let root = PathBuf::from(shellexpand_home(root));
        root.is_dir().then(|| root.join(".whetstone"))
    }

    /// Reinstalls and machine moves: whatever copy of the progress has seen
    /// more practice wins, so a stale file can never clobber fresh work.
    fn adopt_vault_if_richer(&mut self) {
        let Some(path) = self.vault_path() else { return };
        let Ok(raw) = std::fs::read(&path) else { return };
        let Some(plain) = vault_decode(&raw) else { return };
        let Ok(vault) = serde_json::from_slice::<VaultFile>(&plain) else { return };
        if total_observations(&vault.learner) > total_observations(&self.learner) {
            self.learner = vault.learner;
        }
        if self.app_state.is_null() && !vault.app.is_null() {
            self.app_state = vault.app;
        }
        let _ = self.persist();
    }

    /// One write path for everything durable: local mirrors always, plus
    /// the portable vault file when enabled.
    fn persist(&self) -> Result<()> {
        store_json(&self.data_dir.join("learner.json"), &self.learner)?;
        store_json(&self.data_dir.join("app-state.json"), &self.app_state)?;
        if let Some(path) = self.vault_path() {
            let vault = VaultFile {
                version: 1,
                learner: self.learner.clone(),
                app: self.app_state.clone(),
            };
            let encoded = vault_encode(&serde_json::to_vec(&vault)?);
            let tmp = path.with_extension("whetstone.tmp");
            std::fs::write(&tmp, encoded)?;
            std::fs::rename(&tmp, &path)?;
        }
        Ok(())
    }

    fn save_config(&self) -> Result<()> {
        store_json(&self.data_dir.join("config.json"), &self.config)
    }

    fn save_learner(&self) -> Result<()> {
        self.persist()
    }
}

pub async fn serve(mock: bool) -> Result<()> {
    let mut context = ServeContext::new(mock)?;
    eprintln!("whetstone serve: data dir {}", context.data_dir.display());
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let params = request.get("params").cloned().unwrap_or(json!({}));
        let reply = dispatch(&mut context, &method, params).await;
        let envelope = match reply {
            Ok(data) => json!({"id": id, "ok": true, "data": data}),
            Err(error) => {
                json!({"id": id, "ok": false, "error": humanize_error(&format!("{error:#}"))})
            }
        };
        println!("{envelope}");
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
    Ok(())
}

async fn dispatch(context: &mut ServeContext, method: &str, params: Value) -> Result<Value> {
    match method {
        "get_state" => Ok(state_snapshot(context)),
        "set_config" => set_config(context, &params),
        "scan_library" => scan_library(context),
        "mastery_status" => Ok(mastery_status(context)),
        "start_session" => start_session(context, &params),
        "preparation_status" => preparation_status(context, &params),
        "record_results" => record_results(context, &params),
        "record_attempts" => record_attempts(context, &params),
        "record_feedback" => record_feedback(context, &params),
        "sync_note_changes" => sync_note_changes(context),
        "get_app_state" => Ok(context.app_state.clone()),
        "set_app_state" => {
            context.app_state = params;
            context.persist()?;
            Ok(Value::Null)
        }
        "bank_status" => bank_status(context),
        "host_start" => host_start(context, params).await,
        "host_control" => {
            let action = params.get("action").and_then(Value::as_str).unwrap_or("");
            match &context.classroom {
                Some(host) => host.control(action),
                None => bail!("no classroom is running"),
            }
        }
        "host_status" => match &context.classroom {
            Some(host) => Ok(host.status()),
            None => bail!("no classroom is running"),
        },
        "host_stop" => {
            if let Some(host) = context.classroom.take() {
                host.stop();
            }
            Ok(Value::Null)
        }
        other => bail!("unknown method: {other}"),
    }
}

/// Start hosting a classroom quiz. Questions come either inline
/// ("questions": full objects, e.g. human-authored) or from a stored
/// session file ("session_id"), so AI sets replay with zero extra work.
async fn host_start(context: &mut ServeContext, params: Value) -> Result<Value> {
    if let Some(host) = context.classroom.take() {
        host.stop();
    }
    let mut questions: Vec<crate::classroom::QuizItem> = Vec::new();
    if let Some(list) = params.get("questions").and_then(Value::as_array) {
        for q in list {
            let stem = q.get("question_text").or_else(|| q.get("stem")).and_then(Value::as_str).unwrap_or("");
            let options: Vec<String> = q.get("options").and_then(Value::as_array).map(|a| {
                a.iter().filter_map(Value::as_str).map(str::to_owned).collect()
            }).unwrap_or_default();
            let answer_index = q.get("answer_index").and_then(Value::as_u64).unwrap_or(0) as u8;
            if stem.is_empty() || options.len() < 2 {
                bail!("each question needs question_text and at least two options");
            }
            let mut item = crate::classroom::QuizItem::default();
            item.stem = stem.to_owned();
            item.options = options;
            item.answer_index = answer_index;
            item.question_type = "mcq".into();
            item.figure_pdf = q.get("figure_pdf").and_then(Value::as_str).map(str::to_owned);
            questions.push(item);
        }
    } else if let Some(session) = params.get("session_id").and_then(Value::as_str) {
        let set: Value = serde_json::from_slice(&std::fs::read(
            context.data_dir.join("sessions").join(format!("{session}.json")),
        )?)?;
        let served = set.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
        for q in &served {
            let mut item = crate::classroom::QuizItem::default();
            item.stem = q.get("question_text").and_then(Value::as_str).unwrap_or("").to_owned();
            item.options = q.get("options").and_then(Value::as_array).map(|a| {
                a.iter().filter_map(Value::as_str).map(str::to_owned).collect()
            }).unwrap_or_default();
            let answer = q.get("correct_answer").and_then(Value::as_str).unwrap_or("");
            item.answer_index = item.options.iter().position(|o| o == answer).unwrap_or(0) as u8;
            item.question_type = "mcq".into();
            item.figure_pdf = q.get("figure_pdf").and_then(Value::as_str).map(str::to_owned);
            questions.push(item);
        }
    }
    if questions.is_empty() {
        bail!("host_start needs questions or a session_id");
    }
    let time_limit = params.get("time_limit_secs").and_then(Value::as_u64).unwrap_or(30) as u32;
    let port = params.get("port").and_then(Value::as_u64).unwrap_or(4870) as u16;
    if let Some(page) = params.get("join_page_html").and_then(Value::as_str) {
        *crate::classroom::host::JOIN_PAGE_OVERRIDE.lock().expect("page lock") = Some(page.to_owned());
    }
    let host = crate::classroom::host::start(questions, time_limit, port).await?;
    let addresses = local_ipv4_addresses();
    let reply = json!({
        "room_code": host.room_code,
        "port": host.port,
        "join_urls": addresses.iter().map(|ip| format!("http://{ip}:{}", host.port)).collect::<Vec<_>>(),
    });
    context.classroom = Some(host);
    Ok(reply)
}

/// Best-effort LAN address for the projector QR code: connect a UDP
/// socket outward (no packets are sent) and read the chosen local address.
fn local_ipv4_addresses() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                out.push(addr.ip().to_string());
            }
        }
    }
    if out.is_empty() {
        out.push("localhost".into());
    }
    out
}

// ---------------------------------------------------------------------------
// State and library
// ---------------------------------------------------------------------------

fn state_snapshot(context: &ServeContext) -> Value {
    let month = Utc::now().format("%Y-%m").to_string();
    let used = *context.config.monthly_used.get(&month).unwrap_or(&0);
    json!({
        "library_root": context.config.library_root,
        "backend": {
            "kind": "byok",
            "model": match context.config.provider.as_str() {
                "gemini" => "gemini-3.5-flash",
                "openai" => "gpt-5.6 (sol/terra/luna)",
                "claude-code" => "Claude Code (subscription)",
                "codex" => "Codex CLI (subscription)",
                "ollama" => "Ollama (local)",
                _ => "claude-sonnet-5",
            },
            "provider": context.config.provider
        },
        "tier": "pro",
        "verify_answers": context.config.verify_answers,
        "quota": {"tier": "pro", "month": month, "used": used, "limit": Value::Null, "remaining": Value::Null},
        "quality_tier": context.config.quality_tier,
        "challenge_target": context.config.challenge_target,
        "max_job_cost_microusd": context.config.max_job_cost_microusd,
        "max_monthly_cost_microusd": context.config.max_monthly_cost_microusd,
        "created_frontmatter_key": context.config.created_frontmatter_key,
        "vault_state": context.config.vault_state,
        "cli_model": context.config.cli_model,
        "codex_model": context.config.codex_model,
        "ollama_model": context.config.ollama_model
    })
}

fn set_config(context: &mut ServeContext, params: &Value) -> Result<Value> {
    if let Some(provider) = params.get("provider").and_then(Value::as_str) {
        // Refusing (not silently ignoring) lets the Settings picker show its
        // honest "Provider was not changed" error instead of reverting mutely.
        crate::anthropic::Provider::parse(provider)?;
        context.config.provider = provider.to_owned();
    }
    if let Some(root) = params.get("library_root").and_then(Value::as_str) {
        context.config.library_root = Some(shellexpand_home(root));
        // Pointing at a vault that already carries progress adopts it.
        context.adopt_vault_if_richer();
    }
    if let Some(alias) = params.get("cli_model").and_then(Value::as_str) {
        if ["sonnet", "opus", "fable", "haiku"].contains(&alias) {
            context.config.cli_model = alias.to_owned();
        }
    }
    if let Some(model) = params.get("ollama_model").and_then(Value::as_str) {
        if !model.trim().is_empty() {
            context.config.ollama_model = model.trim().to_owned();
        }
    }
    if let Some(alias) = params.get("codex_model").and_then(Value::as_str) {
        if ["sol", "terra", "luna"].contains(&alias) {
            context.config.codex_model = alias.to_owned();
        }
    }
    if let Some(enabled) = params.get("vault_state").and_then(Value::as_bool) {
        context.config.vault_state = enabled;
        if enabled {
            context.adopt_vault_if_richer();
            context.persist()?;
        }
    }
    if let Some(tier) = params.get("quality_tier").and_then(Value::as_str) {
        context.config.quality_tier = tier.to_owned();
    }
    if let Some(target) = params.get("challenge_target").and_then(Value::as_str) {
        context.config.challenge_target = target.to_owned();
    }
    if let Some(verify) = params.get("verify_answers").and_then(Value::as_bool) {
        context.config.verify_answers = verify;
    }
    if let Some(cap) = params.get("max_job_cost_microusd") {
        context.config.max_job_cost_microusd = cap.as_u64();
    }
    if let Some(cap) = params.get("max_monthly_cost_microusd") {
        context.config.max_monthly_cost_microusd = cap.as_u64();
    }
    if let Some(key) = params.get("created_frontmatter_key").and_then(Value::as_str) {
        let key = key.trim();
        context.config.created_frontmatter_key =
            if key.is_empty() { default_created_key() } else { key.to_owned() };
    }
    context.save_config()?;
    Ok(state_snapshot(context))
}

fn library_root(context: &ServeContext) -> Result<PathBuf> {
    let root = context
        .config
        .library_root
        .as_ref()
        .context("no notes folder is configured yet")?;
    let path = PathBuf::from(shellexpand_home(root));
    if !path.is_dir() {
        bail!("notes folder is not a directory: {}", path.display());
    }
    Ok(path)
}

fn note_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let supported = entry
            .path()
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| SUPPORTED.contains(&x.to_ascii_lowercase().as_str()))
            .unwrap_or(false);
        if !supported {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();
        if rel.split('/').any(|part| part.starts_with('.')) {
            continue;
        }
        files.push((rel, entry.into_path()));
    }
    files
}

fn note_title(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_owned()
}

fn scan_library(context: &ServeContext) -> Result<Value> {
    let root = library_root(context)?;
    let now = Utc::now().timestamp() as f64;
    let notes: Vec<Value> = note_files(&root)
        .into_iter()
        .map(|(rel, abs)| {
            let state = context.learner.notes.get(&rel).cloned().unwrap_or_default();
            let due_in_days = state.last_practiced.map(|last| {
                state.interval_days - (now - last) / 86_400.0
            });
            let text = if abs.extension().and_then(|x| x.to_str()) == Some("pdf") {
                None
            } else {
                std::fs::read_to_string(&abs).ok()
            };
            // The author's own "created:" frontmatter beats filesystem birth
            // time — vault syncs and clones reset the latter wholesale.
            let created_at = text
                .as_deref()
                .and_then(|t| frontmatter_created(t, &context.config.created_frontmatter_key))
                .or_else(|| {
                    std::fs::metadata(&abs)
                        .ok()
                        .and_then(|m| m.created().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                });
            let word_count = text
                .as_deref()
                .map(|t| t.split_whitespace().count())
                .unwrap_or(0);
            json!({
                "path": rel,
                "title": note_title(&rel),
                "skill": state.skill,
                "due_in_days": due_in_days,
                "word_count": word_count,
                "tags": [],
                "created_at": created_at
            })
        })
        .collect();
    Ok(Value::Array(notes))
}

/// When far more notes are selected than one session can cover, choose a
/// subset instead of flooding the pipeline with hundreds of envelopes:
/// weakness-weighted (low demonstrated skill), freshness-weighted (recently
/// created notes are least consolidated), folder-clustered (picking a note
/// raises its siblings' odds, so related topics land together and compose),
/// and genuinely random on top — a hundred-note selection must not quiz the
/// same topics every session.
fn select_session_notes(
    context: &ServeContext,
    root: &Path,
    rel_paths: Vec<String>,
    count: usize,
) -> Vec<String> {
    let cap = count.div_ceil(2).clamp(6, 16);
    if rel_paths.len() <= cap {
        return rel_paths;
    }
    let now = Utc::now().timestamp() as f64;
    let mut state = (Utc::now().timestamp_micros() as u64) | 1;
    let mut random = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 10_000) as f64 / 10_000.0
    };
    let mut weighted: Vec<(String, f64)> = rel_paths
        .into_iter()
        .map(|rel| {
            let skill_weight = match context.learner.notes.get(&rel) {
                Some(s) if s.observations > 0 => {
                    1.0 + (1.0 - (s.skill / 100.0).clamp(0.0, 1.0)) * 1.5
                }
                _ => 1.75, // never practiced: favored
            };
            let recency_weight = std::fs::read_to_string(root.join(&rel))
                .ok()
                .and_then(|t| frontmatter_created(&t, &context.config.created_frontmatter_key))
                .map(|created| {
                    let age_days = ((now - created) / 86_400.0).max(0.0);
                    1.0 + 1.2 * (-age_days / 7.0).exp()
                })
                .unwrap_or(1.0);
            (rel, skill_weight * recency_weight)
        })
        .collect();
    let mut picked = Vec::new();
    while picked.len() < cap && !weighted.is_empty() {
        let total: f64 = weighted.iter().map(|(_, w)| w).sum();
        let mut target = random() * total;
        let mut chosen = weighted.len() - 1;
        for (index, (_, weight)) in weighted.iter().enumerate() {
            if target <= *weight {
                chosen = index;
                break;
            }
            target -= weight;
        }
        let (rel, _) = weighted.remove(chosen);
        let folder = rel.rsplit_once('/').map(|(d, _)| d.to_owned()).unwrap_or_default();
        for (other, weight) in weighted.iter_mut() {
            let other_folder = other.rsplit_once('/').map(|(d, _)| d).unwrap_or_default();
            if other_folder == folder {
                *weight *= 1.6;
            }
        }
        picked.push(rel);
    }
    picked
}

/// Progress line for the practice button: learners think in questions,
/// not pipeline phases ("seeding", "verified") — never leak those.
fn friendly_progress(accepted: usize, requested: usize) -> String {
    if accepted == 0 {
        "Reading your notes and drafting questions…".to_owned()
    } else {
        format!("{accepted} of {requested} questions ready…")
    }
}

/// Translate raw provider/pipeline failures into something a learner can
/// act on. Unrecognized errors pass through with a plain-language lead-in.
fn humanize_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("credit balance is too low") {
        return "Your Anthropic account is out of credits. Add credits at console.anthropic.com, then start this practice again. Finished questions are kept."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let auth_failure = lower.contains("invalid x-api-key")
        || lower.contains("authentication_error")
        || lower.contains("unauthorized")
        || lower.contains("not logged in")
        || lower.contains("login")
        || lower.contains("401");
    if auth_failure && lower.contains("codex") {
        return "The Codex CLI is not signed in. Run codex in a terminal, sign in with your ChatGPT account, then start this practice again."
            .to_owned();
    }
    if auth_failure && lower.contains("claude") {
        return "Claude Code is not signed in. Run claude in a terminal, sign in, then start this practice again."
            .to_owned();
    }
    if auth_failure {
        return "Your API key was rejected by the provider. Check it in Settings → Advanced."
            .to_owned();
    }
    if lower.contains("rate limit") || lower.contains("rate_limit") || lower.contains("429") {
        return "The provider is rate-limiting requests. Wait a minute, then start this practice again. Finished questions are kept."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if lower.contains("overloaded") || lower.contains("529") {
        return "The provider is briefly overloaded. Try again in a minute. Finished questions are kept."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if lower.contains("model_format_error")
        || lower.contains("missing field")
        || lower.contains("decoding")
        || lower.contains("parsing structured response")
        || lower.contains("invalid type")
        || lower.contains("omitted items array")
        || lower.contains("omitted usage")
    {
        return "The model replied in an unexpected format. Start this practice again and it will pick up where it left off."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if lower.contains("budget gate stopped") || lower.contains("budget invariant") {
        return "Whetstone's safety stop halted this preparation before it could overspend. Start the practice again to continue."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if lower.contains("requires a newer version") || lower.contains("model_not_found") {
        return "The selected model needs a newer version of that command line tool. Update it, or pick another model in Settings."
            .to_owned();
    }
    if lower.contains("cli_missing") {
        return "That command line tool is not installed on this computer. Install it and sign in, or pick another provider in Settings."
            .to_owned();
    }
    if lower.contains("no api key configured") || lower.contains("api_key is empty") {
        return "No API key is set for the selected provider. Add one in Settings → Advanced."
            .to_owned();
    }
    if lower.contains("refusing partial success") || lower.contains("quality gate") {
        return "Some questions kept failing quality checks even after automatic retries. The selected notes may be too thin. Try fewer questions or add related notes."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if lower.contains("timed out")
        || lower.contains("connection error")
        || lower.contains("transport_error")
        || lower.contains("dns error")
    {
        return "Whetstone could not reach the provider. Check your connection, then start this practice again. Progress is saved."
            .split_whitespace().collect::<Vec<_>>().join(" ");
    }
    // Heuristic: hand-written user-facing sentences pass through; anything
    // that smells like internals gets a plain-language wrapper.
    let technical = ["error", "failed", "status", "schema", "json", "http", "expect", "panic"]
        .iter()
        .any(|marker| lower.contains(marker));
    if technical {
        let detail: String = raw.chars().take(180).collect();
        return format!(
            "Something went wrong while preparing questions. Starting the practice again usually fixes it. (Detail: {detail})"
        )
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    }
    raw.to_owned()
}

/// Pull a creation timestamp out of a note's YAML frontmatter. Tolerant by
/// design: quoted or bare values, date or date-time, wiki-linked dates.
fn frontmatter_created(text: &str, key: &str) -> Option<f64> {
    let body = text
        .trim_start_matches('\u{feff}')
        .strip_prefix("---")?;
    let end = body.find("\n---")?;
    for line in body[..end].lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        if !k.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let value = v
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim_matches(|c| c == '[' || c == ']')
            .trim();
        return parse_created_date(value);
    }
    None
}

fn parse_created_date(value: &str) -> Option<f64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(dt.timestamp() as f64);
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Some(dt.and_utc().timestamp() as f64);
        }
    }
    for format in ["%Y-%m-%d", "%d-%m-%Y", "%d/%m/%Y", "%Y/%m/%d"] {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(value, format) {
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp() as f64);
        }
    }
    None
}

fn mastery_status(context: &ServeContext) -> Value {
    let entries: Vec<Value> = context
        .learner
        .notes
        .iter()
        .map(|(rel, state)| {
            let label = if state.observations == 0 {
                "New"
            } else if state.skill < 45.0 {
                "Shaky"
            } else if state.skill < 65.0 {
                "Developing"
            } else if state.skill < 82.0 {
                "Solid"
            } else {
                "Strong"
            };
            let confidence = match state.observations {
                0..=4 => "low",
                5..=19 => "medium",
                _ => "high",
            };
            json!({
                "source_path": rel,
                "label": label,
                "confidence": confidence,
                "observations": state.observations,
                "latent_mean": state.skill,
                "uncertainty": (30.0 - state.observations as f64).max(5.0),
                "next_challenge": context.config.challenge_target
            })
        })
        .collect();
    Value::Array(entries)
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

fn start_session(context: &mut ServeContext, params: &Value) -> Result<Value> {
    // Idempotent attach: a duplicate start while a job runs reports that job
    // instead of double-spending.
    if let Some(job) = &context.job {
        if job.result.lock().expect("job lock").is_none() {
            return job_response(context);
        }
    }
    let root = library_root(context)?;
    let rel_paths: Vec<String> = params
        .get("note_paths")
        .and_then(Value::as_array)
        .context("start_session requires note_paths")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if rel_paths.is_empty() {
        bail!("select at least one note");
    }
    let count = params
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 60) as usize;
    let api_key = params
        .get("api_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();

    let combine_topics = params
        .get("combine_topics")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let rel_paths = select_session_notes(context, &root, rel_paths, count);
    let abs_paths: Vec<PathBuf> = rel_paths.iter().map(|rel| root.join(rel)).collect();
    if abs_paths.iter().any(|p| p.extension().and_then(|x| x.to_str()) == Some("pdf"))
        && !matches!(context.config.provider.as_str(), "anthropic" | "gemini")
    {
        bail!(
            "PDF notes need a provider that reads documents natively. Switch to Anthropic or Google in Settings, or deselect the PDF topics."
        );
    }
    let loaded = collect_sources(&abs_paths).context("source preflight failed")?;
    let (sources, mut rel_by_hash) = compose_envelopes(loaded, &root, combine_topics)?;

    // Priority-ranked spare topics (daily practice sends its unpicked
    // remainder): composed up front so a quality-gate retry can widen the
    // material pool instead of hammering the same thin notes.
    let fallback_rels: Vec<String> = params
        .get("fallback_paths")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    let fallback_sources = if fallback_rels.is_empty() {
        Vec::new()
    } else {
        let abs: Vec<PathBuf> = fallback_rels.iter().map(|rel| root.join(rel)).collect();
        match collect_sources(&abs).and_then(|loaded| compose_envelopes(loaded, &root, combine_topics)) {
            Ok((extra_sources, extra_rels)) => {
                rel_by_hash.extend(extra_rels);
                extra_sources
            }
            Err(error) => {
                eprintln!("  fallback topics unavailable: {error:#}");
                Vec::new()
            }
        }
    };

    // Per-envelope learner level (0 weak … 1 strong) from the member notes
    // the learner has actually practiced; unknown topics stay neutral.
    let mut skill_by_hash = BTreeMap::new();
    for (hash, rels) in &rel_by_hash {
        let levels: Vec<f64> = rels
            .iter()
            .filter_map(|rel| context.learner.notes.get(rel))
            .filter(|state| state.observations > 0)
            .map(|state| (state.skill / 100.0).clamp(0.0, 1.0))
            .collect();
        if !levels.is_empty() {
            skill_by_hash.insert(hash.clone(), levels.iter().sum::<f64>() / levels.len() as f64);
        }
    }

    let results_dir = context.data_dir.join("results");
    std::fs::create_dir_all(&results_dir)?;
    // User-facing spending caps were removed (the user's provider bill is
    // the real limit); a wide fixed guard remains purely as a runaway stop.
    let budget_usd = DEFAULT_JOB_BUDGET_USD;
    let pipeline_config = PipelineConfig {
        count,
        budget_usd,
        // The requested model; non-Anthropic providers resolve the actual
        // tier per call from effort. Distinct names also key the cache, so
        // switching providers never serves another provider's cached items.
        author_model: match context.config.provider.as_str() {
            "gemini" => "gemini-3.5-flash".into(),
            "openai" => "gpt-5.6-terra".into(),
            "ollama" => context.config.ollama_model.clone(),
            "claude-code" => format!("claude-code-{}", context.config.cli_model),
            "codex" => format!("codex-cli-{}", context.config.codex_model),
            _ => "claude-sonnet-5".into(),
        },
        validator_model: match context.config.provider.as_str() {
            "gemini" => "gemini-3.5-flash".into(),
            "openai" => "gpt-5.6-terra".into(),
            "ollama" => context.config.ollama_model.clone(),
            "claude-code" => format!("claude-code-{}", context.config.cli_model),
            "codex" => format!("codex-cli-{}", context.config.codex_model),
            _ => "claude-sonnet-5".into(),
        },
        max_attempts: 2,
        quality_tier: context.config.quality_tier.clone(),
        ledger_path: results_dir.join("whetstone-job.json"),
        inventory_path: results_dir.join("validated-inventory.json"),
        cache_dir: context.data_dir.join("cache"),
        skill_by_hash,
    };
    // A stale inventory from the previous job must never be reported as this
    // job's progress.
    write_inventory_checkpoint(&pipeline_config.inventory_path, &[], count, false)?;
    let client = AnthropicClient::from_provider(&context.config.provider, api_key)?;
    let ledger = Arc::new(Mutex::new(new_ledger(&pipeline_config, &sources)));
    let job_id = ledger.lock().expect("ledger lock").job_id.clone();
    write_ledger(
        &pipeline_config.ledger_path,
        &ledger.lock().expect("ledger lock"),
    )?;

    let result: Arc<Mutex<Option<std::result::Result<Vec<CandidateQuestion>, String>>>> =
        Arc::new(Mutex::new(None));
    let done = result.clone();
    let output_path = results_dir.join("questions.md");
    let totals_path = context.data_dir.join("usage-totals.json");
    tokio::spawn(async move {
        // A quality-gate shortfall is OUR problem, not the learner's: rerun
        // with a larger attempt allowance. Earlier attempts replay from the
        // cache for pennies; the added attempts author fresh variants, so a
        // retry genuinely differs instead of deterministically re-failing.
        let mut pipeline_config = pipeline_config;
        let mut sources = sources;
        let mut outcome = run_pipeline(&client, &pipeline_config, &sources, &[], &ledger).await;
        for retry in 0..2 {
            let shortfall = matches!(
                &outcome,
                Err(error) if error.to_string().contains("refusing partial success")
            );
            if !shortfall {
                break;
            }
            pipeline_config.max_attempts += 2;
            // Second retry: the selected notes may simply be too thin —
            // widen the material pool with the priority-ranked spares.
            if retry == 1 && !fallback_sources.is_empty() {
                eprintln!(
                    "  quality gate shortfall persists: widening with {} fallback topic(s)",
                    fallback_sources.len()
                );
                sources.extend(fallback_sources.clone());
            }
            eprintln!(
                "  quality gate shortfall: retrying with {} attempts",
                pipeline_config.max_attempts
            );
            outcome = run_pipeline(&client, &pipeline_config, &sources, &[], &ledger).await;
        }
        {
            // Spend happened whether or not the job succeeded: fold this
            // job's provider usage into the lifetime totals exactly once.
            let guard = ledger.lock().expect("ledger lock");
            let mut totals: UsageTotals = load_json(&totals_path);
            totals.calls += guard.calls.len() as u64;
            totals.estimated_cost_microusd += (guard.actual_spend_usd * 1_000_000.0) as u64;
            totals.uncertain_cost_microusd += (guard.uncertain_spend_usd * 1_000_000.0) as u64;
            let committed = (guard.lifetime_committed_spend() * 1_000_000.0) as u64;
            let month = Utc::now().format("%Y-%m").to_string();
            *totals.monthly_cost_microusd.entry(month).or_insert(0) += committed;
            let _ = store_json(&totals_path, &totals);
        }
        let settled = match outcome {
            Ok(items) => {
                let mut guard = ledger.lock().expect("ledger lock");
                guard.status = "complete".into();
                guard.accepted_count = items.len();
                let write = write_outputs(&output_path, &items, &mut guard)
                    .and_then(|()| write_ledger(&pipeline_config.ledger_path, &guard));
                match write {
                    Ok(()) => Ok(items),
                    Err(error) => Err(format!("writing outputs failed: {error:#}")),
                }
            }
            Err(error) => {
                let mut guard = ledger.lock().expect("ledger lock");
                if guard.status != "incomplete_quality_gate" {
                    guard.status = "failed_closed".into();
                }
                let _ = write_ledger(&pipeline_config.ledger_path, &guard);
                Err(format!("{error:#}"))
            }
        };
        *done.lock().expect("job lock") = Some(settled);
    });

    context.job = Some(RunningJob {
        job_id,
        requested: count,
        ledger_path: results_dir.join("whetstone-job.json"),
        inventory_path: results_dir.join("validated-inventory.json"),
        rel_by_hash,
        result,
    });
    job_response(context)
}

/// Composability: notes big enough to stand alone become their own
/// envelopes; small atomic notes are grouped per folder into combined
/// envelopes so moves can compose ACROSS notes and a tiny note never
/// kills the selection. Returns the effective sources plus the rel-path
/// lineage each envelope covers.
fn compose_envelopes(
    loaded: Vec<crate::model::SourceDocument>,
    root: &Path,
    combine_all: bool,
) -> Result<(Vec<crate::model::SourceDocument>, BTreeMap<String, Vec<String>>)> {
    use crate::model::{SourceDocument, SourcePayload};
    use sha2::{Digest, Sha256};
    const SOLO_MIN_CHARS: usize = 2_000;
    const BUNDLE_TARGET_CHARS: usize = 60_000;

    let rel_of = |source: &SourceDocument| -> String {
        source
            .path
            .strip_prefix(root)
            .unwrap_or(&source.path)
            .to_string_lossy()
            .to_string()
    };
    let mut sources = Vec::new();
    let mut rel_by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut small_by_folder: BTreeMap<String, Vec<SourceDocument>> = BTreeMap::new();
    for source in loaded {
        let native_pdf = matches!(source.payload, crate::model::SourcePayload::Pdf(_))
            && source.extracted_text.trim().is_empty();
        if native_pdf
            || (!combine_all && source.extracted_text.trim().chars().count() >= SOLO_MIN_CHARS)
        {
            let rel = rel_of(&source);
            rel_by_hash.insert(source.sha256.clone(), vec![rel.clone()]);
            let mut source = source;
            source.note_paths = vec![rel];
            sources.push(source);
        } else {
            // combine_all (daily practice) throws every note into ONE pool
            // regardless of size or folder: interleaving topics inside an
            // envelope is what lets a single question span several notes.
            let folder = if combine_all {
                String::new()
            } else {
                let rel = rel_of(&source);
                rel.rsplit_once('/').map(|(dir, _)| dir.to_owned()).unwrap_or_default()
            };
            small_by_folder.entry(folder).or_default().push(source);
        }
    }
    for (folder, notes) in small_by_folder {
        let mut bundle_notes: Vec<&SourceDocument> = Vec::new();
        let mut bundle_chars = 0usize;
        let flush = |bundle: &mut Vec<&SourceDocument>,
                         sources: &mut Vec<SourceDocument>,
                         rel_by_hash: &mut BTreeMap<String, Vec<String>>| {
            if bundle.is_empty() {
                return;
            }
            let mut text = String::new();
            let mut rels = Vec::new();
            for note in bundle.iter() {
                let rel = rel_of(note);
                text.push_str(&format!("\n\n# Note: {rel}\n\n"));
                text.push_str(&note.extracted_text);
                rels.push(rel);
            }
            if text.trim().chars().count() < 200 {
                eprintln!("  skipping group '{folder}': too little text even combined");
                bundle.clear();
                return;
            }
            let sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
            let name = if folder.is_empty() {
                format!("{} notes (grouped)", bundle.len())
            } else {
                format!("{folder} ({} notes)", bundle.len())
            };
            let domain = crate::source::classify_domain(&name, &text);
            rel_by_hash.insert(sha256.clone(), rels);
            sources.push(SourceDocument {
                path: root.join(&name),
                name,
                note_paths: rel_by_hash.get(&sha256).cloned().unwrap_or_default(),
                media_type: "text/plain".into(),
                sha256,
                extracted_text: text.clone(),
                page_count: None,
                domain,
                payload: SourcePayload::Text(text),
            });
            bundle.clear();
        };
        for note in &notes {
            let chars = note.extracted_text.chars().count();
            if bundle_chars + chars > BUNDLE_TARGET_CHARS && !bundle_notes.is_empty() {
                flush(&mut bundle_notes, &mut sources, &mut rel_by_hash);
                bundle_chars = 0;
            }
            bundle_notes.push(note);
            bundle_chars += chars;
        }
        flush(&mut bundle_notes, &mut sources, &mut rel_by_hash);
    }
    if sources.is_empty() {
        bail!("the selected notes contain too little text to author from, even combined");
    }
    Ok((sources, rel_by_hash))
}

fn preparation_status(context: &mut ServeContext, params: &Value) -> Result<Value> {
    let requested_id = params
        .get("job_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match &context.job {
        Some(job) if job.job_id == requested_id => job_response(context),
        // After a sidecar restart the in-memory job is gone; a completed set
        // may still exist on disk under this id.
        _ => {
            let path = context.data_dir.join("sessions").join(format!("{requested_id}.json"));
            if let Ok(bytes) = std::fs::read(&path) {
                let set: Value = serde_json::from_slice(&bytes)?;
                return Ok(json!({"status": "ready", "question_set": set, "job": Value::Null}));
            }
            Ok(json!({
                "status": "failed",
                "question_set": Value::Null,
                "job": {
                    "job_id": requested_id, "state": "failed", "phase": Value::Null,
                    "completed_units": 0, "total_units": Value::Null,
                    "message": Value::Null,
                    "error": "The preparation job did not survive a backend restart. Start a new session; caching makes the retry cheap.",
                    "known_cost_microusd": 0, "calls_without_cost": 0
                }
            }))
        }
    }
}

fn job_response(context: &mut ServeContext) -> Result<Value> {
    let Some(job) = &context.job else {
        bail!("no preparation job is active");
    };
    let ledger: Option<JobLedger> = std::fs::read(&job.ledger_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let inventory: Option<ValidatedInventory> = std::fs::read(&job.inventory_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let accepted = inventory.as_ref().map(|inv| inv.items.len()).unwrap_or(0);
    let cost_microusd = ledger
        .as_ref()
        .map(|l| (l.committed_spend() * 1_000_000.0) as u64)
        .unwrap_or(0);
    let phase = ledger
        .as_ref()
        .map(|l| l.status.clone())
        .unwrap_or_else(|| "starting".into());
    let settled = job.result.lock().expect("job lock").clone();

    match settled {
        None => {
            // Early-ready streaming: once a starter pool exists the session
            // begins; the client keeps polling and appends the rest as they
            // are accepted. Adaptive serving needs a spread, so wait for 3.
            if accepted >= 3 {
                let items = inventory.map(|inv| inv.items).unwrap_or_default();
                let set = question_set_partial(&job.job_id, &items, &job.rel_by_hash, job.requested);
                return Ok(json!({
                    "status": "ready",
                    "question_set": set,
                    "job": {
                        "job_id": job.job_id, "state": "preparing", "phase": phase,
                        "completed_units": accepted as u64,
                        "total_units": job.requested as u64,
                        "message": format!("{accepted} of {} ready · more on the way", job.requested),
                        "error": Value::Null,
                        "known_cost_microusd": cost_microusd,
                        "calls_without_cost": 0
                    }
                }));
            }
            Ok(json!({
                "status": "preparing",
                "question_set": Value::Null,
                "job": {
                    "job_id": job.job_id, "state": "preparing", "phase": phase,
                    "completed_units": accepted as u64,
                    "total_units": job.requested as u64,
                    "message": friendly_progress(accepted, job.requested),
                    "error": Value::Null,
                    "known_cost_microusd": cost_microusd,
                    "calls_without_cost": 0
                }
            }))
        }
        Some(Err(reason)) => Ok(json!({
            "status": "failed",
            "question_set": Value::Null,
            "job": {
                "job_id": job.job_id, "state": "failed", "phase": phase,
                "completed_units": accepted as u64,
                "total_units": job.requested as u64,
                "message": Value::Null,
                "error": humanize_error(&reason),
                "known_cost_microusd": cost_microusd,
                "calls_without_cost": 0
            }
        })),
        Some(Ok(items)) => {
            let set =
                question_set_partial(&job.job_id, &items, &job.rel_by_hash, job.requested);
            store_json(
                &context.data_dir.join("sessions").join(format!("{}.json", job.job_id)),
                &set,
            )?;
            let month = Utc::now().format("%Y-%m").to_string();
            *context.config.monthly_used.entry(month).or_insert(0) += items.len() as u32;
            let day = Utc::now().format("%Y-%m-%d").to_string();
            *context.config.daily_used.entry(day).or_insert(0) += items.len() as u32;
            // Keep a rolling two weeks; the map must not grow forever.
            while context.config.daily_used.len() > 14 {
                let oldest = context.config.daily_used.keys().next().cloned();
                if let Some(oldest) = oldest {
                    context.config.daily_used.remove(&oldest);
                }
            }
            let _ = context.save_config();
            Ok(json!({"status": "ready", "question_set": set, "job": Value::Null}))
        }
    }
}

fn question_set(
    job_id: &str,
    items: &[CandidateQuestion],
    rel_by_hash: &BTreeMap<String, Vec<String>>,
) -> Value {
    // Sessions climb the ladder: serve questions in ascending measured
    // rating, so later questions are harder than earlier ones.
    let mut items: Vec<&CandidateQuestion> = items.iter().collect();
    items.sort_by(|a, b| {
        let ra = a.elo.as_ref().map(|e| e.rating).unwrap_or(0.0);
        let rb = b.elo.as_ref().map(|e| e.rating).unwrap_or(0.0);
        ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let items = items;
    let questions: Vec<Value> = items.iter().map(|item| {
        let source_paths = item_source_paths(item, rel_by_hash);
        let source_path = source_paths
            .first()
            .cloned()
            .unwrap_or_else(|| item.source_name.clone());
        let difficulty = match item.moves.rung {
            1 => "easy",
            2 => "medium",
            3 => "hard",
            _ => "very_hard",
        };
        // Ratings drive adaptive ordering invisibly; learners see only the
        // trust tag, on its own line at the very end.
        // Verification detail is bookkeeping, not learning material — it
        // lives in the bank stats, never in the learner's explanation.
        let explanation = format!(
            "{}\n\n{}",
            item.decisive_insight.trim(),
            item.worked_solution.trim()
        );
        let correct_answer = match item.question_type.as_str() {
            "integer" | "decimal" => item.numeric_answer.trim().to_owned(),
            _ => item
                .options
                .get(item.answer_index as usize)
                .cloned()
                .unwrap_or_default(),
        };
        let correct_answers: Value = if item.question_type == "multi" {
            json!(
                item.correct_indices
                    .iter()
                    .filter_map(|i| item.options.get(*i as usize).cloned())
                    .collect::<Vec<_>>()
            )
        } else {
            Value::Null
        };
        json!({
            "id": item.id,
            "type": item.question_type,
            "question_text": item.stem,
            "options": item.options,
            "explanation": explanation.trim(),
            "correct_answer": correct_answer,
            "correct_answers": correct_answers,
            "source_title": note_title(&source_path),
            "source_path": source_path,
            "source_paths": source_paths,
            "difficulty": difficulty,
            "rating": item.elo.as_ref().map(|state| state.rating),
            "figure_svg": item.diagram_svg,
            "figure_svg_dark": item.diagram_svg_dark,
            "figure_pdf": item.diagram_pdf,
            "figure_pdf_dark": item.diagram_pdf_dark,
            "figure_placement": item.figure_placement
        })
    }).collect();
    let item_source_paths: BTreeMap<String, Vec<String>> = items
        .iter()
        .map(|item| {
            let rels = item_source_paths(item, rel_by_hash);
            (item.id.clone(), rels)
        })
        .collect();
    json!({
        "set_id": job_id,
        "created_at": Utc::now().to_rfc3339(),
        "catalog_version": 1,
        "questions": questions,
        "generation_manifest": {
            "engine": "take2_moves_oracle",
            // The frontend's adaptive serving keys on this mode: harder after
            // a correct answer, easier after a miss, from the rated pool.
            "serving_mode": "adaptive_pool",
            "generated_count": items.len(),
            "conceptual_count": items.iter().filter(|i| i.verification.verdict != "proved").count(),
            "requested_count": items.len(),
            "item_source_paths": item_source_paths,
            "warnings": []
        }
    })
}

fn item_source_paths(
    item: &CandidateQuestion,
    rel_by_hash: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    if !item.source_paths.is_empty() {
        return item.source_paths.clone();
    }
    // Compatibility for accepted inventory/cache entries written before
    // item-level provenance existed.
    rel_by_hash
        .get(&item.source_hash)
        .cloned()
        .unwrap_or_else(|| vec![item.source_name.clone()])
}

/// A mid-generation snapshot of the pool: same shape, marked with the true
/// requested count so the client knows more questions are coming.
fn question_set_partial(
    job_id: &str,
    items: &[CandidateQuestion],
    rel_by_hash: &BTreeMap<String, Vec<String>>,
    requested: usize,
) -> Value {
    // Never deliver more than the learner asked for: retries and widened
    // make-up passes can leave a surplus in the inventory, and every extra
    // item would otherwise ride along into the session.
    let capped = &items[..items.len().min(requested.max(1))];
    let mut set = question_set(job_id, capped, rel_by_hash);
    if let Some(manifest) = set
        .get_mut("generation_manifest")
        .and_then(Value::as_object_mut)
    {
        manifest.insert("requested_count".into(), json!(requested));
    }
    set
}

// ---------------------------------------------------------------------------
// Evidence recording
// ---------------------------------------------------------------------------

fn record_results(context: &mut ServeContext, params: &Value) -> Result<Value> {
    let results = params
        .get("results")
        .and_then(Value::as_array)
        .context("record_results requires results")?;
    let now = Utc::now().timestamp() as f64;
    let mut deltas = Vec::new();
    for entry in results {
        let Some(rel) = entry.get("note_path").and_then(Value::as_str) else {
            continue;
        };
        let correct = entry.get("correct").and_then(Value::as_u64).unwrap_or(0) as f64;
        let total = entry.get("total").and_then(Value::as_u64).unwrap_or(0) as f64;
        if total <= 0.0 {
            continue;
        }
        let state = context.learner.notes.entry(rel.to_owned()).or_default();
        let before = state.skill;
        let accuracy = correct / total;
        state.skill = (state.skill + (accuracy - 0.5) * 12.0).clamp(0.0, 100.0);
        state.observations += total as u32;
        state.last_practiced = Some(now);
        state.interval_days = if accuracy >= 0.7 {
            (state.interval_days * 2.0).clamp(3.0, 60.0)
        } else {
            1.0
        };
        deltas.push(json!({"note_path": rel, "before": before, "after": state.skill}));
    }
    context.learner.revision += 1;
    context.save_learner()?;
    Ok(json!({"deltas": deltas}))
}

fn record_attempts(context: &ServeContext, params: &Value) -> Result<Value> {
    let attempts = params
        .get("attempts")
        .and_then(Value::as_array)
        .context("record_attempts requires attempts")?;
    let line = json!({
        "at": Utc::now().to_rfc3339(),
        "session_id": params.get("session_id"),
        "attempts": attempts
    });
    append_jsonl(&context.data_dir.join("attempts.jsonl"), &line)?;
    Ok(json!({"updated": attempts.len(), "missing": Value::Null}))
}

fn record_feedback(context: &ServeContext, params: &Value) -> Result<Value> {
    let line = json!({"at": Utc::now().to_rfc3339(), "feedback": params});
    append_jsonl(&context.data_dir.join("feedback.jsonl"), &line)?;
    Ok(json!({"updated": true}))
}

fn sync_note_changes(context: &mut ServeContext) -> Result<Value> {
    context.learner.revision += 1;
    context.save_learner()?;
    Ok(json!({
        "changed": 0, "removed": 0, "events": 0,
        "requires_revalidation": 0, "ambiguities": 0,
        "revision": context.learner.revision
    }))
}

/// Settings' bank card: banked questions across all delivered sessions plus
/// lifetime provider usage.
fn bank_status(context: &ServeContext) -> Result<Value> {
    let totals: UsageTotals = load_json(&context.data_dir.join("usage-totals.json"));
    let mut items = 0usize;
    let mut verified = 0usize;
    if let Ok(entries) = std::fs::read_dir(context.data_dir.join("sessions")) {
        for entry in entries.flatten() {
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            let Ok(set) = serde_json::from_slice::<Value>(&bytes) else { continue };
            let Some(questions) = set.get("questions").and_then(Value::as_array) else { continue };
            items += questions.len();
            verified += questions
                .iter()
                .filter(|q| {
                    q.get("explanation")
                        .and_then(Value::as_str)
                        .map(|text| text.contains("machine-verified"))
                        .unwrap_or(false)
                })
                .count();
        }
    }
    let observations = std::fs::read_to_string(context.data_dir.join("attempts.jsonl"))
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .map(|entry| {
                    entry
                        .get("attempts")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0)
                })
                .sum::<usize>()
        })
        .unwrap_or(0);
    Ok(json!({
        "items": items,
        "observations": observations,
        "by_lifecycle": {
            "MachineVerified": verified,
            "PersonallyObserved": items - verified
        },
        "usage": {
            "calls": totals.calls,
            "by_phase": Value::Null,
            "estimated_cost_microusd": totals.estimated_cost_microusd,
            "uncertain_cost_microusd": totals.uncertain_cost_microusd
        }
    }))
}

fn append_jsonl(path: &Path, line: &Value) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{humanize_error, question_set, question_set_partial};
    use std::collections::BTreeMap;

    #[test]
    fn delivery_never_exceeds_the_requested_count() {
        let items: Vec<crate::model::CandidateQuestion> = (0..12)
            .map(|i| {
                let mut item = crate::model::CandidateQuestion::default();
                item.id = format!("q{i}");
                item
            })
            .collect();
        let set = question_set_partial("job", &items, &BTreeMap::new(), 10);
        let served = set["questions"].as_array().expect("questions array");
        assert_eq!(served.len(), 10, "surplus inventory must not ride along");
    }

    #[test]
    fn delivered_mapping_prefers_item_provenance_over_group_envelope() {
        let mut item = crate::model::CandidateQuestion::default();
        item.id = "q1".into();
        item.source_name = "6 notes (grouped)".into();
        item.source_hash = "bundle".into();
        item.source_paths = vec!["algorithms/interval-sweep.md".into()];
        let rels = BTreeMap::from([(
            "bundle".into(),
            vec![
                "algorithms/interval-sweep.md".into(),
                "physics/measurement.md".into(),
            ],
        )]);

        let set = question_set("job", &[item], &rels);
        assert_eq!(
            set["generation_manifest"]["item_source_paths"]["q1"],
            serde_json::json!(["algorithms/interval-sweep.md"])
        );
        assert_eq!(set["questions"][0]["source_title"], "interval-sweep");
        assert_eq!(
            set["questions"][0]["source_path"],
            "algorithms/interval-sweep.md"
        );

        let mut legacy = crate::model::CandidateQuestion::default();
        legacy.id = "old".into();
        legacy.source_hash = "bundle".into();
        let legacy_set = question_set("legacy", &[legacy], &rels);
        assert_eq!(
            legacy_set["generation_manifest"]["item_source_paths"]["old"],
            serde_json::json!([
                "algorithms/interval-sweep.md",
                "physics/measurement.md"
            ])
        );
    }

    #[test]
    fn provider_failures_reach_the_learner_in_plain_language() {
        let raw = "counting tokens for seeds:Arrays versus pointers.md: token-count endpoint \
                   returned 400 Bad Request: Your credit balance is too low to access the \
                   Anthropic API.";
        let friendly = humanize_error(raw);
        assert!(friendly.contains("out of credits"), "{friendly}");
        assert!(!friendly.contains("token-count"), "{friendly}");

        let friendly = humanize_error("decoding seed: missing field `seed_id`");
        assert!(friendly.contains("unexpected format"), "{friendly}");
        assert!(!friendly.contains("seed_id"), "{friendly}");

        let friendly = humanize_error(
            "budget gate stopped author:x: worst-case call reservation $2.10 exceeds $0.90 remaining",
        );
        assert!(friendly.contains("safety stop"), "{friendly}");
    }

    #[test]
    fn technical_leftovers_get_a_plain_wrapper_but_sentences_pass_through() {
        // Unmatched but technical: wrapped with a readable lead-in.
        let friendly = humanize_error("serde panic: expected value at line 1 column 2");
        assert!(friendly.starts_with("Something went wrong"), "{friendly}");
        // Hand-written user-facing text is not mangled.
        assert_eq!(humanize_error("select at least one note"), "select at least one note");
    }
}
