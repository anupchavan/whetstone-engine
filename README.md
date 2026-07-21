# Whetstone Engine

Turn a folder of Markdown notes into exam-grade, machine-verified adaptive
practice questions — locally, with your own model provider.

This is the open-source core behind the [Whetstone apps](https://github.com/anupchavan/whetstone-releases/releases)
and the Adaptive Practice Obsidian plugin. It authors questions from your
own notes through a multi-stage pipeline: seeded authoring, a sympy oracle
that machine-verifies computable answers, blind solver probes, a grounded
fidelity reviewer, a source-blind clarity gate with bounded wording repair,
and Elo difficulty calibration.

## Install

```sh
cargo install --path .
whetstone man | sudo tee /usr/local/share/man/man1/whetstone.1 > /dev/null
```

## Use

```sh
# Generate 10 verified questions from a notes folder with a local model
whetstone generate --input ~/notes/physics --count 10 --provider ollama --model llama3.1

# Or with an API provider
ANTHROPIC_API_KEY=sk-ant-… whetstone generate --input ~/notes --count 10

# Editorial depth: scholar | deep_work | olympiad_studio
whetstone generate --input ~/notes --count 10 --quality-tier scholar

# Run as a JSON-lines sidecar (what the apps and plugin speak)
whetstone serve
```

Outputs land in `results/`: a Markdown question set, the JSON items, and a
job ledger with per-call spend accounting. Generation is resumable — rerun
the same command and completed work is reused, never re-billed.

Providers: `anthropic`, `gemini`, `openai`, `ollama` (local, keyless),
`claude-code` and `codex` (run on your existing subscriptions via their
CLIs). A hard budget cap (`--budget-usd`) bounds every job.

The workspace also builds `quiz-server`, a self-hostable classroom quiz
host (live rooms, WebSocket protocol, zero persistence).

## Verification, honestly

Questions with computable answers are proved or rejected by a local sympy
oracle. Conceptual questions need independent blind-solver agreement.
Figures are compiled TikZ. Anything that fails a gate is rejected with a
recorded reason — the ledger shows exactly what was discarded and why.

## License

AGPL-3.0. Commercial licensing for closed-source embedding is available —
open an issue.
