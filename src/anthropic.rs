use crate::model::{CallRecord, JobLedger, Usage};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

const API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Copy)]
struct ModelPrice {
    input_per_million: f64,
    output_per_million: f64,
}

fn price(model: &str) -> Result<ModelPrice> {
    let p = match model {
        // Prices checked against Anthropic's pricing page on 2026-07-14.
        // Sonnet 5's introductory price ends 2026-08-31.
        "claude-sonnet-5" => ModelPrice {
            input_per_million: 2.0,
            output_per_million: 10.0,
        },
        "claude-fable-5" => ModelPrice {
            input_per_million: 10.0,
            output_per_million: 50.0,
        },
        "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" => ModelPrice {
            input_per_million: 5.0,
            output_per_million: 25.0,
        },
        "claude-sonnet-4-6" | "claude-sonnet-4-5-20250929" => ModelPrice {
            input_per_million: 3.0,
            output_per_million: 15.0,
        },
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => ModelPrice {
            input_per_million: 1.0,
            output_per_million: 5.0,
        },
        // Google (checked 2026-07: ai.google.dev/gemini-api/docs/pricing)
        "gemini-3.5-flash" => ModelPrice {
            input_per_million: 1.5,
            output_per_million: 9.0,
        },
        "gemini-2.5-flash" => ModelPrice {
            input_per_million: 0.30,
            output_per_million: 2.50,
        },
        // OpenAI GPT-5.6 family (GA 2026-07-09)
        "gpt-5.6-sol" => ModelPrice {
            input_per_million: 5.0,
            output_per_million: 30.0,
        },
        "gpt-5.6-terra" => ModelPrice {
            input_per_million: 2.5,
            output_per_million: 15.0,
        },
        "gpt-5.6-luna" => ModelPrice {
            input_per_million: 1.0,
            output_per_million: 6.0,
        },
        _ => bail!(
            "no audited price is configured for model {model}; refusing an unbounded-cost call"
        ),
    };
    Ok(p)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Google,
    OpenAI,
    /// The Claude Code CLI in print mode: runs on the user's Claude
    /// subscription (Pro/Max) instead of metered API billing.
    ClaudeCode,
    /// The Codex CLI in exec mode: runs on the user's ChatGPT subscription.
    CodexCli,
    /// A local OpenAI-compatible server (Ollama, LM Studio): keyless,
    /// endpoint from OLLAMA_HOST or the conventional localhost port.
    Ollama,
}

impl Provider {
    pub fn parse(name: &str) -> Result<Self> {
        Ok(match name {
            "anthropic" => Self::Anthropic,
            "gemini" | "google" => Self::Google,
            "openai" => Self::OpenAI,
            "claude-code" => Self::ClaudeCode,
            "codex" | "codex-cli" => Self::CodexCli,
            "ollama" | "local" => Self::Ollama,
            other => bail!("unknown provider {other}"),
        })
    }

    fn env_key(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Google => "GEMINI_API_KEY",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::ClaudeCode | Self::CodexCli | Self::Ollama => "",
        }
    }

    fn is_cli(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::CodexCli)
    }

    fn cli_binary(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::CodexCli => "codex",
            _ => "",
        }
    }

    /// Adaptive model choice: the pipeline expresses how hard it is thinking
    /// via `effort`; each provider maps that to its own tier so cheap phases
    /// (seeding, low-rung authoring, blind probes) never pay flagship rates.
    fn resolve_model<'a>(self, requested: &'a str, effort: Option<&str>) -> &'a str {
        match self {
            Self::Anthropic | Self::Ollama => requested,
            Self::Google => match effort {
                Some("high") | Some("xhigh") | Some("medium") => "gemini-3.5-flash",
                _ => "gemini-2.5-flash",
            },
            Self::OpenAI => match effort {
                Some("high") | Some("xhigh") => "gpt-5.6-sol",
                Some("medium") => "gpt-5.6-terra",
                _ => "gpt-5.6-luna",
            },
            // The requested name arrives as "claude-code-<alias>" or
            // "codex-cli-<alias>"; keep it whole so ledger rows and cache
            // keys carry the alias.
            Self::ClaudeCode | Self::CodexCli => requested,
        }
    }
}

