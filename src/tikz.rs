//! TikZ → SVG rendering with a hard compile gate.
//!
//! The author model emits TikZ; this module compiles it with the local TeX
//! Live (standalone class) and converts to SVG via dvisvgm. A figure that
//! fails to compile is DROPPED (the item survives — stems must be fully
//! specified in text; figures aid intuition). Colors are restricted to the
//! Flexoki palette, emitted as light-mode (600-shade) hexes; the app remaps
//! them to the dark-mode (400-shade) pairs at render time.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Flexoki definitions the author references by name. Light documents use
/// the 600 shades on paper-black ink; dark documents use the 400 shades on
/// paper-white ink (stephango.com/flexoki).
const LIGHT_COLORS: &str = r#"
\definecolor{fxtx}{HTML}{100F0F}
\definecolor{fxtx2}{HTML}{6F6E69}
\definecolor{fxtx3}{HTML}{B7B5AC}
\definecolor{fxui}{HTML}{E6E4D9}
\definecolor{fxred}{HTML}{AF3029}
\definecolor{fxorange}{HTML}{BC5215}
\definecolor{fxyellow}{HTML}{AD8301}
\definecolor{fxgreen}{HTML}{66800B}
\definecolor{fxcyan}{HTML}{24837B}
\definecolor{fxblue}{HTML}{205EA6}
\definecolor{fxpurple}{HTML}{5E409D}
\definecolor{fxmagenta}{HTML}{A02F6F}
"#;

const DARK_COLORS: &str = r#"
\definecolor{fxtx}{HTML}{CECDC3}
\definecolor{fxtx2}{HTML}{878580}
\definecolor{fxtx3}{HTML}{575653}
\definecolor{fxui}{HTML}{282726}
\definecolor{fxred}{HTML}{D14D41}
\definecolor{fxorange}{HTML}{DA702C}
\definecolor{fxyellow}{HTML}{D0A215}
\definecolor{fxgreen}{HTML}{879A39}
\definecolor{fxcyan}{HTML}{3AA99F}
\definecolor{fxblue}{HTML}{4385BE}
\definecolor{fxpurple}{HTML}{8B7EC8}
\definecolor{fxmagenta}{HTML}{CE5D97}
"#;

enum Engine {
    Pdflatex(String),
    Tectonic(String),
}

/// Engine preference: system TeX Live (developer machines), then tectonic —
/// a self-contained single-binary TeX engine we can ship INSIDE the app
/// bundle next to the sidecar, so end users need no TeX install at all.
fn engine() -> Option<Engine> {
    let pdflatex = "/Library/TeX/texbin/pdflatex";
    if Path::new(pdflatex).exists() {
        return Some(Engine::Pdflatex(pdflatex.to_owned()));
    }
    // Bundled next to this executable (packaged app), then common installs.
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("tectonic").display().to_string());
        }
    }
    candidates.extend([
        "/opt/homebrew/bin/tectonic".to_owned(),
        "/usr/local/bin/tectonic".to_owned(),
    ]);
    for candidate in candidates {
        if Path::new(&candidate).exists() {
            return Some(Engine::Tectonic(candidate));
        }
    }
    for tool in ["pdflatex", "tectonic"] {
        if Command::new(tool)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(if tool == "pdflatex" {
                Engine::Pdflatex(tool.to_owned())
            } else {
                Engine::Tectonic(tool.to_owned())
            });
        }
    }
    None
}

#[allow(dead_code)] // preflight probe (tests, future UI hint)
pub fn available() -> bool {
    engine().is_some()
}

/// Compile a tikzpicture body (or full \begin{tikzpicture} block) to PDF
/// bytes. PDF (not SVG) because macOS renders PDF natively in NSImage —
/// no poppler/ghostscript dependency for end users. Every failure path
/// returns Err; the caller drops the figure, never the item.
#[allow(dead_code)] // light-theme convenience used by tests
pub fn render_pdf(tikz: &str, work_dir: &Path) -> Result<Vec<u8>> {
    render_pdf_themed(tikz, work_dir, false)
}

