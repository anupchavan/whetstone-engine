use crate::model::{Mechanism, SourceDocument, SourcePayload};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &["pdf", "txt", "md", "markdown"];
const MIN_PDF_EXTRACTED_CHARS: usize = 200;
pub const MIN_TEXT_SOURCE_CHARS: usize = 40;

pub fn collect_sources(inputs: &[PathBuf]) -> Result<Vec<SourceDocument>> {
    let mut paths = Vec::new();
    for input in inputs {
        if input.is_file() {
            paths.push(input.clone());
        } else if input.is_dir() {
            for entry in WalkDir::new(input).follow_links(false) {
                let entry = entry.with_context(|| format!("walking {}", input.display()))?;
                if entry.file_type().is_file() && supported(entry.path()) {
                    paths.push(entry.into_path());
                }
            }
        } else {
            bail!("input does not exist: {}", input.display());
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        bail!("no supported .pdf, .txt, .md, or .markdown notes found");
    }
    paths.into_iter().map(load_source).collect()
}

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| SUPPORTED_EXTENSIONS.contains(&x.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn load_source(path: PathBuf) -> Result<SourceDocument> {
    if !supported(&path) {
        bail!("unsupported source type: {}", path.display());
    }
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("note")
        .to_owned();
    let (media_type, extracted_text, page_count, payload) = if ext == "pdf" {
        // pdftotext (poppler) gives the deterministic text preflight, but
        // app users rarely have it: without it the PDF still works as a
        // native document attachment, with the model reading pages itself.
        let text = match Command::new("pdftotext").args(["-layout"]).arg(&path).arg("-").output() {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .replace('\u{000c}', "\n\n[PAGE BREAK]\n\n"),
            _ => {
                eprintln!(
                    "  note: pdftotext unavailable for {} - sending the PDF natively without text preflight",
                    path.display()
                );
                String::new()
            }
        };
        let pages = pdf_page_count(&path);
        (
            "application/pdf".to_owned(),
            text,
            pages,
            SourcePayload::Pdf(bytes),
        )
    } else {
        let text = String::from_utf8(bytes.clone())
            .with_context(|| format!("{} is not valid UTF-8", path.display()))?;
        (
            "text/plain".to_owned(),
            text.clone(),
            None,
            SourcePayload::Text(text),
        )
    };
    let extracted_chars = extracted_text.trim().chars().count();
    if ext == "pdf" && extracted_chars > 0 && extracted_chars < MIN_PDF_EXTRACTED_CHARS {
        bail!(
            "PDF extracted too little text and needs OCR or repair: {}",
            path.display()
        );
    }
    // Text notes below MIN_TEXT_SOURCE_CHARS are NOT an error here: callers
    // decide (the CLI skips them with a warning; the sidecar groups small
    // notes into a combined envelope so atomic vaults stay selectable).
    let _ = extracted_chars;
    let domain = classify_domain(&name, &extracted_text);
    let note_path = path.to_string_lossy().to_string();
    Ok(SourceDocument {
        path,
        name,
        note_paths: vec![note_path],
        media_type,
        sha256,
        extracted_text,
        page_count,
        domain,
        payload,
    })
}

fn pdf_page_count(path: &Path) -> Option<u32> {
    let output = Command::new("pdfinfo").arg(path).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("Pages:")?.trim().parse().ok())
}

pub fn classify_domain(name: &str, text: &str) -> String {
    let name_lower = name.to_ascii_lowercase();
    for (needle, domain) in [
        ("ray optics", "physics"),
        ("solid state", "physical-chemistry"),
        ("function", "mathematics"),
        ("p block", "inorganic-chemistry"),
        ("p-block", "inorganic-chemistry"),
    ] {
        if name_lower.contains(needle) {
            return domain.to_owned();
        }
    }
    let sample =
        format!("{} {}", name, text.chars().take(40_000).collect::<String>()).to_ascii_lowercase();
    let groups: &[(&str, &[&str])] = &[
        (
            "physics",
            &[
                "optics", "ray", "lens", "mirror", "velocity", "force", "physics",
            ],
        ),
        (
            "mathematics",
            &[
                "function",
                "trigonometric",
                "polynomial",
                "mathematics",
                "domain",
                "range",
            ],
        ),
        (
            "physical-chemistry",
            &[
                "solid state",
                "unit cell",
                "lattice",
                "packing",
                "physical chemistry",
            ],
        ),
        (
            "inorganic-chemistry",
            &["p block", "phosph", "halogen", "inorganic", "p-block"],
        ),
        (
            "organic-chemistry",
            &[
                "organic chemistry",
                "reaction mechanism",
                "alkane",
                "aromatic",
            ],
        ),
        (
            "chemistry",
            &["chemistry", "molecule", "compound", "reaction"],
        ),
        (
            "computer-science",
            &["algorithm", "program", "complexity", "data structure"],
        ),
        ("biology", &["biology", "cell", "organism", "genetics"]),
    ];
    groups
        .iter()
        .map(|(domain, terms)| {
            (
                *domain,
                terms
                    .iter()
                    .map(|t| sample.matches(t).count())
                    .sum::<usize>(),
            )
        })
        .max_by_key(|(_, score)| *score)
        .filter(|(_, score)| *score > 0)
        .map(|(domain, _)| domain.to_owned())
        .unwrap_or_else(|| "general".to_owned())
}

pub fn load_mechanisms(path: &Path) -> Result<Vec<Mechanism>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("invalid mechanism JSON at line {}", i + 1))
        })
        .collect()
}

pub fn allocate_quotas(total: usize, sources: &[SourceDocument]) -> HashMap<String, usize> {
    let mut result = HashMap::new();
    if sources.is_empty() {
        return result;
    }
    let base = total / sources.len();
    let remainder = total % sources.len();
    for (i, source) in sources.iter().enumerate() {
        result.insert(source.sha256.clone(), base + usize::from(i < remainder));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn quotas_sum_and_are_balanced() {
        let mk = |hash: &str| SourceDocument {
            path: hash.into(),
            name: hash.into(),
            note_paths: vec![hash.into()],
            media_type: "text/plain".into(),
            sha256: hash.into(),
            extracted_text: "text".into(),
            page_count: None,
            domain: "general".into(),
            payload: SourcePayload::Text("text".into()),
        };
        let sources = vec![mk("a"), mk("b"), mk("c"), mk("d")];
        let q = allocate_quotas(60, &sources);
        assert_eq!(q.values().sum::<usize>(), 60);
        assert!(q.values().all(|x| *x == 15));
    }

    #[test]
    fn concise_markdown_note_passes_preflight() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pitching.md");
        let mut file = fs::File::create(&path).unwrap();
        write!(
            file,
            "# Maximum angular acceleration\n\nThe maximum is $\\phi \\omega^2 = 0.0046$ rad/s^2."
        )
        .unwrap();

        let source = load_source(path).unwrap();
        assert!(source.extracted_text.contains("0.0046"));
    }

    #[test]
    fn nearly_empty_markdown_note_loads_for_caller_side_policy() {
        // Loading must NOT fail for a tiny note: one atomic stub in a
        // multi-note selection must never kill the whole job. The CLI skips
        // tiny notes with a warning; the sidecar groups them into a combined
        // envelope. Size policy lives with those callers.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.md");
        fs::write(&path, "# TODO\n").unwrap();

        let source = load_source(path).unwrap();
        assert!(source.extracted_text.trim().chars().count() < MIN_TEXT_SOURCE_CHARS);
    }
}