/// Locate a CLI binary. The sidecar runs with launchd's minimal PATH, so
/// the common install locations are probed explicitly. Machines often
/// carry SEVERAL copies (npm global, standalone installer, homebrew) at
/// different versions, and newer models are gated on newer CLIs, so every
/// candidate is version-probed and the newest wins.
fn find_cli(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    #[cfg(not(target_os = "windows"))]
    let probe_paths = vec![
        format!("{home}/.local/bin/{name}"),
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("{home}/.cargo/bin/{name}"),
        format!("{home}/bin/{name}"),
    ];
    #[cfg(target_os = "windows")]
    let probe_paths = {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        vec![
            format!("{appdata}\\npm\\{name}.cmd"),
            format!("{home}\\.local\\bin\\{name}.exe"),
            format!("{home}\\.local\\bin\\{name}.cmd"),
            format!("{local}\\Programs\\{name}\\{name}.exe"),
            format!("{home}\\.cargo\\bin\\{name}.exe"),
        ]
    };
    let mut candidates: Vec<std::path::PathBuf> = probe_paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .collect();
    #[cfg(not(target_os = "windows"))]
    let locator = ("which", name.to_owned());
    #[cfg(target_os = "windows")]
    let locator = ("where", name.to_owned());
    if let Some(found) = std::process::Command::new(locator.0)
        .arg(&locator.1)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned()
        })
        .filter(|found| !found.is_empty())
        .map(std::path::PathBuf::from)
    {
        if !candidates.contains(&found) {
            candidates.push(found);
        }
    }
    candidates
        .into_iter()
        .filter_map(|path| cli_version(&path).map(|version| (version, path)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, path)| path)
}

/// "codex-cli 0.144.5" or "2.1.202 (Claude Code)" -> comparable [0,144,5].
fn cli_version(path: &std::path::Path) -> Option<Vec<u64>> {
    let out = std::process::Command::new(path).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let numbers: Vec<u64> = text
        .split_whitespace()
        .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))?
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    (!numbers.is_empty()).then_some(numbers)
}

#[derive(Clone)]
pub struct AnthropicClient {
    http: Client,
    api_key: String,
    base_url: String,
    provider: Provider,
    /// Resolved binary path for the CLI-backed providers.
    cli_path: Option<std::path::PathBuf>,
}

pub struct MessageSpec<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub content: Vec<Value>,
    pub schema: Value,
    pub max_tokens: u32,
    pub phase: &'a str,
    pub source_hash: Option<&'a str>,
    pub effort: Option<&'a str>,
}