/// Render with the light (600-shade) or dark (400-shade) Flexoki table.
pub fn render_pdf_themed(tikz: &str, work_dir: &Path, dark: bool) -> Result<Vec<u8>> {
    let body = tikz.trim();
    if body.is_empty() {
        bail!("empty tikz body");
    }
    if body.len() > 8_000 {
        bail!("tikz body too large");
    }
    // The TeX source is model-generated from untrusted notes: forbid the
    // escape hatches (shell escape is off by default, but belt and braces).
    let lower = body.to_lowercase();
    for forbidden in ["\\write18", "\\input", "\\include", "\\openout", "\\read", "\\csname"] {
        if lower.contains(forbidden) {
            bail!("tikz contains forbidden primitive {forbidden}");
        }
    }
    let picture = if body.contains("\\begin{tikzpicture}") {
        body.to_owned()
    } else {
        format!("\\begin{{tikzpicture}}\n{body}\n\\end{{tikzpicture}}")
    };
    let palette = if dark { DARK_COLORS } else { LIGHT_COLORS };
    let document = format!(
        "\\documentclass[tikz,border=6pt]{{standalone}}\n\
         \\usetikzlibrary{{arrows.meta,calc,angles,quotes,patterns,decorations.pathmorphing,decorations.markings,positioning}}\n\
         {palette}\n\
         \\begin{{document}}\n\
         \\color{{fxtx}}\n\
         {picture}\n\
         \\end{{document}}\n"
    );

    std::fs::create_dir_all(work_dir)?;
    let stamp = format!("fig-{:x}", md5ish(&document));
    let tex_path = work_dir.join(format!("{stamp}.tex"));
    std::fs::write(&tex_path, &document)?;

    let status = match engine().context("no TeX engine (TeX Live or tectonic) available")? {
        Engine::Pdflatex(binary) => Command::new(binary)
            .current_dir(work_dir)
            .args([
                "-interaction=nonstopmode",
                "-halt-on-error",
                "-no-shell-escape",
            ])
            .arg(format!("{stamp}.tex"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("spawning pdflatex")?,
        Engine::Tectonic(binary) => Command::new(binary)
            .current_dir(work_dir)
            .args(["--untrusted", "--chatter", "minimal", "-o", "."])
            .arg(format!("{stamp}.tex"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("spawning tectonic")?,
    };
    if !status.success() {
        bail!("TeX engine failed (tikz does not compile)");
    }
    let pdf = std::fs::read(work_dir.join(format!("{stamp}.pdf"))).context("reading pdf")?;
    if !pdf.starts_with(b"%PDF") {
        bail!("engine produced no valid PDF");
    }
    Ok(pdf)
}

/// Cheap content hash (no external dep): FNV-1a over the document.
fn md5ish(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> std::path::PathBuf {
        std::env::temp_dir().join("whetstone-tikz-tests")
    }

    #[test]
    fn valid_tikz_compiles_to_pdf() {
        if !available() {
            return;
        }
        let pdf = render_pdf(
            "\\draw[fxblue, thick, ->] (0,0) -- (2,1) node[right, fxtx]{$v$};\n\\draw[fxred] (0,0) circle (0.5);",
            &work(),
        )
        .expect("compiles");
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn broken_tikz_is_rejected_not_shipped() {
        if !available() {
            return;
        }
        assert!(render_pdf("\\draw[unclosed (0,0 -- ;", &work()).is_err());
    }

    #[test]
    fn escape_hatches_are_refused() {
        assert!(render_pdf("\\write18{rm -rf /} \\draw (0,0);", &work()).is_err());
        assert!(render_pdf("\\input{/etc/passwd} \\draw (0,0);", &work()).is_err());
    }
}
