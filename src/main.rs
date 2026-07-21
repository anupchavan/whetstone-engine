mod anthropic;
mod classroom;
mod elo;
mod model;
mod moves;
mod oracle;
mod pipeline;
mod render;
mod serve;
mod source;
mod tikz;

use anthropic::AnthropicClient;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use pipeline::{
    PipelineConfig, new_ledger, rebalance_answer_positions, run_pipeline,
    validate_delivery_inventory,
};
use render::{write_delivery_manifest, write_ledger, write_outputs};
use source::{collect_sources, load_mechanisms};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "whetstone",
    version,
    about = "Budget-bounded source-faithful Olympiad practice factory"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a fully validated Markdown question set from note files or folders.
    Generate {
        /// A note file or recursively scanned folder. Repeat for multiple roots.
        #[arg(long = "input", required = true, action = clap::ArgAction::Append)]
        inputs: Vec<PathBuf>,
        /// Markdown output path. JSON items and a job ledger are written beside it.
        #[arg(long, default_value = "results/questions.md")]
        output: PathBuf,
        #[arg(long, default_value_t = 60)]
        count: usize,
        /// Absolute maximum committed Anthropic spend, including uncertain timeouts.
        #[arg(long, default_value_t = 5.0)]
        budget_usd: f64,
        /// anthropic | gemini | openai | ollama | claude-code | codex.
        /// Keys come from ANTHROPIC_API_KEY / GEMINI_API_KEY /
        /// OPENAI_API_KEY; ollama and the CLIs need none.
        #[arg(long, default_value = "anthropic")]
        provider: String,
        /// Model override; sensible per-provider defaults when omitted.
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value = "claude-sonnet-5", hide = true)]
        author_model: String,
        #[arg(long, default_value = "claude-sonnet-5", hide = true)]
        validator_model: String,
        #[arg(long, default_value_t = 2)]
        max_attempts: usize,
        /// Editorial depth: scholar | deep_work | olympiad_studio.
        #[arg(long, default_value = "olympiad_studio")]
        quality_tier: String,
        #[arg(long, default_value = "mechanisms.jsonl")]
        mechanisms: PathBuf,
    },
    /// Run as the app sidecar: newline-delimited JSON protocol over stdin/stdout.
    Serve {
        /// Serve canned questions with no API key or spend (UI testing).
        #[arg(long)]
        mock: bool,
    },
    /// Print the manual page (install: whetstone man > /usr/local/share/man/man1/whetstone.1).
    Man,
    /// Finalize a smaller, explicitly authorized target from fully validated cached inventory.
    Finalize {
        #[arg(long, default_value = "results/validated-inventory.json")]
        inventory: PathBuf,
        #[arg(long, default_value = "results/whetstone-job.json")]
        ledger: PathBuf,
        #[arg(long, default_value = "results/questions.md")]
        output: PathBuf,
        #[arg(long)]
        count: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Man => {
            let man = clap_mangen::Man::new(<Cli as clap::CommandFactory>::command());
            let mut out = Vec::new();
            man.render(&mut out)?;
            use std::io::Write;
            std::io::stdout().write_all(&out)?;
            return Ok(());
        }
        Command::Generate {
            inputs,
            output,
            count,
            budget_usd,
            provider,
            model,
            author_model,
            validator_model,
            max_attempts,
            quality_tier,
            mechanisms,
        } => {
            if count == 0 {
                bail!("--count must be positive");
            }
            if !(0.01..=1000.0).contains(&budget_usd) {
                bail!("--budget-usd must be between 0.01 and 1000");
            }
            if max_attempts == 0 || max_attempts > 5 {
                bail!("--max-attempts must be between 1 and 5");
            }
            let mut sources = collect_sources(&inputs).context("source preflight failed")?;
            sources.retain(|source| {
                let ok = source.extracted_text.trim().chars().count() >= source::MIN_TEXT_SOURCE_CHARS;
                if !ok {
                    eprintln!("  skipping (too little text): {}", source.path.display());
                }
                ok
            });
            if sources.is_empty() {
                bail!("every selected note is too small; nothing to author from");
            }
            let mechanisms = load_mechanisms(&mechanisms).context("mechanism preflight failed")?;
            let ledger_path = output.with_file_name("whetstone-job.json");
            let (default_author, default_validator): (String, String) = match provider.as_str() {
                "gemini" => ("gemini-3.5-flash".into(), "gemini-3.5-flash".into()),
                "openai" => ("gpt-5.6-terra".into(), "gpt-5.6-terra".into()),
                "ollama" => (
                    model.clone().unwrap_or_else(|| "llama3.1".into()),
                    model.clone().unwrap_or_else(|| "llama3.1".into()),
                ),
                "claude-code" => ("claude-code-sonnet".into(), "claude-code-sonnet".into()),
                "codex" => ("codex-cli-terra".into(), "codex-cli-terra".into()),
                _ => (author_model, validator_model),
            };
            let author_model = model.clone().unwrap_or(default_author);
            let validator_model = model.unwrap_or(default_validator);
            let config = PipelineConfig {
                count,
                budget_usd,
                author_model,
                validator_model,
                max_attempts,
                quality_tier,
                ledger_path: ledger_path.clone(),
                inventory_path: output.with_file_name("validated-inventory.json"),
                cache_dir: output
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(".whetstone-cache"),
                skill_by_hash: Default::default(),
            };
            let fresh_ledger = new_ledger(&config, &sources);
            let existing_ledger = fs::read(&ledger_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<model::JobLedger>(&bytes).ok());
            if let Some(old) = existing_ledger.as_ref().filter(|old| {
                old.idempotency_key == fresh_ledger.idempotency_key && old.status == "complete"
            }) {
                if old.committed_spend() > budget_usd {
                    bail!(
                        "matching completed job spent ${:.4}, above the requested ${budget_usd:.2} cap",
                        old.committed_spend()
                    );
                }
                if old.accepted_count != count
                    || !output.is_file()
                    || !output.with_extension("json").is_file()
                {
                    bail!(
                        "matching job is marked complete but its {}-item output artifacts are missing or inconsistent; preserve the ledger and inspect the output directory",
                        count
                    );
                }
                eprintln!(
                    "Already complete: {} accepted questions -> {} (${:.4} committed)",
                    old.accepted_count,
                    output.display(),
                    old.committed_spend()
                );
                return Ok(());
            }
            let ledger = match existing_ledger
                .filter(|old| old.idempotency_key == fresh_ledger.idempotency_key)
            {
                Some(mut old) => {
                    if old.committed_spend() > budget_usd {
                        bail!(
                            "existing matching job has ${:.4} committed spend, above the requested ${budget_usd:.2} cap",
                            old.committed_spend()
                        );
                    }
                    old.budget_cap_usd = budget_usd;
                    old.status = "resuming".into();
                    for source in &mut old.sources {
                        if let Some(fresh) = fresh_ledger
                            .sources
                            .iter()
                            .find(|fresh| fresh.sha256 == source.sha256)
                        {
                            source.domain = fresh.domain.clone();
                        }
                    }
                    old
                }
                None => fresh_ledger,
            };
            write_ledger(&ledger_path, &ledger)?;
            eprintln!("Whetstone job {}", ledger.job_id);
            eprintln!(
                "Preflight: {} source(s), {} requested questions, ${:.2} hard cap",
                sources.len(),
                count,
                budget_usd
            );
            match oracle::Oracle::prepare(&config.cache_dir) {
                Ok(oracle) if oracle.available() => {
                    eprintln!("  oracle: python3+sympy ready (computable keys verified for free)")
                }
                _ => eprintln!(
                    "  oracle: UNAVAILABLE — computable keys will need blind agreement instead"
                ),
            }
            for source in &sources {
                eprintln!(
                    "  {} [{}; {} extracted chars]",
                    source.path.display(),
                    source.domain,
                    source.extracted_text.chars().count()
                );
            }
            let client = AnthropicClient::from_provider(&provider, String::new())?;
            let ledger = std::sync::Mutex::new(ledger);
            match run_pipeline(&client, &config, &sources, &mechanisms, &ledger).await {
                Ok(items) => {
                    let mut ledger = ledger.into_inner().expect("ledger lock");
                    write_outputs(&output, &items, &mut ledger)?;
                    ledger.status = "complete".into();
                    ledger.accepted_count = items.len();
                    write_ledger(&ledger_path, &ledger)?;
                    eprintln!(
                        "Complete: {} accepted questions -> {}",
                        items.len(),
                        output.display()
                    );
                    eprintln!(
                        "Anthropic spend this budget epoch: ${:.4} committed / ${:.2} (${:.4} lifetime)",
                        ledger.committed_spend(),
                        ledger.budget_cap_usd,
                        ledger.lifetime_committed_spend()
                    );
                    Ok(())
                }
                Err(error) => {
                    let mut ledger = ledger.into_inner().expect("ledger lock");
                    if ledger.status != "incomplete_quality_gate" {
                        ledger.status = "failed_closed".into();
                    }
                    let _ = write_ledger(&ledger_path, &ledger);
                    Err(error)
                }
            }
        }
        Command::Serve { mock } => serve::serve(mock).await,
        Command::Finalize {
            inventory,
            ledger,
            output,
            count,
        } => {
            if count == 0 {
                bail!("--count must be positive");
            }
            let checkpoint: model::ValidatedInventory = serde_json::from_slice(
                &fs::read(&inventory)
                    .with_context(|| format!("reading {}", inventory.display()))?,
            )
            .with_context(|| format!("decoding {}", inventory.display()))?;
            if checkpoint.accepted_count != checkpoint.items.len() {
                bail!("validated inventory count does not match its item payload");
            }
            if count > checkpoint.items.len() {
                bail!(
                    "requested {count} finalized questions but only {} are fully validated",
                    checkpoint.items.len()
                );
            }
            let mut items = checkpoint.items;
            items.truncate(count);
            validate_delivery_inventory(&items)?;
            rebalance_answer_positions(&mut items);
            validate_delivery_inventory(&items)?;

            let mut audit: model::JobLedger = serde_json::from_slice(
                &fs::read(&ledger).with_context(|| format!("reading {}", ledger.display()))?,
            )
            .with_context(|| format!("decoding {}", ledger.display()))?;
            if audit.committed_spend() > audit.budget_cap_usd + 1e-9 {
                bail!("source job ledger exceeds its authorized budget cap");
            }
            let original_requested_count = audit.requested_count;
            audit.status = "complete_reduced_target".into();
            audit.requested_count = count;
            audit.accepted_count = count;
            write_outputs(&output, &items, &mut audit)?;

            let mut distribution = BTreeMap::<String, usize>::new();
            for item in &items {
                *distribution.entry(item.source_name.clone()).or_default() += 1;
            }
            let manifest_path = output.with_file_name("whetstone-delivery.json");
            write_delivery_manifest(
                &manifest_path,
                &serde_json::json!({
                    "status": "complete_reduced_target",
                    "parent_job_id": audit.job_id,
                    "parent_job_ledger": ledger.display().to_string(),
                    "original_requested_count": original_requested_count,
                    "delivered_count": count,
                    "source_inventory": inventory.display().to_string(),
                    "source_inventory_originally_complete": checkpoint.complete,
                    "explicit_offline_finalization": true,
                    "additional_ai_spend_usd": 0.0,
                    "budget_epoch_spend_usd": audit.committed_spend(),
                    "budget_cap_usd": audit.budget_cap_usd,
                    "lifetime_committed_spend_usd": audit.lifetime_committed_spend(),
                    "source_distribution": distribution,
                    "markdown": output.display().to_string(),
                    "items_json": output.with_extension("json").display().to_string()
                }),
            )?;
            eprintln!(
                "Finalized reduced target: {count} validated questions -> {} (no API calls)",
                output.display()
            );
            Ok(())
        }
    }
}
