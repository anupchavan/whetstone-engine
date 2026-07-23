use crate::model::{CandidateQuestion, JobLedger, ValidatedInventory};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn render_markdown(items: &[CandidateQuestion], ledger: &JobLedger) -> String {
    let mut out = String::new();
    out.push_str("# Whetstone — rated practice bank\n\n");
    out.push_str(&format!(
        "> {} independently checked, source-faithful practice questions with provisional difficulty ratings. AI spend: ${:.4} / ${:.2}. Ratings are anchor-v1 estimates from structural priors and model probes; learner attempts will sharpen them. These are formative practice, not a secure or consequential assessment.\n\n",
        items.len(), ledger.committed_spend(), ledger.budget_cap_usd
    ));
    out.push_str("## Questions\n\n");
    for (index, item) in items.iter().enumerate() {
        out.push_str(&format!("### {}. {}\n\n", index + 1, item.topic));
        out.push_str(&format!("{}\n\n", item.stem.trim()));
        if let Some(svg) = &item.diagram_svg {
            out.push_str(svg.trim());
            out.push_str("\n\n");
        }
        for (option_index, option) in item.options.iter().enumerate() {
            let letter = (b'A' + option_index as u8) as char;
            out.push_str(&format!("- **{letter}.** {}\n", option.trim()));
        }
        let rating_line = item
            .elo
            .as_ref()
            .map(|elo| {
                format!(
                    " · Rating: ~{:.0} ± {:.0} ({}, provisional)",
                    elo.rating,
                    elo.deviation,
                    crate::elo::band(elo.rating)
                )
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "\n*Domain: {} · Source: {} · Status: based on your source{rating_line}*\n\n",
            item.domain, item.source_name
        ));
    }
    out.push_str("---\n\n## Answers and worked solutions\n\n");
    for (index, item) in items.iter().enumerate() {
        let answer = (b'A' + item.answer_index) as char;
        out.push_str(&format!("### {}. Answer: {}\n\n", index + 1, answer));
        out.push_str(&format!(
            "**Decisive insight.** {}\n\n",
            item.decisive_insight.trim()
        ));
        out.push_str(&format!(
            "**Solution.** {}\n\n",
            item.worked_solution.trim()
        ));
        out.push_str("**Why the other choices are tempting.**\n\n");
        for (option_index, rationale) in item.distractor_rationales.iter().enumerate() {
            let letter = (b'A' + option_index as u8) as char;
            out.push_str(&format!("- **{letter}:** {}\n", rationale.trim()));
        }
        out.push_str("\n**Source support.**\n\n");
        for evidence in &item.evidence {
            out.push_str(&format!(
                "- {} — {}\n",
                evidence.locator.trim(),
                evidence.support.trim()
            ));
        }
        let blind = item
            .validation
            .blind_answer_index
            .map(|i| ((b'A' + i) as char).to_string())
            .unwrap_or_else(|| "—".to_owned());
        let key_evidence = match item.verification.verdict.as_str() {
            "proved" => format!("oracle-proved ({})", item.verification.kind),
            _ => "blind-agreement".to_owned(),
        };
        let move_names: Vec<&str> = item
            .moves
            .move_keys
            .iter()
            .map(|key| {
                crate::moves::find_move(key)
                    .map(|m| m.name)
                    .unwrap_or(key.as_str())
            })
            .collect();
        let composition = format!(
            "seed {} × {}{}",
            item.moves.seed_id,
            move_names.join(" × "),
            if item.moves.operators.is_empty() {
                String::new()
            } else {
                format!(" [{}]", item.moves.operators.join(", "))
            }
        );
        out.push_str(&format!(
            "\n*Key evidence: {key_evidence} · Composition: {composition} · cue {} · rung {} ({}) · blind probe chose {blind} at {:.0}% confidence · grounded review passed.*\n\n",
            item.moves.cue_visibility,
            item.moves.rung,
            crate::moves::rung(item.moves.rung).label,
            item.validation.blind_confidence.unwrap_or(0.0) * 100.0
        ));
    }
    out
}