impl AnthropicClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is not set")?;
        Self::from_key(api_key)
    }

    pub fn from_provider(provider: &str, api_key: String) -> Result<Self> {
        let provider = Provider::parse(provider)?;
        if provider.is_cli() {
            let name = provider.cli_binary();
            let Some(cli_path) = find_cli(name) else {
                bail!("cli_missing: the {name} command line tool is not installed on this Mac");
            };
            let http = Client::builder().build()?;
            return Ok(Self {
                http,
                api_key: String::new(),
                base_url: String::new(),
                provider,
                cli_path: Some(cli_path),
            });
        }
        let api_key = if api_key.trim().is_empty() {
            std::env::var(provider.env_key()).unwrap_or_default()
        } else {
            api_key
        };
        if api_key.trim().is_empty() && !matches!(provider, Provider::Ollama) {
            bail!("no API key configured for the selected provider");
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(30))
            .build()?;
        let base_url = match provider {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::Google => "https://generativelanguage.googleapis.com",
            Provider::OpenAI => "https://api.openai.com",
            Provider::Ollama => return Ok(Self {
                http,
                api_key,
                base_url: std::env::var("OLLAMA_HOST")
                    .ok()
                    .filter(|url| !url.trim().is_empty())
                    .unwrap_or_else(|| "http://localhost:11434".into()),
                provider,
                cli_path: None,
            }),
            Provider::ClaudeCode | Provider::CodexCli => unreachable!(),
        };
        Ok(Self {
            http,
            api_key,
            base_url: base_url.to_owned(),
            provider,
            cli_path: None,
        })
    }

    /// Explicit key (e.g. from the app's Settings keychain), falling back to
    /// the environment only when NO key was passed. The stored key is
    /// authoritative — deleting it in Settings must really disable
    /// generation, not silently fall through to a shell-exported key.
    pub fn from_key(api_key: String) -> Result<Self> {
        let api_key = if api_key.trim().is_empty() {
            std::env::var("ANTHROPIC_API_KEY").unwrap_or_default()
        } else {
            api_key
        };
        if api_key.trim().is_empty() {
            bail!("ANTHROPIC_API_KEY is empty");
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            api_key,
            base_url: "https://api.anthropic.com".to_owned(),
            provider: Provider::Anthropic,
            cli_path: None,
        })
    }

    /// Thread-safe call: budget is reserved atomically before the request and
    /// settled after, so concurrent calls cannot jointly breach the cap. The
    /// mutex is never held across an await.
    pub async fn call_json<T: DeserializeOwned>(
        &self,
        spec: MessageSpec<'_>,
        ledger: &Mutex<JobLedger>,
    ) -> Result<T> {
        let MessageSpec {
            model,
            system,
            content,
            schema,
            max_tokens,
            phase,
            source_hash,
            effort,
        } = spec;
        let model = self.provider.resolve_model(model, effort);
        if self.provider.is_cli() {
            // Authoring calls get local verification tools: python to check
            // the arithmetic (and prefer the derivation with the simplest
            // calculation), and a latex compile so the model can look at
            // its own figure before shipping it.
            let tools = phase.starts_with('a') && phase.contains('c');
            let text = self
                .call_cli(model, system, &content, &schema, effort, phase, tools)
                .await?;
            let usage = Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            };
            let mut guard = ledger.lock().expect("ledger lock");
            guard.calls.push(CallRecord {
                phase: phase.to_owned(),
                model: model.to_owned(),
                source_hash: source_hash.map(str::to_owned),
                usage: Some(usage),
                actual_cost_usd: 0.0,
                uncertain_reservation_usd: 0.0,
                status: "complete".into(),
            });
            drop(guard);
            return serde_json::from_str(&text)
                .with_context(|| format!("parsing structured response during {phase}"));
        }
        let request = match self.provider {
            Provider::Anthropic => {
                let mut output_config = json!({
                    "format": {"type": "json_schema", "schema": schema}
                });
                if let Some(effort) = effort {
                    output_config
                        .as_object_mut()
                        .expect("output config is an object")
                        .insert("effort".into(), json!(effort));
                }
                json!({
                    "model": model,
                    "max_tokens": max_tokens,
                    "system": system,
                    "messages": [{"role": "user", "content": content}],
                    "output_config": output_config
                })
            }
            Provider::Google => {
                let parts = gemini_parts(&content)?;
                json!({
                    "_model": model,
                    "system_instruction": {"parts": [{"text": system}]},
                    "contents": [{"role": "user", "parts": parts}],
                    "generationConfig": {
                        "maxOutputTokens": max_tokens,
                        "responseMimeType": "application/json",
                        "responseSchema": sanitize_schema_for_gemini(&schema)
                    }
                })
            }
            Provider::ClaudeCode | Provider::CodexCli => unreachable!("handled by call_cli"),
            Provider::OpenAI | Provider::Ollama => {
                let text = flatten_text_content(&content)?;
                let mut request = json!({
                    "model": model,
                    "max_completion_tokens": max_tokens,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": text}
                    ],
                    "response_format": {
                        "type": "json_schema",
                        "json_schema": {"name": "result", "schema": schema, "strict": true}
                    }
                });
                // Our effort strings (low/medium/high) are valid
                // reasoning_effort values; cheap phases then spend few
                // reasoning tokens on top of the cheaper tier.
                if let Some(effort) = effort {
                    request
                        .as_object_mut()
                        .expect("request is an object")
                        .insert("reasoning_effort".into(), json!(effort));
                }
                request
            }
        };
        let input_tokens = self
            .count_tokens(&request)
            .await
            .with_context(|| format!("counting tokens for {phase}"))?;
        let pricing = price(model)?;
        let reservation = worst_case_cost(pricing, input_tokens, max_tokens as u64);
        {
            let mut ledger = ledger.lock().expect("ledger lock");
            let remaining = ledger.budget_cap_usd
                - ledger.committed_spend()
                - ledger.inflight_reservation_usd;
            if reservation > remaining + 1e-9 {
                bail!(
                    "budget gate stopped {phase}: worst-case call reservation ${reservation:.4} exceeds ${remaining:.4} remaining"
                );
            }
            ledger.inflight_reservation_usd += reservation;
        }

        let response = match self.send_with_retry(&request).await {
            Ok(response) => response,
            Err(error) => {
                let mut ledger = ledger.lock().expect("ledger lock");
                ledger.inflight_reservation_usd -= reservation;
                ledger.uncertain_spend_usd += reservation;
                ledger.calls.push(CallRecord {
                    phase: phase.to_owned(),
                    model: model.to_owned(),
                    source_hash: source_hash.map(str::to_owned),
                    usage: None,
                    actual_cost_usd: 0.0,
                    uncertain_reservation_usd: reservation,
                    status: format!("transport_error: {error:#}"),
                });
                return Err(error).with_context(|| {
                    format!(
                        "calling Anthropic during {phase}; reservation retained as uncertain spend"
                    )
                });
            }
        };
        let status = response.status();
        let body_result = response.json::<Value>().await;
        let mut guard = ledger.lock().expect("ledger lock");
        guard.inflight_reservation_usd -= reservation;
        let body: Value = match body_result {
            Ok(body) => body,
            Err(error) => {
                // The provider may have completed and billed the call even
                // though the body never arrived: retain the reservation.
                guard.uncertain_spend_usd += reservation;
                guard.calls.push(CallRecord {
                    phase: phase.to_owned(),
                    model: model.to_owned(),
                    source_hash: source_hash.map(str::to_owned),
                    usage: None,
                    actual_cost_usd: 0.0,
                    uncertain_reservation_usd: reservation,
                    status: format!("body_decode_error: {error:#}"),
                });
                return Err(error).context("decoding Anthropic response");
            }
        };
        if !status.is_success() {
            drop(guard);
            bail!(
                "provider returned {status} during {phase}: {}",
                compact_error(&body)
            );
        }
        let usage = parse_usage(self.provider, &body)?;
        let actual_cost = actual_cost(pricing, &usage);
        guard.actual_spend_usd += actual_cost;
        let stop_reason = stop_reason(self.provider, &body);
        guard.calls.push(CallRecord {
            phase: phase.to_owned(),
            model: model.to_owned(),
            source_hash: source_hash.map(str::to_owned),
            usage: Some(usage),
            actual_cost_usd: actual_cost,
            uncertain_reservation_usd: 0.0,
            status: stop_reason.clone(),
        });
        if guard.committed_spend() > guard.budget_cap_usd + 1e-6 {
            bail!("provider usage exceeded the budget invariant; job quarantined");
        }
        drop(guard);
        // The retry ladders upstream match on this substring, so every
        // provider's truncation surfaces as "max_tokens".
        if stop_reason == "max_tokens" {
            bail!("provider hit max_tokens during {phase}; structured response is incomplete");
        }
        let text = extract_text(self.provider, &body);
        let Some(text) = text else {
            bail!(
                "provider returned no text during {phase}: {}",
                compact_error(&body)
            );
        };
        serde_json::from_str(&text)
            .with_context(|| format!("parsing structured response during {phase}"))
    }

    /// Subscription transport: spawn the user's own CLI in its official
    /// headless mode. No API key is ever involved. Hermetic on purpose:
    /// empty working directory, user config skipped, all tools disabled,
    /// and the API-key env vars scrubbed so the CLI cannot silently switch
    /// from subscription auth to metered billing.
    #[allow(clippy::too_many_arguments)]
    async fn call_cli(
        &self,
        model: &str,
        system: &str,
        content: &[Value],
        schema: &Value,
        effort: Option<&str>,
        phase: &str,
        tools: bool,
    ) -> Result<String> {
        let cli = self.cli_path.as_ref().context("cli path missing")?;
        let mut prompt = String::new();
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    prompt.push_str(block.get("text").and_then(Value::as_str).unwrap_or(""));
                    prompt.push_str("\n\n");
                }
                other => bail!(
                    "unsupported content block {other:?} for the subscription provider; PDFs need an API provider"
                ),
            }
        }
        let scratch = std::env::temp_dir().join(format!("whetstone-cli-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&scratch)?;
        if tools {
            prompt.push_str(
                "\n\nLocal verification tools are available in this directory. \
                 Use python3 to check every computed key, and when two derivation \
                 routes exist, choose the one with the simplest arithmetic. If you \
                 write a TikZ figure, compile it (pdflatex), convert it to an image \
                 (sips -s format png), view the image, and fix the figure if it \
                 looks wrong. Then return the final JSON.",
            );
        }
        let effort_level = match effort {
            Some("high") | Some("xhigh") => "high",
            Some("medium") => "medium",
            _ => "low",
        };

        // Windows npm installs are .cmd shims that CreateProcess cannot
        // launch directly; route them through cmd /C.
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(cli);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut command = tokio::process::Command::new(cli);
        command
            .current_dir(&scratch)
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .env_remove("CODEX_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let out_file = scratch.join("out.json");
        match self.provider {
            Provider::ClaudeCode => {
                // Verified against claude 2.1.202: --bare and a private
                // CLAUDE_CONFIG_DIR both break keychain auth, and
                // --disallowedTools "*" blocks the internal StructuredOutput
                // tool that --json-schema relies on. --setting-sources ""
                // is the combination that stays authenticated AND keeps the
                // user's CLAUDE.md memory out of the context.
                let alias = model.strip_prefix("claude-code-").unwrap_or("sonnet");
                command
                    .arg("-p")
                    .arg("--output-format")
                    .arg("json")
                    .arg("--system-prompt")
                    .arg(system)
                    .arg("--json-schema")
                    .arg(serde_json::to_string(schema)?)
                    .arg("--model")
                    .arg(alias)
                    .arg("--effort")
                    .arg(effort_level)
                    .arg("--setting-sources")
                    .arg("");
                if tools {
                    // Verification tools, tightly scoped: python for checking
                    // the arithmetic, latex plus an image conversion so the
                    // model can LOOK at its own compiled figure.
                    command
                        .arg("--tools")
                        .arg("Bash")
                        .arg("--allowedTools")
                        .arg("Bash(python3 *)")
                        .arg("--allowedTools")
                        .arg("Bash(pdflatex *)")
                        .arg("--allowedTools")
                        .arg("Bash(sips *)")
                        .arg("--allowedTools")
                        .arg("Read")
                        .arg("--max-turns")
                        .arg("12");
                } else {
                    command.arg("--tools").arg("").arg("--max-turns").arg("1");
                }
            }
            Provider::CodexCli => {
                let schema_file = scratch.join("schema.json");
                std::fs::write(&schema_file, serde_json::to_vec(schema)?)?;
                let alias = model.strip_prefix("codex-cli-").unwrap_or("terra");
                command
                    .arg("exec")
                    .arg("--ignore-user-config")
                    .arg("--skip-git-repo-check")
                    .arg("--sandbox")
                    // Tool-enabled calls may compile and inspect figures in
                    // the scratch directory; plain calls stay read-only.
                    .arg(if tools { "workspace-write" } else { "read-only" })
                    .arg("-c")
                    .arg(format!("model=\"gpt-5.6-{alias}\""))
                    .arg("-c")
                    .arg(format!("model_reasoning_effort=\"{effort_level}\""))
                    .arg("--output-schema")
                    .arg(&schema_file)
                    .arg("-o")
                    .arg(&out_file)
                    .arg("-");
                // Codex takes the whole prompt on stdin; fold the system
                // text in front since exec has no separate system slot.
                prompt = format!("{system}\n\n{prompt}");
            }
            _ => unreachable!(),
        }

        let mut child = command.spawn().with_context(|| {
            format!("starting {} for {phase}", self.provider.cli_binary())
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt.as_bytes()).await?;
            drop(stdin);
        }
        let waited = tokio::time::timeout(Duration::from_secs(600), child.wait_with_output()).await;
        let output = match waited {
            Ok(result) => result?,
            Err(_) => {
                let _ = std::fs::remove_dir_all(&scratch);
                bail!("the {} call timed out during {phase}", self.provider.cli_binary());
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let result = self.extract_cli_result(&stdout, &out_file, &output.status, &stderr, phase);
        let _ = std::fs::remove_dir_all(&scratch);
        result
    }

    fn extract_cli_result(
        &self,
        stdout: &str,
        out_file: &std::path::Path,
        status: &std::process::ExitStatus,
        stderr: &str,
        phase: &str,
    ) -> Result<String> {
        match self.provider {
            Provider::ClaudeCode => {
                let body: Value = serde_json::from_str(stdout.trim()).with_context(|| {
                    format!(
                        "claude returned non-JSON during {phase}: {}",
                        stdout.chars().take(300).collect::<String>()
                    )
                })?;
                if body.get("is_error").and_then(Value::as_bool) == Some(true) {
                    // The CLI reports errors in "result" (with a "subtype"
                    // like error_max_turns), not under error.message.
                    let detail = body
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .or_else(|| body.get("result").and_then(Value::as_str))
                        .filter(|text| !text.trim().is_empty())
                        .or_else(|| body.get("subtype").and_then(Value::as_str))
                        .unwrap_or("no detail");
                    bail!("claude failed during {phase}: {detail}");
                }
                if let Some(structured) = body.get("structured_output") {
                    if !structured.is_null() {
                        return Ok(structured.to_string());
                    }
                }
                body.get("result")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow!("claude response had no result during {phase}"))
            }
            Provider::CodexCli => {
                if let Ok(bytes) = std::fs::read(out_file) {
                    let text = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !text.is_empty() {
                        return Ok(text);
                    }
                }
                if !status.success() {
                    bail!(
                        "codex failed during {phase}: {}",
                        stderr.lines().rev().take(4).collect::<Vec<_>>().join(" | ")
                    );
                }
                let text = stdout.trim();
                if text.is_empty() {
                    bail!("codex returned nothing during {phase}");
                }
                Ok(text.to_owned())
            }
            _ => unreachable!(),
        }
    }

    async fn count_tokens(&self, message_request: &Value) -> Result<u64> {
        // Only Anthropic exposes a free token-count endpoint. For the other
        // providers a conservative character-based estimate feeds the
        // runaway-stop reservation (roughly 3.2 chars/token, +15% headroom).
        if self.provider != Provider::Anthropic {
            let chars = message_request.to_string().chars().count() as f64;
            return Ok((chars / 3.2 * 1.15) as u64);
        }
        let mut count_request = message_request.clone();
        if let Some(object) = count_request.as_object_mut() {
            object.remove("max_tokens");
            object.remove("effort");
        }
        let mut delay = 2;
        for attempt in 0..3 {
            let response_result = self
                .http
                .post(format!("{}/v1/messages/count_tokens", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", API_VERSION)
                .json(&count_request)
                .send()
                .await;
            let response = match response_result {
                Ok(response) => response,
                Err(_) if attempt < 2 => {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    delay *= 2;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let status = response.status();
            let raw = response.text().await?;
            if (status.is_server_error() || raw.trim().is_empty()) && attempt < 2 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
                delay *= 2;
                continue;
            }
            let body: Value = serde_json::from_str(&raw).with_context(|| {
                format!(
                    "token-count endpoint returned non-JSON {status}: {}",
                    raw.chars().take(300).collect::<String>()
                )
            })?;
            if !status.is_success() {
                bail!(
                    "token-count endpoint returned {status}: {}",
                    compact_error(&body)
                );
            }
            return body
                .get("input_tokens")
                .and_then(Value::as_u64)
                .ok_or_else(|| anyhow!("token-count response omitted input_tokens"));
        }
        unreachable!()
    }

    fn request_builder(&self, body: &Value) -> reqwest::RequestBuilder {
        match self.provider {
            Provider::Anthropic => self
                .http
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", API_VERSION)
                .json(body),
            Provider::Google => {
                // The model is not in the Gemini body; recover it from the
                // spec stashed under a private key by the caller? No — the
                // caller passes it via the URL builder below.
                let model = body
                    .get("_model")
                    .and_then(Value::as_str)
                    .unwrap_or("gemini-2.5-flash")
                    .to_owned();
                let mut clean = body.clone();
                clean.as_object_mut().map(|o| o.remove("_model"));
                self.http
                    .post(format!(
                        "{}/v1beta/models/{model}:generateContent",
                        self.base_url
                    ))
                    .header("x-goog-api-key", &self.api_key)
                    .json(&clean)
            }
            Provider::OpenAI | Provider::Ollama => self
                .http
                .post(format!("{}/v1/chat/completions", self.base_url))
                .header(
                    "Authorization",
                    // Ollama ignores auth; a placeholder keeps proxies happy.
                    format!("Bearer {}", if self.api_key.is_empty() { "ollama" } else { &self.api_key }),
                )
                .json(body),
            Provider::ClaudeCode | Provider::CodexCli => unreachable!("handled by call_cli"),
        }
    }

    async fn send_with_retry(&self, body: &Value) -> Result<reqwest::Response> {
        let mut delay = 2;
        for attempt in 0..3 {
            // Transport failures (connect timeout, reset) retry like 5xx:
            // one flaky connection must not kill a whole preparation job.
            let sent = self
                .request_builder(body)
                .send()
                .await;
            let response = match sent {
                Ok(response) => response,
                Err(error) if attempt < 2 => {
                    eprintln!("  transport error (attempt {}): {error}", attempt + 1);
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    delay *= 2;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            if !matches!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::INTERNAL_SERVER_ERROR
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
            ) || attempt == 2
            {
                return Ok(response);
            }
            tokio::time::sleep(Duration::from_secs(delay)).await;
            delay *= 2;
        }
        unreachable!()
    }
}

fn compact_error(body: &Value) -> String {
    body.get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| body.as_str().unwrap_or("unknown provider error"))
        .chars()
        .take(600)
        .collect()
}

/// Convert Anthropic-style content blocks into Gemini parts. Text passes
/// through; base64 PDF documents become inline_data.
fn gemini_parts(content: &[Value]) -> Result<Vec<Value>> {
    let mut parts = Vec::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                parts.push(json!({"text": block.get("text").and_then(Value::as_str).unwrap_or("")}));
            }
            Some("document") => {
                let data = block
                    .pointer("/source/data")
                    .and_then(Value::as_str)
                    .context("document block without base64 data")?;
                parts.push(json!({
                    "inline_data": {"mime_type": "application/pdf", "data": data}
                }));
            }
            other => bail!("unsupported content block {other:?} for the Google provider"),
        }
    }
    Ok(parts)
}

/// OpenAI chat completions path takes plain text; PDF sources need the
/// Anthropic or Google provider.
fn flatten_text_content(content: &[Value]) -> Result<String> {
    let mut out = String::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                out.push_str(block.get("text").and_then(Value::as_str).unwrap_or(""));
                out.push_str("\n\n");
            }
            Some("document") => {
                bail!("PDF sources are not supported with the OpenAI provider yet; use Anthropic or Google for PDFs")
            }
            other => bail!("unsupported content block {other:?} for the OpenAI provider"),
        }
    }
    Ok(out)
}

