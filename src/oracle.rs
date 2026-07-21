//! Deterministic verification oracle (architecture §9.2).
//!
//! Runs the author's verification script through the embedded Python harness
//! (`oracle_harness.py`, AST-whitelisted, rlimited). The only verdicts are
//! proved / disproved / unsupported; unsupported NEVER passes a gate — those
//! items fall back to the blind-agreement gate, exactly like the old engine.
//!
//! Verification is free and local, so it runs on every candidate that claims
//! a computable key. This is the piece that decouples item difficulty from
//! blind-solver ability: an item the solver cannot crack still verifies.

use crate::model::Verification;
use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const HARNESS: &str = include_str!("oracle_harness.py");
/// Wall-clock backstop above the harness's own 10s CPU rlimit.
const WALL_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Oracle {
    harness_path: std::path::PathBuf,
    available: bool,
}

impl Oracle {
    /// Materializes the harness into the cache dir and checks python3+sympy.
    pub fn prepare(cache_dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        let harness_path = cache_dir.join("oracle_harness.py");
        std::fs::write(&harness_path, HARNESS)
            .with_context(|| format!("writing {}", harness_path.display()))?;
        let available = Command::new("python3")
            .args(["-E", "-c", "import sympy"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        Ok(Self {
            harness_path,
            available,
        })
    }

    pub fn available(&self) -> bool {
        self.available
    }

    /// Runs one verification script. Every failure path maps to `unsupported`
    /// so the caller's gate logic stays a three-way match, never an error.
    pub fn verify(&self, verification: &mut Verification, keyed_option: &str, options: &[String]) {
        if verification.kind == "none" || verification.script.trim().is_empty() {
            verification.verdict = "not_run".into();
            verification.detail = "item declares no computable key".into();
            return;
        }
        if !self.available {
            verification.verdict = "unsupported".into();
            verification.detail = "python3 with sympy is unavailable on this machine".into();
            return;
        }
        // The script must be judged against the ITEM's key, not whatever
        // internal letter map it declares: inject the keyed option text and
        // the real option order as constants every script can (and new
        // scripts must) assert against.
        let mut preamble = String::new();
        let clean = |text: &str| text.replace("'", "\\'");
        preamble.push_str(&format!("KEYED_OPTION = '{}'\n", clean(keyed_option)));
        preamble.push_str("OPTIONS = [");
        for option in options {
            preamble.push_str(&format!("'{}', ", clean(option)));
        }
        preamble.push_str("]\n");
        let script = format!("{preamble}{}", verification.script);
        match self.run_harness(&script) {
            Ok((verdict, detail)) => {
                verification.verdict = verdict;
                verification.detail = detail;
            }
            Err(error) => {
                verification.verdict = "unsupported".into();
                verification.detail = format!("oracle harness failure: {error:#}");
            }
        }
    }

    fn run_harness(&self, script: &str) -> Result<(String, String)> {
        // -E ignores PYTHONPATH/PYTHONSTARTUP style env injection while keeping
        // user site-packages, where the pip --user sympy install lives (-I
        // would hide it). In-process containment is the harness's AST gate.
        let mut child = Command::new("python3")
            .arg("-E")
            .arg(&self.harness_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawning python3 oracle")?;
        child
            .stdin
            .take()
            .context("oracle stdin unavailable")?
            .write_all(script.as_bytes())
            .context("writing script to oracle")?;
        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(_) => break,
                None if start.elapsed() > WALL_TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok((
                        "unsupported".into(),
                        "verification exceeded the wall-clock limit".into(),
                    ));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        let output = child.wait_with_output().context("collecting oracle output")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().last().unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(line).context("oracle emitted no verdict JSON")?;
        let verdict = parsed
            .get("verdict")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unsupported")
            .to_owned();
        let detail = parsed
            .get("detail")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok((verdict, detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Verification;

    fn oracle() -> Oracle {
        let dir = std::env::temp_dir().join("whetstone-oracle-tests");
        Oracle::prepare(&dir).expect("harness materializes")
    }

    fn verification(kind: &str, script: &str) -> Verification {
        Verification {
            kind: kind.into(),
            script: script.into(),
            verdict: "not_run".into(),
            detail: String::new(),
        }
    }

    #[test]
    fn correct_sympy_derivation_is_proved() {
        let oracle = oracle();
        if !oracle.available() {
            return; // machine without sympy: oracle degrades, tests stay honest
        }
        let mut v = verification(
            "sympy",
            r#"
import sympy
x = sympy.symbols('x', positive=True)
# f(xy) = f(x)/y with f(30)=20 forces f(x) = 600/x; key claims f(40) = 15.
f = 600 / x
assert sympy.simplify(f.subs(x, 30)) == 20
assert sympy.simplify(f.subs(x, 40)) == 15
"#,
        );
        oracle.verify(&mut v, "42", &[]);
        assert_eq!(v.verdict, "proved", "{}", v.detail);
    }

    #[test]
    fn wrong_key_is_disproved_not_errored() {
        let oracle = oracle();
        if !oracle.available() {
            return;
        }
        let mut v = verification("numeric", "assert 2 + 2 == 5, 'key contradicted'");
        oracle.verify(&mut v, "42", &[]);
        assert_eq!(v.verdict, "disproved");
    }

    #[test]
    fn filesystem_and_import_escapes_are_unsupported() {
        let oracle = oracle();
        if !oracle.available() {
            return;
        }
        for script in [
            "import os\nassert True",
            "open('/etc/passwd')",
            "__import__('subprocess')",
            "().__class__.__mro__",
        ] {
            let mut v = verification("numeric", script);
            oracle.verify(&mut v, "42", &[]);
            assert_eq!(v.verdict, "unsupported", "script escaped: {script}");
        }
    }

    #[test]
    fn infinite_loop_hits_the_limit_as_unsupported() {
        let oracle = oracle();
        if !oracle.available() {
            return;
        }
        let mut v = verification("numeric", "while True:\n    pass");
        oracle.verify(&mut v, "42", &[]);
        assert_eq!(v.verdict, "unsupported");
    }

    #[test]
    fn declared_non_computable_items_are_not_run() {
        let oracle = oracle();
        let mut v = verification("none", "");
        oracle.verify(&mut v, "42", &[]);
        assert_eq!(v.verdict, "not_run");
    }
}