pub fn write_outputs(
    output: &Path,
    items: &[CandidateQuestion],
    ledger: &mut JobLedger,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let markdown = render_markdown(items, ledger);
    atomic_write(output, markdown.as_bytes())?;
    ledger.output_markdown = Some(output.display().to_string());
    let items_path = output.with_extension("json");
    atomic_write(&items_path, &serde_json::to_vec_pretty(items)?)?;
    Ok(())
}

pub fn write_ledger(path: &Path, ledger: &JobLedger) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &serde_json::to_vec_pretty(ledger)?)
}

pub fn write_inventory_checkpoint(
    path: &Path,
    items: &[CandidateQuestion],
    requested_count: usize,
    complete: bool,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let checkpoint = ValidatedInventory {
        complete,
        requested_count,
        accepted_count: items.len(),
        learner_release_allowed: complete && items.len() == requested_count,
        items: items.to_vec(),
    };
    atomic_write(path, &serde_json::to_vec_pretty(&checkpoint)?)
}

pub fn write_delivery_manifest(path: &Path, manifest: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, &serde_json::to_vec_pretty(manifest)?)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|x| x.to_str()).unwrap_or("out")
    ));
    fs::write(&temp, bytes).with_context(|| format!("writing {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("committing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DifficultyFeatures, EvidenceRef, ItemValidation};

    #[test]
    fn answer_index_renders_as_letter() {
        let item = CandidateQuestion {
            id: "q".into(),
            domain: "physics".into(),
            topic: "Optics".into(),
            instructional_purpose: "transfer".into(),
            stem: "Stem?".into(),
            diagram_svg: None,
            diagram_svg_dark: None,
            diagram_pdf: None,
            diagram_pdf_dark: None,
            question_type: "mcq".into(),
            diagram_tikz: String::new(),
            figure_placement: "none".into(),
            option_svgs: vec![],
            numeric_answer: String::new(),
            correct_indices: vec![],
            options: vec!["x".into(), "y".into(), "z".into(), "w".into()],
            answer_index: 0,
            worked_solution: "Because.".into(),
            decisive_insight: "Perspective.".into(),
            target_skill: String::new(),
            where_used: String::new(),
            why_necessary: String::new(),
            source_paths: vec![],
            source_kind: String::new(),
            distractor_rationales: vec!["ok".into(); 4],
            evidence: vec![EvidenceRef {
                locator: "p. 1".into(),
                support: "rule".into(),
            }],
            difficulty: DifficultyFeatures {
                essential_inferences: 1,
                representation_changes: 1,
                cue_visibility: "low".into(),
                distractor_attractiveness: "high".into(),
                computational_burden: "low".into(),
            },
            truth_status: "source_faithful_only".into(),
            source_name: "note".into(),
            source_hash: "abc".into(),
            validation: ItemValidation {
                local_gate: true,
                blind_answer_index: Some(0),
                blind_confidence: Some(0.9),
                fidelity_gate: true,
                construct_gate: true,
                presentation_gate: true,
                novelty_gate: true,
                reviewer_reason: None,
                blind_issue: None,
            },
            moves: crate::model::MoveAssignment {
                seed_id: "s1".into(),
                move_keys: vec!["false-symmetry".into()],
                operators: vec![],
                cue_visibility: "medium".into(),
                rung: 2,
                mutations: vec![],
                bridge: false,
            },
            verification: crate::model::Verification {
                kind: "numeric".into(),
                script: "assert 1 == 1".into(),
                verdict: "proved".into(),
                detail: String::new(),
            },
            elo: Some(crate::elo::new_state(1500.0, 350.0)),
        };
        let ledger = JobLedger {
            job_id: "j".into(),
            idempotency_key: "i".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            status: "complete".into(),
            requested_count: 1,
            accepted_count: 1,
            budget_cap_usd: 5.0,
            budget_baseline_usd: 0.0,
            actual_spend_usd: 1.0,
            uncertain_spend_usd: 0.0,
            inflight_reservation_usd: 0.0,
            author_model: "m".into(),
            validator_model: "m".into(),
            sources: vec![],
            calls: vec![],
            output_markdown: None,
        };
        let md = render_markdown(&[item], &ledger);
        assert!(md.contains("Answer: A"));
    }
}