/// Gemini's responseSchema is an OpenAPI subset: strip JSON-Schema keys it
/// rejects while keeping the structural ones it honors.
fn sanitize_schema_for_gemini(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut clean = serde_json::Map::new();
            for (key, value) in map {
                match key.as_str() {
                    "additionalProperties" | "$schema" | "minItems" | "maxItems"
                    | "minLength" | "maxLength" | "minimum" | "maximum" | "pattern" => {}
                    _ => {
                        clean.insert(key.clone(), sanitize_schema_for_gemini(value));
                    }
                }
            }
            Value::Object(clean)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(sanitize_schema_for_gemini).collect())
        }
        other => other.clone(),
    }
}

/// Normalized stop reason; "max_tokens" is the contract the retry ladders
/// match on for every provider.
fn stop_reason(provider: Provider, body: &Value) -> String {
    match provider {
        Provider::ClaudeCode | Provider::CodexCli => "complete".to_owned(),
        Provider::Anthropic => body
            .get("stop_reason")
            .and_then(Value::as_str)
            .unwrap_or("complete")
            .to_owned(),
        Provider::Google => {
            let reason = body
                .pointer("/candidates/0/finishReason")
                .and_then(Value::as_str)
                .unwrap_or("STOP");
            if reason == "MAX_TOKENS" { "max_tokens".into() } else { reason.to_lowercase() }
        }
        Provider::OpenAI | Provider::Ollama => {
            let reason = body
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop");
            if reason == "length" { "max_tokens".into() } else { reason.to_owned() }
        }
    }
}

fn extract_text(provider: Provider, body: &Value) -> Option<String> {
    match provider {
        Provider::ClaudeCode | Provider::CodexCli => None,
        Provider::Anthropic => body
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            })
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        Provider::Google => body
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
            .filter(|t| !t.is_empty()),
        Provider::OpenAI | Provider::Ollama => body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn parse_usage(provider: Provider, body: &Value) -> Result<Usage> {
    match provider {
        Provider::ClaudeCode | Provider::CodexCli => unreachable!("handled by call_cli"),
        Provider::Google => {
            let meta = body
                .get("usageMetadata")
                .ok_or_else(|| anyhow!("Gemini response omitted usageMetadata"))?;
            return Ok(Usage {
                input_tokens: meta.get("promptTokenCount").and_then(Value::as_u64).unwrap_or(0),
                output_tokens: meta
                    .get("candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    + meta.get("thoughtsTokenCount").and_then(Value::as_u64).unwrap_or(0),
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            });
        }
        Provider::OpenAI | Provider::Ollama => {
            let u = body
                .get("usage")
                .ok_or_else(|| anyhow!("OpenAI response omitted usage"))?;
            return Ok(Usage {
                input_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
                output_tokens: u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0),
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            });
        }
        Provider::Anthropic => {}
    }
    let u = body
        .get("usage")
        .ok_or_else(|| anyhow!("Anthropic response omitted usage"))?;
    let nested_creation = u
        .get("cache_creation")
        .map(|c| {
            c.get("ephemeral_5m_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + c.get("ephemeral_1h_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
        })
        .unwrap_or(0);
    Ok(Usage {
        input_tokens: u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
        output_tokens: u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        cache_creation_input_tokens: u
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(nested_creation),
        cache_read_input_tokens: u
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

fn worst_case_cost(price: ModelPrice, input_tokens: u64, max_output_tokens: u64) -> f64 {
    (input_tokens as f64 * price.input_per_million * 1.25
        + max_output_tokens as f64 * price.output_per_million)
        / 1_000_000.0
}

fn actual_cost(price: ModelPrice, usage: &Usage) -> f64 {
    (usage.input_tokens as f64 * price.input_per_million
        + usage.cache_creation_input_tokens as f64 * price.input_per_million * 1.25
        + usage.cache_read_input_tokens as f64 * price.input_per_million * 0.10
        + usage.output_tokens as f64 * price.output_per_million)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_cost_accounts_for_cache() {
        let p = price("claude-sonnet-5").unwrap();
        let u = Usage {
            input_tokens: 100_000,
            output_tokens: 10_000,
            cache_creation_input_tokens: 20_000,
            cache_read_input_tokens: 30_000,
        };
        let cost = actual_cost(p, &u);
        assert!((cost - 0.356).abs() < 1e-9);
    }

    #[test]
    fn reservation_uses_maximum_output() {
        let p = price("claude-sonnet-5").unwrap();
        assert!((worst_case_cost(p, 100_000, 10_000) - 0.35).abs() < 1e-9);
    }
}
