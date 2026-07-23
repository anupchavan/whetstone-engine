# Whetstone Question Generation Quality Review

Last updated: 23 July 2026

## Purpose

This is a living review of Whetstone's generated questions and the architecture
that produces them. New examples should update the evidence in this document
rather than automatically creating a new prompt rule or code change.

The objective is not to make every question unusually difficult. The objective
is to generate a healthy mixture in which:

- foundation questions are clear, accurate, and diagnostically useful;
- application questions require the learner to use the source concept;
- hard questions earn their difficulty through reasoning, not obscurity;
- creative questions remain anchored to the learning notes;
- answer choices do not make the underlying question artificially easy.

## Review policy

### No-change is a valid conclusion

A question does not need a criticism or an engine change merely because it was
submitted for review. Each example may result in one of these decisions:

1. **Keep** — good as generated; no action.
2. **Keep and observe** — acceptable, with a minor pattern worth watching.
3. **Targeted repair** — preserve the question and repair only wording,
   distractors, or scenario consistency.
4. **Engine change candidate** — repeated evidence indicates a systematic
   generation or validation problem.
5. **Immediate correction** — reserved for wrong keys, material ambiguity,
   unsupported claims, or serious technical errors.

One merely imperfect question is normally insufficient evidence for an
architectural change. Repeated failures with the same cause are much stronger
evidence.

### Evaluation dimensions

Questions are considered across the following dimensions:

- **Source grounding:** Is the tested knowledge stated in or validly derived
  from the selected learning notes?
- **Construct alignment:** Does solving the question genuinely require the
  skill it claims to assess?
- **Learning value:** Does answering or reviewing it strengthen a useful
  mental model?
- **Difficulty calibration:** Is its delivered difficulty consistent with the
  reasoning actually required and the learner's current skill on the source
  topic?
- **Distractor quality:** Does each wrong choice represent a plausible and
  meaningfully distinct mistake?
- **Clarity:** Can a learner who knows the subject parse the setup without
  having the source note open?
- **Scenario consistency:** Are the instruments, units, terminology, and
  physical or computational mechanisms coherent?
- **Originality:** Does it transform the source rather than merely quote it?

Scores are descriptive, not hard acceptance thresholds.

### Source extensions are allowed

A question may introduce a new scenario, object, measurement, constraint, or
elementary fact that does not appear verbatim in the originating note. This is
not automatically hallucination or source drift.

An extension is acceptable when:

- the new information is stated clearly in the question;
- the remaining reasoning uses the note's concepts;
- the answer is derivable from the stated extension plus the notes and ordinary
  prerequisites;
- the learner is not expected to know an unstated specialist fact;
- the extension does not contradict or silently distort the source.

Reviews should therefore separate **source fidelity** from **construct
diagnosticity**. A question can be perfectly fair and source-faithful while
supplying so much source knowledge in the stem that a correct answer provides
only modest evidence that the learner previously mastered the note. Such a
question may still be valuable for teaching or application and should normally
be retained with an appropriate role or mastery weight.

### Learner skill is required review context

Question difficulty and usefulness must be judged against Whetstone's current
learner state for the exact source note, not against an expert reviewer in
isolation.

For every future review:

1. Read the running app's current session state when available.
2. Read the live learner record for the question's exact `source_path`.
3. Record skill, observation count, delivered difficulty/role, and result when
   available.
4. Judge whether the question creates appropriate effort at that skill level.
5. Do not criticize a direct or moderately scaffolded question merely because
   it is easy for an expert.

Low skill makes clear cues, standard applications, and explicit setup more
appropriate. High skill increases the value of cue concealment, transfer,
multi-step composition, and stronger distractors.

**Do not inspect or use elapsed response time when reviewing question
quality.** The learner may leave the app open or spend time on unrelated
activity, so duration is not reliable evidence of difficulty, engagement, or
productive struggle. Correctness is useful learner-state evidence, but a single
attempt is still not enough to justify a broad engine change.

## Current architecture: strengths to preserve

The current engine already has a strong foundation:

- Notes are selected using learner weakness, freshness, and related-folder
  clustering.
- Session note count aims for roughly two questions per note and is bounded for
  context and cost.
- Substantive notes author independently; only short notes from the same folder
  are composed.
- Source-grounded seeds declare a specific skill and source kind.
- Analytical moves, setup mutations, operators, bridges, cue visibility, and
  difficulty rungs create structural variety.
- Computable questions can be checked by a local oracle.
- A source-blind solve checks the key and contributes provisional difficulty
  evidence.
- A grounded review checks fidelity, correctness, presentation, construct
  quality, and novelty.
- Compromised questions can be downgraded and assigned less mastery weight
  instead of being treated as equally diagnostic.

The next improvements should refine this system, not replace it.

## Architecture suggestions under consideration

These are proposals, not an instruction to implement every item. Their status
should change as more generated questions provide evidence.

### 1. Make the initial blind solve genuinely blind

**Current concern:** The blind solver receives the answer options and the
declared `target_skill`. Choices can enable elimination, and the named skill can
function as a hint even when the prompt says otherwise.

**Suggested design:**

1. Give an optionless solver only the stem.
2. Record its answer and reasoning route.
3. Give a separate solver the stem and options.
4. Measure whether the options created a large performance gain.
5. Reveal `target_skill` only to a later construct auditor.

This would distinguish retrieval from recognition and make difficulty and
mastery weight more trustworthy.

**Evidence status:** Strong architectural case; not dependent on one bad
question.

### 2. Add a narrow adversarial distractor audit

**Current concern:** The grounded reviewer performs many jobs at once and may
accept options that use different nouns but collapse under the same generic
elimination rule.

**Suggested design:** Without initially seeing the author's rationales, the
auditor should:

- construct the most charitable reasoning path to every choice;
- identify the misconception behind every wrong choice;
- flag two or more choices rejected by the same shortcut;
- detect stylistic clues in the keyed choice;
- detect an option that closely repeats source wording while the others sound
  invented.

Weak distractors should normally trigger a distractor-only repair, not deletion
of an otherwise useful question.

**Evidence status:** Supported by Review Q-001 and Q-022. Q-022 is a clean
second example: all three wrong options preserve incidental features of the
source's 8-bit example, so the learner can reject them together merely by
noticing that the width varies.

### 3. Use the complete reviewer verdict in acceptance

**Current concern:** `ReviewResult` includes `accept` and
`distractor_independence`, but the main acceptance expression does not directly
consume either field. A non-null blind-solver issue can also survive into an
otherwise all-green validation result.

**Suggested policy:**

- correctness or fidelity failure: reject;
- weak distractor independence: repair once;
- leakage or generic bypass: downgrade mastery evidence;
- failed repair of a foundation item: optionally retain with low weight;
- failed repair of a higher-rung item: replace it.

**Evidence status:** Concrete implementation mismatch; high-confidence
candidate. Q-021 strengthens the case: the blind solver explicitly notes that
the keyed design assumes an existing polymorphic extension point, but the
grounded reviewer calls the stem self-contained and every gate passes.

### 4. Extend bounded component repair

The existing wording repair is a good pattern. Add two similarly constrained
repairs:

- **Distractor repair:** Freeze the stem, key, and solution. Replace only weak
  choices.
- **Scenario repair:** Freeze the tested reasoning and key. Correct only
  terminology, units, instruments, or world consistency.

Every repair should be independently rechecked.

**Evidence status:** Supported by Q-001 and Q-016. Q-016 specifically shows a
missing topology rule that can be repaired without changing the intended
reasoning or key. Low risk because load-bearing content remains frozen.

### 5. Plan a compact blueprint before full authoring

Before generating polished prose and verification material, construct a small
item blueprint:

- target skill;
- source premise;
- proposed transfer context;
- decisive inference chain;
- intended tempting route;
- distinct misconception paths;
- binding conditions;
- external prerequisite facts;
- expected assessment level.

Rejecting an unsuitable seed–move pairing at this stage is cheaper than
repairing a complete question.

**Evidence status:** Promising hypothesis. Do not implement solely because a
single generated question is ordinary.

### 6. Generate the open problem before its choices

For conceptual multiple-choice questions:

1. Author an open-ended problem and solution.
2. Obtain an independent solution attempt.
3. Derive distractors from realistic mistakes and source misconceptions.
4. Convert those mistakes into choices.
5. Compare optionless and option-assisted performance.

This should improve distractors without forcing every question to become more
complex.

**Evidence status:** Promising, but likely more expensive. Evaluate against the
dedicated distractor audit first.

### 7. Enrich seed metadata only where useful

Possible additions include:

- prerequisite knowledge;
- common misconceptions;
- non-examples;
- observable failure symptoms;
- boundary cases;
- assumptions that may be varied;
- facts that may safely be treated as elementary.

**Evidence status:** Useful direction, but avoid turning seed extraction into an
oversized speculative schema.

### 8. Classify the note's instructional center before authoring

**Current concern:** Source fidelity and a valid target skill are not enough to
show that a question has “read the room.” A coding note may contain examples,
edge cases, and concrete inputs, but its instructional center may be algorithm
design, implementation, invariants, complexity, or debugging. A generator can
currently select one secondary fact, build a valid instance around it, and
label the result high-rung transfer even though the learner merely executes the
already-described algorithm by hand.

**Suggested design:**

Before seed selection, classify each substantive note's dominant learning
intent, allowing more than one when genuinely present:

- concept or mechanism;
- factual classification;
- mathematical derivation;
- algorithm selection and design;
- code implementation;
- debugging and failure diagnosis;
- proof, invariant, or complexity analysis;
- procedural execution or worked-example practice.

Carry this as `instructional_center` or `assessment_affordances` into the
blueprint and reviewer. The blueprint should state both:

- **what source fact the question uses**, and
- **what learner capability the source is mainly trying to develop**.

For algorithm-and-code notes, prefer questions that require at least one of:

- choosing, completing, or repairing a general implementation;
- identifying the invariant maintained by the pointers or state;
- explaining why sorting plus a sweep is sufficient;
- distinguishing the brute-force and optimized approaches;
- deriving time or space complexity;
- finding a counterexample for a buggy comparison or update order;
- predicting behavior after a meaningful code mutation;
- designing an adversarial test that exposes an implementation error.

A concrete instance may still be useful as:

- a low-weight foundation dry run;
- a test case embedded inside a debugging or code-selection question;
- a boundary-case check after the general method has been assessed; or
- one item in a deliberately varied set.

But merely computing the output of one small instance should not normally be
called high-rung transfer for a code-centered note. Its role and mastery weight
should reflect execution practice, not algorithmic competence.

For a future LeetCode-style mode, treat the original problem contract,
constraints, baseline approach, optimized approach, invariant, edge cases,
complexity, and implementation as a structured problem family. Instance
evaluation should support those tasks rather than replace them.

**Reviewer addition:** Ask:

> If the learner answers this correctly, what important capability emphasized
> by the note have they demonstrated?

If the honest answer is only “they evaluated this one input,” the item may
remain as a foundation check, but should fail a transfer label unless the
source itself is chiefly about procedural execution.

**Evidence status:** Supported directly by Q-006 and by the earlier
sensor-adjusted railway-platform example. The user reports that the same
failure recurs across coding notes. This is an engine-change candidate, though
the exact implementation should be coordinated with the planned
LeetCode-style question mode.

### 9. Audit every lost function in component-removal questions

**Current concern:** A hidden-assumption or cost-cutting question may remove a
component and ask when the removal is safe. The author or reviewer can focus on
one salient function while silently ignoring another function stated in the
same source. A condition that neutralizes only one consequence of removal is
then accepted as sufficient.

**Suggested design:**

For every component deletion, substitution, or “redundant layer” scenario:

1. Build a role ledger containing every function attributed to the component
   by the source and by any explicitly adopted real-world model.
2. List what requirement or failure mode corresponds to each lost function.
3. Require the proposed condition to neutralize **all** load-bearing lost
   functions.
4. Reject “safe only if X” when X addresses merely one item in the ledger.
5. If the question intentionally uses a simplified model, state that scope in
   the stem and avoid phrases such as “actually valid” that imply unrestricted
   real-world validity.
6. When real-world engineering behavior materially differs from the note,
   either supply the missing facts or keep the question explicitly within the
   note's simplified model.

**Reviewer question:**

> After deleting this component, which of its documented functions remain
> uncompensated by the keyed condition?

This check should be separate from ordinary source fidelity. A question can
quote every relevant source sentence yet still draw an invalid sufficiency
conclusion from only one of them.

**Evidence status:** Q-011 is direct evidence and contains an objective
correctness problem: the source assigns the shield both inward EMI rejection
and outward-leakage prevention, while the keyed condition handles only inward
EMI. High-confidence candidate.

### 10. Require provenance for every decisive inference

**Current concern:** A grounded reviewer can accept a question when one part of
its reasoning is supported by the note and the final answer is technically
correct, even though another indispensable inference comes from unstated,
advanced external knowledge.

This is especially dangerous when the question combines:

- one source-supported premise;
- one language-, framework-, or domain-specific edge rule; and
- a valid conclusion that requires both.

A single evidence citation may create the appearance of grounding without
covering the whole reasoning chain.

**Suggested design:**

The blueprint and grounded review should enumerate every decisive inference.
For each inference, record one provenance class:

- directly stated by the selected source;
- validly derived from cited source facts;
- explicitly supplied in the stem;
- ordinary prerequisite at the learner's level; or
- externally verified enrichment deliberately introduced by the system.

Acceptance should require complete inference coverage:

1. Every load-bearing inference has a provenance label.
2. Every source-derived inference links to specific evidence.
3. An advanced language or domain rule cannot be marked “ordinary prerequisite”
   merely because a validator model knows it.
4. If an enrichment is necessary, either state it in the stem or route the item
   through a mode that explicitly permits and teaches verified enrichment.
5. Correctness verification and grounding verification remain separate: a
   technically correct answer does not cure unsupported assessment content.

**Reviewer question:**

> Could a learner who fully mastered the cited note—but did not already know
> any uncited specialist rule—derive the answer?

If not, the item is not note-grounded transfer.

**Evidence status:** Q-013 directly demonstrates partial evidence coverage. The
note supports that an abstract base constructor runs for a subclass object, but
not C++'s special virtual-dispatch behavior during construction. The second
rule is decisive and unsupported. High-confidence candidate.

### 11. Smaller implementation observations

- The author role initially says “single-select” although later instructions
  support multiple-select and numeric questions.
- The networking notes currently classify as `general`, because the domain
  classifier lacks networking vocabulary.
- Conceptual grounded review currently uses low reasoning effort even though it
  is the primary protection for non-computable claims.
- A list of explicit inference steps would be more auditable than an
  author-supplied scalar inference count.
- A correct final key should not automatically validate every intermediate
  claim in the worked solution. Q-020 reaches the correct loop bound while
  falsely asserting that a pointer starting at 0 can advance only \(N-1\)
  times; the verifier exercises only the other termination branch and the
  reviewer accepts the proof unchanged.
- Presentation review should inspect the actual rendered explanation, not only
  the source string. Q-022 displays literal TeX fragments such as `^{n}` and
  `\bmod` in the learner-facing result despite relying on mathematical
  notation throughout its solution.

These are worth addressing, but they are not all equally important to learner
outcomes.

## Reviewed questions

### Q-001 — Encapsulation complete but no signal on the medium

**Decision:** Keep the underlying question; targeted scenario and distractor
repair would improve it.

**Approximate assessment:**

- Source grounding: 9/10
- Learning value: 7.5/10
- Difficulty: 4/10
- Overall quality: 7/10

**What worked:**

- It tested the boundary between header-producing layers and real
  transmission.
- It was strongly grounded in the encapsulation and physical-layer notes.
- It used a diagnostic symptom rather than asking for a layer definition
  directly.
- Its assigned foundation/application role was reasonable.

**What could improve:**

- Several wrong choices were variations of the same broad misconception.
- The instrument and medium wording mixed an ordinary oscilloscope, optical
  pulses, and “wire” imprecisely.
- The accepted mastery weight appeared somewhat stronger than the question's
  actual diagnostic value.

**Architectural evidence contributed:**

- Supports an adversarial distractor audit.
- Supports component-level scenario repair.
- Demonstrates why an optionless blind solve is useful.

### Q-002 — Front-only decapsulation leaves trailing bytes

**Decision:** **Keep. No engine change is justified by this question.**

**Approximate assessment:**

- Source grounding: 9.5/10
- Construct alignment: 9/10
- Learning value: 8.5/10
- Difficulty: 5.5/10
- Distractor quality: 7.5/10
- Clarity: 8.5/10
- Overall quality: 8.5/10

**What worked:**

- The question is directly grounded in the notes' distinction between ordinary
  headers and the frame trailer.
- It turns a source fact into a debugging symptom: unexpected bytes remain at
  the end after a front-only unwrapping model runs.
- Solving it requires locating the failed assumption in the engineer's model,
  not merely recalling the order of the layers.
- The symptom carries useful directional information without explicitly naming
  the required mechanism.
- The setup is self-contained and the simplification is clearly attributed to
  an engineer's model rather than presented as a complete implementation of a
  network stack.
- The choices represent recognizable confusions about which layers add or
  remove which information.

**Minor observations, not defects:**

- Several wrong choices invent a trailer at another layer. This creates some
  family resemblance between distractors, but here it is closely connected to
  the exact misconception being tested and does not make the question poor.
- “Real frames” is broad wording, but the surrounding description makes the
  relevant behavior understandable.
- The trailing-byte symptom makes the question approachable. That is
  appropriate for a strong application question; it does not need additional
  complication merely to become harder.

**Architectural evidence contributed:**

- Supports retaining symptom-to-cause and counterexample-style transformations.
- Shows that the current engine can produce a useful, source-aligned
  application question.
- Does not add evidence for a new prompt restriction or rejection rule.

### Q-003 — Choosing a vibration-resistant coaxial connector

**Decision:** **Keep and observe. Source-faithful and fair; no engine change is
justified by this question.**

**Approximate assessment:**

- Source grounding: 8.5/10
- Construct alignment: 7/10
- Learning value: 8/10
- Difficulty: 4.5/10
- Distractor quality: 8/10
- Clarity: 8.5/10
- Overall quality: 8/10

**Source-fidelity audit:**

The coaxial-cable note directly provides:

- BNC as Bayonet Neill-Concelman with a twist-lock and radio use;
- F-Type as threaded/screw-on with cable-TV and broadband use;
- N-Type as threaded with RF, outdoor, and wireless use;
- SMA as threaded with RF, antenna, and Wi-Fi use;
- TNC as Threaded Neill-Concelman with RF and communication use.

The question makes reasonable, transparent extensions:

- BNC and TNC sharing the Neill-Concelman family is directly inferable from
  their full names.
- Their overlapping RF/radio role is supported by their listed uses.
- The wind-vibrated mast and loosening connector are a novel application
  scenario rather than a claim copied from the note.
- “Spring-loaded quarter-turn bayonet” is more mechanically specific than the
  coaxial note's “twist-lock,” but the question states it explicitly.
- The separate fiber/connectors note explicitly describes TNC as being used
  where better vibration resistance is required, providing additional support
  within the learning vault.

No unstated specialist fact is required to distinguish the choices. The stem
provides the relevant locking styles and family relationship, while the notes
provide the connector identities and application categories. The answer can be
derived from that combined information.

**What worked:**

- The learner must satisfy two constraints simultaneously: address the
  mechanical failure while preserving connector family and intended use.
- The alternatives represent distinct partial solutions: preserve convention
  without fixing the fault, fix the locking style while changing the family or
  application, or satisfy both constraints.
- The “whatever is standard for radios” statement creates a useful
  proxy-versus-requirement distinction.
- The question converts a connector table into a design decision rather than
  asking for direct recall of a row.
- The scenario is concise and self-contained.

**Minor observations, not defects:**

- Nearly all decisive table facts are restated in the stem. A learner who has
  not studied the note can still perform the constraint match. This lowers its
  value as evidence of prior connector knowledge, but does not reduce its
  fairness or usefulness as an application exercise.
- “Thread-locked” could be read as implying a separate thread-locking compound.
  “Threaded coupling that better resists vibration” would be mechanically
  clearer, but the existing wording is understandable.
- Because one choice mirrors both stated constraints very explicitly, the
  question is easier than a frontier item. It remains a good moderate
  application question and does not need artificial complication.

**Architectural evidence contributed:**

- Reinforces the distinction between supplied facts and tested knowledge.
- Supports optionless solving as a way to measure how much note knowledge the
  question actually requires.
- Does not support rejecting questions merely because they introduce a new
  real-world scenario or explicitly supply unfamiliar facts.

### Q-004 — Counting passes of `n & (n - 1)`

**Decision:** **Keep the problem and repair one distractor's scope. No broad
engine change.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill: 3/100
- Recorded observations: 4
- Delivered role: Stretch
- Result shown in the running app: correct

At this learner level, the question should not be judged as trivial. It asks the
learner to connect the operation to popcount, apply it to a substantially larger
number than the note's example, and justify a lower bound. That reasoning
profile is consistent with useful stretch practice at skill 3; elapsed time is
not used as evidence.

**Approximate assessment at the current skill level:**

- Source grounding: 10/10
- Construct alignment: 8.5/10
- Learning value: 9/10
- Difficulty for this learner: 7.5/10
- Distractor quality: 7/10
- Clarity: 8/10
- Overall quality before the narrow repair: 8/10

**Source-fidelity audit:**

The note explicitly teaches that repeatedly applying `n & (n - 1)` clears the
rightmost set bit and that the number of repetitions equals the number of set
bits. The question's central reasoning is therefore directly and faithfully
grounded.

The additional minimality claim is also validly derived from the stated setup:
if every pass removes exactly one set bit and the process stops only after all
set bits are removed, no procedure under that same one-set-bit-per-pass
restriction can use fewer passes. No external specialist fact is required.

**What worked:**

- It transforms the note's small worked example into a fresh execution case.
- It tests popcount rather than confusing it with bit length.
- It adds a proof-of-minimality judgment instead of stopping at arithmetic.
- The bit-length and off-by-one alternatives correspond to plausible mistakes.
- The operation and loop semantics are fully stated, which is appropriate for
  a learner currently at skill 3.
- The explanation reinforces both the mechanism and the lower-bound argument.

**Targeted issue:**

The choice claiming that “a cleverer single-operation mask could finish in
fewer” is too broad as written. If an alternative operation may clear several
set bits at once—or simply force the value to zero—then the claim is arguably
true even though the displayed routine still takes the stated number of
passes. The intended contrast is valid only when the alternative remains
restricted to clearing exactly one set bit per pass.

This is an option-scope ambiguity, not a flaw in the core question. Repair only
that choice so it explicitly claims a supposedly cleverer **one-set-bit-per-pass**
mask can finish sooner. Freeze the stem, key, calculation, and explanation.

**Architectural evidence contributed:**

- Makes live learner skill a mandatory part of every future review.
- Positively supports the engine's adaptive use of direct but nontrivial
  questions for a low-skill topic.
- Adds a second example supporting a narrow adversarial option audit,
  specifically quantifier and comparison-class consistency.
- Does not support making the author prompt more complex or rejecting this
  question.

### Q-005 — Hub versus switch simultaneous aggregate throughput

**Decision:** **Keep. No repair or engine change is justified by this
question.**

**Live learner context:**

- Exact source: `Computer Networks/Full duplex vs half duplex.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: application
- Validated reasoning rung: 3
- Result shown in the running session: correct

Because this is effectively a first-observation topic, the question should not
be discounted as easy merely because its arithmetic is small. Its quality is
judged from the reasoning it requires, not from the elapsed attempt time.

**Approximate assessment for the current learner state:**

- Source grounding: 10/10
- Construct alignment: 9/10
- Learning value: 8.5/10
- Difficulty for this learner: 7/10
- Distractor quality: 9/10
- Clarity: 9/10
- Overall quality: 9/10

**Source-fidelity audit:**

The source note explicitly states all of the underlying conceptual facts:

- half duplex permits both directions, but only one direction is active at a
  time;
- hub-based Ethernet is an example of half duplex;
- full duplex permits both directions simultaneously; and
- modern switched Ethernet is an example of full duplex.

The question adds a numerical line rate, a diagnostic readout, and continuous
traffic in both directions. These are transparent scenario givens, not hidden
external knowledge. The required comparison follows entirely from those givens
plus the source's duplex classifications.

The answer also does not depend on an unstated convention about whether a
“100 Mb/s full-duplex link” is marketed as 100 or 200 Mb/s. The stem asks for
**simultaneous aggregate throughput** and explicitly supplies 100 Mb/s in each
direction, making the intended sum well-defined.

**What worked:**

- It tests the operational consequence of half versus full duplex instead of
  asking for their definitions.
- Holding the card report constant across both setups creates a useful
  invariant: the learner must recognize that identical endpoint capability
  does not imply identical system throughput.
- Continuous traffic in both directions is essential and well chosen. It makes
  simultaneous transmission capacity observable; without reverse traffic,
  full duplex would provide no throughput advantage for the stated workload.
- The four options form a clean two-by-two misconception grid: correctly or
  incorrectly classify the hub, and correctly or incorrectly classify the
  switch.
- The assumptions are explicit enough to avoid debates about particular
  hardware, negotiation, protocol overhead, or real-world contention.
- The explanation identifies the decisive distinction before performing the
  small calculation.

**Minor observation, not a defect:**

- “Aggregate throughput” is sometimes used inconsistently in informal
  networking discussions. A parenthetical such as “the sum of traffic moving
  in both directions at that instant” would make the wording completely
  convention-proof. Here, however, “simultaneous aggregate,” the continuous
  bidirectional workload, and the explicit duplex definitions already make the
  intended quantity sufficiently clear. This does not warrant repair.

**Architectural evidence contributed:**

- Positively supports the `invariant-under-change` and
  `instrument-vs-truth` reasoning moves.
- Shows that small arithmetic can still support a worthwhile application
  question when the conceptual distinction is the actual target.
- Provides a strong example of distractors built as a complete misconception
  matrix rather than as loosely related wrong statements.
- Supports preserving the current question and explanation pipeline; no new
  rejection rule, prompt restriction, or architecture change follows from
  this example.

### Q-006 — Manually evaluating one railway-platform instance

**Decision:** **Keep only as a low-weight foundation dry run, not as transfer.
The repeated pattern is an engine-change candidate.**

**Live learner context:**

- Exact source:
  `DSA/Greedy Algorithms/Minimum number of platforms required for a railway.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: transfer
- Catalog difficulty: very hard
- Validated reasoning rung: 4
- Mastery weight: 0.8
- Result shown in the running session: correct

The elapsed response time is deliberately excluded from this and future
reviews.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment with the declared narrow target: 9/10
- Construct alignment with the note's instructional center: 4/10
- Learning value: 5.5/10
- Difficulty as delivered: 3.5/10
- Clarity: 9/10
- Overall quality as a foundation dry run: 6.5/10
- Overall quality as a very-hard transfer item: 4/10

**Why the existing validators accepted it:**

The question is technically sound:

- the same-minute arrival/departure rule comes directly from the source;
- the inclusive boundary is genuinely load-bearing;
- changing the tie rule changes the computed result;
- the instance is self-contained;
- the answer is locally verifiable; and
- the explanation correctly traces the peak occupancy.

The validated target skill—formulating interval overlap with inclusive
boundaries—is genuinely exercised. The problem is not that the item is wrong,
unfaithful, or pointless.

**Why it did not read the room:**

The note is predominantly an algorithm-and-code lesson. It presents:

- a brute-force overlap-counting implementation;
- a sorted two-pointer greedy implementation;
- the `Arrival[i] <= Departure[j]` tie decision;
- pointer and platform-count updates; and
- comparative time and space complexity.

The generated question supplies the sweep behavior, the crucial same-minute
rule, and a small input, then asks the learner to calculate one output. A
correct response demonstrates that the learner can trace this instance and
honor one boundary condition. It does **not** provide strong evidence that the
learner can:

- derive or implement the sweep;
- choose it over the quadratic method;
- maintain or explain its invariant;
- reason about its complexity;
- identify the correct comparison in code without being told the rule; or
- generalize the method to arbitrary inputs.

Calling this rung-4, very-hard transfer and assigning mastery weight 0.8
therefore overstates what was assessed. The transformation changed the input,
but not the level of learner capability required.

**What remains useful:**

- It is a clean edge-case dry run.
- It highlights why arrivals must be processed before departures on a tie.
- The disjoint clusters prevent every interval from contributing to the peak.
- The explanation clearly connects the boundary convention to the sweep
  result.

This would be reasonable as an early foundation check or as a test case inside
a debugging question. It should not be the main assessment produced from this
code-centered note.

**Better directions for the same source:**

- Show two implementations differing only in `<` versus `<=` and ask which one
  satisfies the station rule, using this dataset as the counterexample.
- Remove the comparison branch and ask the learner to complete it.
- Ask for the invariant represented by `count` and why the maximum is the
  required answer.
- Present the quadratic and sorted-sweep approaches and ask which constraint
  makes one unsuitable.
- Introduce a plausible pointer-update bug and ask for the smallest failing
  test case.
- Ask the learner to derive the worst-case number of sweep iterations or the
  overall complexity from the code.

These retain the useful tie case while assessing algorithmic or implementation
understanding.

**Architectural evidence contributed:**

- Shows that correctness, novelty, a load-bearing condition, and a declared
  target skill can all pass while the item still misses the note's primary
  learning intent.
- Supports adding an instructional-center classification before seed
  selection and an alignment check during grounded review.
- Supports distinguishing instance execution from algorithmic transfer in
  rung assignment and mastery weighting.
- Reinforces the need for a dedicated code/LeetCode-style authoring path rather
  than trying to obtain code-learning questions through generic scenario
  transformations alone.

### Q-007 — Rebuilding a growing power set on every stream arrival

**Decision:** **Keep. This is a good transfer question for a code-centered
note.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Power set bit manipulation.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: transfer
- Catalog difficulty: hard
- Validated reasoning rung: 3
- Mastery weight: 0.85
- Result shown in the running session: incorrect

The result is useful to learner-state tracking, but it does not determine the
quality judgment. Elapsed response time is not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment with the note's instructional center: 9/10
- Learning value: 9/10
- Difficulty as delivered: 8/10
- Distractor quality: 9/10
- Clarity: 9/10
- Overall quality: 9/10

**Source-fidelity audit:**

The source note teaches:

- representing each subset with one value from `0` through `2^N - 1`;
- testing all `N` bit positions for every value;
- generating all `2^N` subsets; and
- deriving a single-build time complexity of `O(2^N * N)`.

The question preserves that exact algorithm and introduces one explicit
extension: the input grows one element at a time and the entire power set is
rebuilt after every arrival. No hidden implementation behavior or specialist
fact is assumed.

Solving the question requires composing the source's per-build complexity over
all prefix sizes and recognizing the behavior of an exponentially growing
sum. That asymptotic-series step is an ordinary prerequisite for algorithm
analysis and is fully motivated by the stated costs.

**Why this does read the room:**

The note is code-centered, but code writing is not its only instructional
purpose. Its implementation and complexity analysis are both substantial
parts of the lesson. This question directly assesses whether the learner can:

- recover the cost of the nested-loop implementation at prefix size `k`;
- express the total cost across repeated executions;
- avoid multiplying every execution by the final, largest cost; and
- reason about exponential domination asymptotically.

Unlike Q-006, the learner is not merely tracing one concrete input. The
streaming mutation changes the complexity problem while leaving the underlying
implementation recognizable. A correct answer therefore provides evidence of
general algorithm-analysis ability relevant to the note.

**What worked:**

- The mutation is compact and load-bearing: rebuilding on every arrival is what
  creates the sum.
- The stem includes enough implementation detail to make the analysis
  self-contained without simply stating the result.
- The tempting extra-factor alternative corresponds to a realistic mistake:
  charging every earlier prefix at the final prefix's cost.
- The remaining alternatives distinguish dropping the per-subset bit scan from
  changing the exponential base.
- The explanation derives the per-arrival cost before summing it, making the
  reasoning reusable.
- It teaches the useful principle that repeatedly recomputing exponentially
  growing prefixes can remain in the same asymptotic class as the final
  computation.

**Minor observations, not defects:**

- The question partly measures familiarity with geometric-series domination,
  not only power-set generation. That is an appropriate prerequisite for a
  hard algorithm-complexity transfer item, but mastery evidence should be
  interpreted as shared between those skills.
- “Costs nothing extra asymptotically” in the explanation means that the
  big-Theta class is unchanged, not that the repeated work has zero practical
  cost. The preceding derivation makes this sufficiently clear.
- The distinctness of the arriving integers is inherited from the source
  problem and is not important to the complexity result. It is harmless.

**Architectural evidence contributed:**

- Provides an important positive counterexample to Q-006: a question from a
  coding note need not ask the learner to write code if it genuinely assesses
  another instructional center present in the note.
- Supports multi-label instructional-center classification rather than a
  binary “coding versus conceptual” split.
- Shows that a `set-in-motion` mutation can produce real transfer when it
  changes the algorithm's cost composition rather than merely changing its
  input values.
- Supports retaining hard complexity-analysis questions in the future
  LeetCode-style family alongside implementation and debugging tasks.
- Does not justify a new rejection rule or repair.

### Q-008 — Clearing the same bit in a minimal pair

**Decision:** **Keep as medium application. It is not a strong transfer item,
but it does read the source section correctly.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill: 3/100
- Recorded observations: 4
- Catalog role: application
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.8
- Result shown in the running session: correct

Elapsed response time is not inspected or used.

**Approximate assessment at the current skill level:**

- Source grounding: 10/10
- Construct alignment: 9/10
- Learning value: 8/10
- Difficulty as delivered: 6/10
- Distractor quality: 8/10
- Clarity: 9/10
- Overall quality: 8/10

**Source-fidelity audit:**

The exact source section teaches that the `i`-th bit is cleared using:

```cpp
n & ~(1 << i)
```

It explains the mask and demonstrates the operation on a concrete binary
number. The generated question preserves this mechanism exactly. Both binary
representations are supplied, use zero-based bit position 4 consistently, and
differ only in the bit targeted by the mask. No external knowledge is required.

**Why this instance is appropriate:**

The note is broad, but this seed comes from a short section whose instructional
center is understanding and applying one constant-time bit operation. It is not
an algorithm-design lesson like the railway-platform note. For this particular
source section, concrete evaluation is a legitimate application task.

The use of two inputs is also purposeful. A single number would test only
whether the learner can execute the mask. The minimal pair—one input with the
target bit set and one with it already clear—reveals two semantic properties:

- clearing is idempotent when the target bit is already zero; and
- two values differing only at the cleared bit collapse to the same output.

The stem asks only for the output pair, so the learner can still solve it
mechanically. The explanation, however, makes the general property explicit.
That is enough for a good medium application question, though not enough to
claim high-rung transfer.

**What worked:**

- The binary forms make bit indexing and the sole input difference visible.
- Both important preconditions are exercised: target bit initially one and
  target bit initially zero.
- The choices represent distinct errors: returning the mask/test result,
  clearing the wrong value or bit, and treating an already-clear operation as
  the identity only for one input.
- The explanation moves from mask construction to both evaluations and then to
  the general collision property.
- At skill 3, explicit representations and a standard operation are
  appropriate scaffolding.

**Minor observations, not defects:**

- Because the transform and both binary inputs are supplied, this is more a
  careful application check than a retrieval or code-construction test.
- Mastery evidence is local: success supports clearing-bit semantics, not broad
  mastery of the large bit-manipulation note.
- A later, harder companion could ask about idempotence, information loss, or
  how many inputs map to the same output without requiring numerical
  evaluation. This question does not need to be replaced by that companion.

**Architectural evidence contributed:**

- Refines the instructional-center proposal: alignment should be evaluated at
  the seed or source-section level, not only from the genre of the whole note.
- Provides a positive example of an instance question that is appropriate
  because procedural execution is itself the source section's intended skill.
- Supports retaining minimal-pair transformations for operation semantics.
- Reinforces that role calibration matters: good application does not need to
  be relabeled as transfer.
- Does not justify a new engine change or repair.

### Q-009 — Rat-in-a-maze search with the restoration step removed

**Decision:** **Keep. This is a strong debugging/application question for a
code-centered note.**

**Live learner context:**

- Exact source: `DSA/Recursion/Rat in a maze.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: application
- Catalog difficulty: hard
- Validated reasoning rung: 3
- Mastery weight: 0.85
- Result shown in the running session: correct

Elapsed response time is not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment with the note's instructional center: 10/10
- Learning value: 9.5/10
- Difficulty as delivered: 8/10
- Distractor quality: 8/10
- Clarity: 9/10
- Overall quality: 9/10

**Source-fidelity audit:**

The source note explicitly teaches the complete backtracking cycle:

1. mark the current cell by setting it to zero;
2. recursively explore neighbours in the order up, left, down, right; and
3. restore the cell to one after all recursive branches return.

The question removes exactly the third step and supplies a small grid on which
that omission changes the result. The grid, movement constraints, mutation,
and neighbour order are all stated. No behavior is imported from outside the
source.

The fixed neighbour order is load-bearing because persistent mutations make
the surviving path depend on which branch contaminates the grid first. Stating
the order is therefore necessary precision rather than incidental detail.

**Why this instance is valuable:**

This is not merely “run the algorithm on another grid.” The instance acts as a
counterexample for a plausible refactoring bug. Solving it requires the learner
to understand:

- visited state represents membership in the **current path**, not a permanent
  global prohibition;
- returning from one recursive branch must restore state for sibling branches;
- a dead-end branch can corrupt a later valid branch even though it found no
  solution itself; and
- traversal order becomes observably important once restoration is broken.

Those are central backtracking and implementation ideas from the note. The
numerical path count is only the observable symptom through which they are
tested.

**What worked:**

- The removed line is a realistic and minimal code mutation.
- The chosen grid contains two valid paths sharing a bottleneck, making state
  restoration genuinely necessary.
- A dead-end is visited before the alternate valid route, so the bug cannot be
  dismissed as harmless on this input.
- The correct and buggy executions differ, giving the mutation diagnostic
  power.
- The explanation identifies the contaminated cell and traces how the sibling
  route is lost.
- The question tests debugging, execution tracing, recursion state, and the
  backtracking invariant together without requiring a large grid.

**Minor observations, not defects:**

- The choices are simple path counts. Their diagnostic value comes mainly from
  the stem and trace rather than richly differentiated verbal misconceptions.
  That is appropriate for a deterministic execution question.
- A learner could eventually obtain the result through careful simulation
  without articulating the invariant. The explanation supplies that
  generalization, and the code mutation still makes this substantially more
  informative than an ordinary dry run.
- A future companion could ask which restoration placement is correct or
  request the smallest counterexample grid, but this question does not need
  replacement.

**Architectural evidence contributed:**

- Provides another positive counterexample to the claim that instance-based
  questions from coding notes are inherently weak.
- Demonstrates the important distinction between an **instance as the whole
  task** and an **instance as a witness for a general implementation bug**.
- Strongly supports code-mutation, counterexample, and symptom-to-cause
  question families in the planned LeetCode-style path.
- Shows that traversal-order details should be included when a mutation makes
  output order-dependent.
- Supports the instructional-center gate proposed after Q-006; no additional
  engine restriction is justified.

### Q-010 — Sequential interval insertion in opposite orders

**Decision:** **Keep. Strong application with a useful invariant; observe the
distractor pattern.**

**Live learner context:**

- Exact source: `DSA/Greedy Algorithms/Insert interval.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: application
- Catalog difficulty: hard
- Validated reasoning rung: 3
- Mastery weight: 0.85
- Result shown in the running session: incorrect

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment with the note's instructional center: 9/10
- Learning value: 9/10
- Difficulty as delivered: 7.5/10
- Distractor quality: 7/10
- Clarity: 8.5/10
- Overall quality: 8.5/10

**Source-fidelity audit:**

The source note teaches the three-stage insertion routine:

1. copy intervals ending strictly before the new interval starts;
2. merge every overlapping or endpoint-touching interval by taking the minimum
   start and maximum end; and
3. copy the remaining intervals.

It also promises that the result remains sorted and non-overlapping. The
question applies that exact routine twice. The initial list satisfies the
preconditions, and the output after the first insertion again satisfies them,
so the second insertion is valid in either order.

The question explicitly states the endpoint convention. This is important
because one bridge touches an existing interval at its boundary; the source
code's strict “before” condition and inclusive merge condition support exactly
that behavior.

**Why this is more than an ordinary instance:**

The two new intervals are chosen so that neither insertion alone produces the
final connected component. The second insertion bridges components produced or
preserved by the first. Comparing both orders probes whether the learner
understands that the routine computes a canonical merged representation of the
union rather than performing a one-time local edit whose history remains
visible.

The learner may solve the item by tracing both orders, but the explanation
extracts the general invariant: insertion order cannot change the represented
union when each intermediate result is fully normalized. This is a meaningful
extension of the one-insertion implementation in the note.

**What worked:**

- Sequential insertion is a natural extension of the source algorithm.
- The intervals create a genuine chained bridge instead of merely repeating
  two independent merges.
- Endpoint touching is load-bearing and clearly specified.
- Both intermediate lists remain sorted and non-overlapping, preserving the
  routine's preconditions.
- The question contrasts historical insertion order with the final canonical
  union.
- The explanation traces both orders and then states the reusable invariant.

**Minor concern: distractor independence**

Most wrong choices are intermediate results obtained after only one of the two
insertions:

- one option presents the two possible first-step states as though they were
  final and order-dependent;
- the other two preserve one of those stale states regardless of order.

These are plausible mistakes, especially for a learner who stops after the
first merge. However, they share the same broad failure mode: ignoring the
second insertion's bridging effect. A stronger option set could include a
different misconception, such as merging only strict overlaps while failing to
merge endpoint touching, or assuming the two new intervals must be inserted
into the original list independently before normalization.

This weakness does not undermine the stem or key and does not require immediate
repair. It is additional evidence for the existing distractor-independence
audit.

**Architectural evidence contributed:**

- Provides another positive example of a coding-note question that tests an
  algorithmic invariant without requiring code writing.
- Shows that `ordering-dependence` and `extend-object` can produce real transfer
  when repeated operations reveal canonical-state behavior.
- Reinforces the distinction between a bare single-instance output question
  and an instance designed to expose composition or order invariance.
- Adds evidence for the existing adversarial distractor audit because three
  wrong answers cluster around stale intermediate states.
- Does not justify a new engine rule or rejection.

### Q-011 — Removing the metallic shield from a coaxial cable

**Decision:** **Immediate correction. The keyed condition is insufficient even
under the source note's simplified model.**

**Live learner context:**

- Exact source: `Computer Networks/Coaxial cables.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: foundation
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.8
- Result shown in the running session: correct

The result does not make the question valid. Elapsed response time is not
inspected or used.

**Approximate assessment before repair:**

- Source grounding: 9/10
- Construct alignment: 7/10
- Learning value: 6/10
- Technical correctness: 4/10
- Distractor quality: 8/10
- Clarity: 8/10
- Overall quality: 5.5/10

**The internal source contradiction:**

The note assigns two explicit functions to the metallic shield:

1. protecting the carried signal from external electromagnetic interference;
   and
2. preventing signal leakage outward.

It assigns physical protection to the outer jacket. The question correctly
distinguishes mechanical protection from electromagnetic protection, but its
key requires only that external EMI be negligible.

That condition explains why the received signal might remain clean. It does
not make removal safe or the designer's redundancy argument valid, because
outward leakage remains unaddressed. The explanation itself lists “stops
outward leakage,” then concludes that eliminating external interference is the
sole required condition. The conclusion therefore fails to account for one of
its own premises.

**The broader engineering problem:**

In a real coaxial transmission line, the metallic outer conductor is not merely
an optional noise screen. It normally provides the return-current path and,
together with the dielectric and geometry, contributes to controlled
impedance and field confinement. Removing it changes the transmission line
even in an environment with negligible external EMI.

Those functions are not described in the current note, so they should not be
silently required of the learner. However, the stem asks when the designer's
reasoning is “actually valid,” which reads as a real engineering claim rather
than a conclusion restricted to the note's simplified model. Under that
wording, none of the supplied choices is sufficient.

**Why the bench observation does not rescue the item:**

A clean test-bench signal is only one observed outcome under one environment
and setup. It does not establish:

- immunity across all deployment environments;
- acceptable outward emissions or leakage;
- a valid signal-return path;
- preserved characteristic impedance; or
- reliable operation over the intended distance and frequency range.

The question turns a limited successful test into a universal safety claim,
then offers a condition addressing only one of several missing guarantees.

**Repair options:**

**Minimal, source-bounded repair:**

- Explicitly say the question uses the note's simplified four-role model.
- State that outward signal leakage is irrelevant or acceptable for this
  hypothetical design.
- Ask which additional environmental condition would preserve the shield's
  remaining relevant function.

With those assumptions, negligible external EMI becomes sufficient within the
declared model.

**Better source-bounded repair:**

- Keep the component-removal scenario.
- Replace the key with a condition covering both documented shield functions:
  external EMI is negligible **and** outward signal leakage is acceptable.
- Recheck the choices and explanation.

**Real-engineering repair:**

- Add the outer conductor's return-path and impedance-control roles to the
  source or supply them in the stem.
- Include a “none of these conditions is sufficient” choice, or ask why the
  clean bench test does not establish a viable coaxial design.

The current stem and key should not be retained unchanged.

**What still worked:**

- The designer's use of the vague word “protection” creates a valuable
  function-versus-label distinction.
- Contrasting the jacket's mechanical role with the shield's electromagnetic
  role is pedagogically useful.
- The alternatives refer to distinct cable properties rather than repeating
  the same misconception.
- The hidden-assumption format is promising; the failure is incomplete
  functional accounting, not the format itself.

**Architectural evidence contributed:**

- Exposes a blind spot in source-faithful validation: citing a supported
  function is not enough when another supported function defeats sufficiency.
- Supports a role-ledger audit for component removal and substitution.
- Shows that `truth_status: source_faithful_only` should constrain wording;
  unrestricted real-world phrases such as “actually valid” require a broader
  technical check.
- Demonstrates why every premise used in the explanation must be reconciled
  with the final conclusion.
- Justifies immediate correction and a targeted validation improvement.

### Q-012 — XOR swap when both indices alias one array slot

**Decision:** **Keep. This is an excellent transfer/debugging question.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill: 3/100
- Recorded observations: 4
- Delivered role shown in the running app: Stretch
- Result shown in the running session: correct

Elapsed response time is not inspected or used.

**Approximate assessment at the current skill level:**

- Source grounding: 9.5/10
- Construct alignment: 10/10
- Learning value: 9.5/10
- Difficulty as delivered: 8.5/10
- Distractor quality: 9.5/10
- Clarity: 9/10
- Overall quality: 9.5/10

**Source-fidelity audit:**

The source presents the standard three-statement XOR swap:

```cpp
A = A ^ B;
B = A ^ B;
A = A ^ B;
```

It demonstrates the identity for two separate variables. The question
faithfully embeds those statements in an array-slot routine, then varies
whether the two arguments designate distinct storage locations. The aliasing
failure is not stated in the note, but it is directly derivable from the
supplied code and XOR's documented behavior. No unstated specialist fact is
required.

**Why this is genuine transfer:**

The learner must move beyond the value-level slogan “XOR swaps two numbers”
and identify an implicit representation-level precondition: the two operands
must be distinct writable locations.

When both indices refer to the same slot, the first assignment computes a
self-XOR and destroys the value. Later assignments cannot recover information
that has already been erased. This is a regime change caused by aliasing, not a
routine numerical example.

The equal values stored at two **different** indices are an especially good
control case. They tempt the learner to confuse equal values with the same
variable, while demonstrating that value equality is harmless and location
aliasing is the real failure condition.

**What worked:**

- The mutation from two variables to two indexed slots is natural and
  load-bearing.
- Exactly one option violates the routine's hidden distinct-location
  precondition.
- The equal-valued pair is a purposeful decoy, not arbitrary data.
- The self-swap case is a minimal counterexample that can be traced in one
  statement.
- The choices distinguish value equality, ordinary distinct-value swaps, and
  memory aliasing.
- The explanation first states the general identity, then separates the
  distinct-location and same-location regimes.
- At skill 3, the supplied array keeps the question self-contained while the
  hidden precondition still creates appropriate stretch.

**Minor observations, not defects:**

- XOR swap is generally less readable and less useful in production than an
  ordinary swap, but the question evaluates the stated routine rather than
  recommending it as best practice.
- The question demonstrates one aliasing failure. It does not by itself assess
  all XOR operations or broad bit-manipulation mastery; its mastery evidence
  should remain tied to the specific procedure and reasoning pattern.

**Architectural evidence contributed:**

- Strongly supports `counterexample-hunt` and `regime-shift` moves for code
  procedures.
- Shows how changing **operand identity** rather than merely operand values can
  reveal a hidden implementation precondition.
- Provides an exemplary distractor construction: one option isolates the
  common equal-values misconception while the key isolates actual aliasing.
- Supports adding aliasing, shared-state, and parameter-collision mutations to
  the future code/LeetCode-style question family.
- Provides another positive example where a tiny instance is a witness for a
  general bug rather than a substitute for algorithmic understanding.
- Does not justify repair or a new rejection rule.

### Q-013 — Virtual dispatch from an abstract base constructor

**Decision:** **Immediate grounding correction. Technically correct C++, but
not fair transfer from the selected note.**

**Live learner context:**

- Exact source: `Object Oriented Programming/Abstraction.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: transfer
- Catalog difficulty: hard
- Validated reasoning rung: 3
- Mastery weight: 0.85
- Result shown in the running session: incorrect

The result does not demonstrate weak abstraction knowledge because the
decisive language rule is absent from the learning note. Elapsed response time
is not inspected or used.

**Approximate assessment:**

- Technical correctness as a standalone C++ question: 9/10
- Source grounding: 4.5/10
- Construct alignment with the selected note: 5/10
- Learning value with the missing rule taught first: 8.5/10
- Fairness in the current adaptive session: 4.5/10
- Distractor quality: 9/10
- Clarity: 8.5/10
- Overall quality as currently sourced: 5.5/10

**What the note actually supports:**

The note directly teaches that:

- an abstract class cannot be instantiated on its own;
- an abstract class may have a constructor;
- that constructor runs when a subclass object is created;
- an abstract subclass may leave inherited abstract methods unimplemented; and
- a concrete descendant must implement the remaining abstract methods.

Those facts support the `A`/`B`/`C` hierarchy and establish that `A`'s
constructor executes during construction of `C`.

**The missing decisive rule:**

The note never teaches C++ virtual dispatch during construction. Predicting the
printed label requires knowing that a virtual call made from `A`'s constructor
does not dispatch to `B`'s override; during base construction, dispatch is
restricted to the class currently under construction.

That is a specific C++ object-lifetime rule. It is neither implied by
abstraction itself nor derivable from the note's examples of ordinary virtual
calls through base pointers. A learner can understand every abstraction fact
in the note and still reasonably select the derived override.

The validator's evidence cites only the paragraph about abstract-class
constructors. It supports the first inference but provides no evidence for the
second, decisive dispatch inference. The technical solver knew the C++ rule,
and that knowledge appears to have been mistaken for source grounding.

**Technical wording observation:**

The conclusion is correct in C++, but the explanation should state the
language's semantic rule directly. Saying “the vtable pointer refers to A's
table” describes a common implementation mechanism, not the portable language
guarantee and not something established by the source. The explanation should
avoid making vtable layout the basis of correctness.

**Why the question is still promising:**

As a question attached to material on constructors, object lifetime, and
virtual dispatch, it is strong:

- it combines two plausible misconceptions;
- every choice corresponds to a meaningful mental model;
- the abstract intermediate class is load-bearing;
- it distinguishes “cannot instantiate an abstract class” from “its base
  subobject and constructor never exist”; and
- it tests a real C++ edge case rather than superficial terminology.

The failure is provenance and placement, not the core C++ problem.

**Repair options:**

**Best repair: move it to the right learning material.**

- Add or select a note covering constructor order and virtual calls during
  construction and destruction.
- Cite both the abstract-constructor fact and the dispatch rule.
- Keep the stem and choices, then revalidate difficulty against that topic's
  learner state.

**Self-contained repair:**

- Explicitly state C++'s in-construction virtual-dispatch rule in the stem.
- Reclassify the item as application of a supplied rule rather than retrieval
  or transfer from the abstraction note.
- Reduce its mastery weight for abstraction because much of the decisive
  knowledge is supplied.

**Source-only replacement:**

- Ask which classes in the hierarchy remain abstract and which constructors
  run when `C` is created.
- Avoid virtual dispatch from constructors unless the note is expanded.

The current item should not count as strong mastery evidence for
`Abstraction.md`.

**Architectural evidence contributed:**

- Shows why evidence must cover every decisive inference, not merely one
  premise in a multi-step solution.
- Exposes a failure mode where a technically knowledgeable validator treats its
  own specialist knowledge as an ordinary learner prerequisite.
- Supports separate correctness and grounding verdicts.
- Supports rerouting good questions to the note that actually teaches their
  decisive rule instead of discarding them.
- Justifies a claim-by-claim provenance audit in the blueprint and grounded
  review.

### Q-014 — Bounding subset cardinality over a bitmask interval

**Decision:** **Keep as medium application. The question is structurally good;
the explanation could reveal the interval pattern more directly.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Power set bit manipulation.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: application
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.85
- Result shown in the running session: incorrect

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment: 9/10
- Learning value: 8.5/10
- Difficulty as delivered: 7/10
- Distractor quality: 8/10
- Clarity: 9/10
- Overall quality: 8.5/10

**Source-fidelity audit:**

The source explicitly teaches that:

- each integer `val` represents one subset;
- bit position `i` determines whether `nums[i]` is included; and
- the inner loop adds one array element for each set bit.

Therefore subset cardinality equals the number of set bits in `val`. The
question uses exactly that representation and adds a bounded-uncertainty task:
determine the tightest possible subset-size range when only an interval for
`val` is known. No external specialist rule is required.

**Why the chosen interval is good:**

The interval has deliberate binary structure:

```text
96  = 1100000₂
103 = 1100111₂
```

Across the entire interval, the two high bits remain fixed at one while the
three low bits take every pattern from `000` through `111`. The subset
therefore always contains the two elements selected by the fixed bits and may
contain anywhere from zero to three additional elements.

That gives a genuine bounds-propagation exercise. The learner must connect
subset size to popcount and then reason about fixed and varying bit positions.
This is stronger than evaluating one mask, while remaining appropriately
scoped as medium application rather than high-rung transfer.

**What worked:**

- The question targets the representation at the heart of the source
  algorithm.
- The array size and index range make the bit-to-element mapping explicit.
- The interval is small enough to verify but structured enough to solve without
  brute enumeration.
- The loosest-range distractor captures ignoring the interval constraint.
- The exact-size and narrow-range distractors capture sampling only typical or
  middle values.
- “Tightest” requires finding attained extremes rather than merely producing
  any safe bound.

**Minor improvement to the explanation:**

The current explanation groups all eight values by popcount. That is correct,
but it makes the solution look more enumerative than the question's design
requires.

A cleaner explanation would lead with:

> Every value in the interval has binary form `1100xyz`, where `xyz` spans all
> three-bit patterns. The two fixed ones contribute two elements, and the
> varying bits contribute between zero and three.

Enumeration can then be used only as a check. This would make the reusable
reasoning pattern clearer without changing the stem, choices, or key.

**Architectural evidence contributed:**

- Positively supports the `bounds-propagation` move for bitmask procedures.
- Shows that a bounded family of instances can test structural reasoning rather
  than merely repeated calculation when the interval is deliberately aligned
  with binary boundaries.
- Reinforces that code-centered notes may validly produce representation and
  invariant questions in addition to implementation tasks.
- Suggests that explanations should prefer the generating structure behind an
  interval over exhaustive enumeration when both are available.
- Does not justify a new engine rule or question repair.

### Q-015 — One-way traffic on full- and half-duplex links

**Decision:** **Keep. This is a strong foundation question built around a
useful limiting case.**

**Live learner context:**

- Exact source: `Computer Networks/Full duplex vs half duplex.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: foundation
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.85
- Result shown in the running session: correct

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 9.5/10
- Construct alignment: 9/10
- Learning value: 8.5/10
- Difficulty as delivered: 6/10
- Answer-format quality: 9/10
- Clarity: 9.5/10
- Overall quality: 8.5/10

**Source-fidelity audit:**

The note teaches that:

- half duplex permits only one direction at a time;
- turn-taking and waiting reduce performance;
- full duplex permits both directions simultaneously; and
- avoiding waiting gives full duplex its performance advantage.

The question makes the traffic pattern explicit: only one endpoint has data to
send. It also supplies the line rate, payload size, negligible reverse traffic,
and ideal-overhead assumption. From the source's mechanism, no turn-taking is
needed because only one direction demands the shared channel.

The conclusion is therefore a valid limiting-case derivation rather than an
unsupported networking fact.

**What worked:**

- It targets the mechanism behind the performance difference instead of
  treating “full duplex is faster” as an unconditional slogan.
- The purely one-way workload is load-bearing and clearly stated.
- The full-duplex completion time provides a useful reference without giving
  away whether half duplex matches or differs from it.
- The open numeric response avoids multiple-choice elimination.
- The ideal-conditions statement removes protocol overhead and acknowledgment
  debates from the intended model.
- The explanation clearly distinguishes channel rate from simultaneous
  bidirectional capacity.
- The foundation role is honest: this is a conceptual misconception check, not
  a high-rung mathematical problem.

**Minor observations, not defects:**

- Real protocols may generate acknowledgments or control traffic in the reverse
  direction, but the stem explicitly makes reverse traffic negligible and
  assumes ideal conditions.
- The arithmetic contributes almost no difficulty. That is appropriate because
  the target is recognizing when the half-duplex constraint is inactive.
- The internal candidate answer `9 s` has no particularly clear misconception
  behind it, but the delivered question uses an open numeric field, so this does
  not weaken the learner-facing item.

**Architectural evidence contributed:**

- Positively supports `approximation-breakdown` and limiting-case questions for
  conceptual notes.
- Shows that a broad comparison claim can be tested well by removing the
  condition that normally produces the difference.
- Provides a good example of easy arithmetic carrying a meaningful conceptual
  misconception check.
- Supports retaining open numeric foundation items when numerical calculation
  is not the main source of difficulty.
- Does not justify repair or a new engine rule.

### Q-016 — Open maze endpoints with an isolated destination

**Decision:** **Keep as low-weight enrichment after a targeted stem repair:
state that movement is orthogonal.**

**Live learner context:**

- Exact source: `DSA/Recursion/Rat in a maze.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: enrichment
- Catalog difficulty: easy
- Validated reasoning rung after calibration: 1
- Mastery weight: 0.35
- Result shown in the running session: correct

The low role and mastery weight accurately reflect that this is a quick
reachability check rather than evidence of backtracking implementation
mastery. Elapsed response time is not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment as enrichment: 8/10
- Learning value: 7/10
- Difficulty calibration: 9/10
- Technical clarity before repair: 5.5/10
- Overall quality before repair: 6.5/10
- Expected quality after the one-line repair: 8/10

**What worked:**

- It tests the useful limiting case that open start and destination cells do
  not guarantee reachability.
- The destination's isolation makes the result easy to verify without a long
  recursive trace.
- The open center cell prevents the grid from looking completely trivial at a
  glance.
- The open numeric response avoids option-based elimination.
- The explanation uses a local cut argument—inspect the destination's
  neighbours—rather than unnecessarily simulating the full DFS.
- The engine correctly downgraded the item to easy enrichment with mastery
  weight 0.35 instead of treating it as transfer from a coding note. This is a
  positive example of post-authoring calibration.

**Material ambiguity: the adjacency rule is missing**

The source note permits only four moves: up, down, left, and right. The
question's stem says the rat may step onto cells holding one, but never states
which cells count as reachable in one step.

The key and explanation assume four-neighbour orthogonal movement. Under a
reasonable alternative reading that permits diagonal movement, the open center
cell creates a route from the start to the destination. The answer therefore
depends on a movement rule absent from the learner-facing stem.

This is not merely extra context. It is a load-bearing topology condition and
must be explicit.

**Targeted repair:**

Add one sentence:

> It may move one cell at a time up, down, left, or right; diagonal movement is
> not allowed.

Freeze the grid, key, explanation, role, and mastery weight. Re-run the local
check and presentation review.

**Instructional-center judgment:**

Even after repair, this should remain enrichment rather than application or
transfer. The learner can solve it by noticing that the destination has no
open orthogonal neighbour; they do not need to design, implement, or trace the
backtracking algorithm.

That is acceptable because the calibrated role already says exactly that. The
question is a quick conceptual boundary check, not the main assessment for the
rat-in-a-maze note. Q-009 remains the stronger code-centered item.

**Architectural evidence contributed:**

- Supports bounded scenario repair for omitted movement or adjacency rules.
- Shows that grid and graph questions need explicit topology: orthogonal,
  diagonal, directed, weighted, wrapping, or otherwise.
- Exposes a presentation-review miss: the reviewer called the stem
  self-contained despite relying on the source's unstated movement rule.
- Positively validates the engine's downgrade path to enrichment and low
  mastery weight.
- Does not justify rejecting limiting-case reachability questions.

### Q-017 — Is the copy-before condition necessary or sufficient?

**Decision:** **Keep. This is a strong foundation question about the
specification of the interval-insertion routine.**

**Live learner context:**

- Exact source: `DSA/Greedy Algorithms/Insert interval.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: foundation
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.8
- Result shown in the running session: incorrect

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment: 9.5/10
- Learning value: 9/10
- Difficulty as delivered: 7/10
- Distractor quality: 9.5/10
- Clarity: 8.5/10
- Overall quality: 9/10

**Source-fidelity audit:**

The source divides the algorithm into three phases:

1. copy intervals ending strictly before the new interval begins;
2. merge overlapping intervals; and
3. copy the remaining intervals after the merged interval.

The proposed condition is exactly the first-phase loop guard. The claimed
outcome—an original interval remains unchanged and is not merged—can also occur
through the third phase. The classification therefore follows entirely from
the source routine.

Endpoint behavior is consistent with the implementation: an interval ending
exactly where the new interval starts does not satisfy the strict copy-before
condition and is handled by the inclusive merge phase.

**Why this reads the coding note well:**

The question does not ask for a code output on one dataset. It asks what a
specific branch condition guarantees relative to the algorithm's
postcondition. Solving it requires examining more than the branch where the
condition appears:

- the first phase proves the forward implication; and
- the final phase supplies a counterexample to the reverse implication.

This tests the relationship between implementation conditions and behavioral
specification, a useful code-reasoning skill.

**What worked:**

- The condition comes directly from a meaningful loop guard in the source.
- The outcome is clearly defined as unchanged **and unmerged**, avoiding
  confusion with an overlap that happens to preserve the same outer endpoints.
- The learner must distinguish a guarantee from a complete characterization.
- A right-side interval provides a simple, general counterexample to necessity.
- The four choices exhaust the necessary/sufficient truth table and are
  genuinely distinct.
- The explanation proves both directions separately instead of relying on an
  example alone.
- The foundation role is appropriate: the logic is formal, but the algorithmic
  facts are direct and well scaffolded.

**Minor observations, not defects:**

- The notation-heavy stem is denser than the source's prose. A small diagram
  could make left/right cases quicker to parse, but it is not required.
- The item partly assesses comfort with necessary-versus-sufficient reasoning.
  That is a useful general prerequisite for program correctness, and every
  needed interval fact remains source-grounded.
- The sentence “survives unchanged exactly when it does not overlap” should
  continue to retain the “unmerged” qualifier from the stem; without that
  qualifier, a containing interval could merge while preserving its outer
  endpoints.

**Architectural evidence contributed:**

- Strongly supports the `necessary-sufficient` move for code branch conditions.
- Shows that source code can generate specification questions by comparing a
  local guard with the full set of paths to a postcondition.
- Provides an excellent distractor structure: all four logical classifications
  are exhaustive without being stylistically artificial.
- Reinforces multi-phase coverage—the reviewer must inspect both copy-before
  and copy-after behavior before deciding necessity.
- Does not justify repair or a new engine rule.

### Q-018 — Recovering an LSB-first transmitted value

**Decision:** **Keep. This is a good application question and a legitimate
extension of the binary-conversion section.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill estimate for this note: 3.0
- Recorded observations for this note: 4
- Result shown in the running session: correct

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 9.5/10
- Construct alignment: 9/10
- Learning value: 8.5/10
- Difficulty fit for the current learner: 8.5/10
- Distractor quality: 7.5/10
- Clarity: 9/10
- Overall quality: 8.7/10

**Source-fidelity audit:**

The note's binary-to-decimal section explicitly says to:

1. begin at the rightmost, least-significant bit;
2. assign that bit position index 0;
3. multiply each bit by the corresponding power of 2; and
4. sum the weighted values.

The sensor and transmission order are not present in the note, but the stem
fully defines them. No external networking or hardware knowledge is needed.
The learner only has to preserve the note's position/significance rule while
recognizing that arrival order is the reverse of the conventional displayed
bit-string order. This satisfies the project's allowed-extension policy:
new context is acceptable when the answer follows from the stated context plus
the source.

**Why this works for the learner:**

At skill 3 with four observations, another direct conversion of a conventional
binary string would likely be too routine. This item adds one meaningful
representation change without adding much calculation. It tests whether the
learner understands positional significance rather than merely imitating the
source's right-to-left procedure.

The question is therefore more useful than “convert `1101` to decimal” while
remaining close enough to the note to serve as a fair application item.

**What worked:**

- The conflict between LSB-first arrival and an MSB-first parser is explicit.
- The learner must separate temporal order, string order, and significance
  order.
- The naive parser result is a strong distractor based on the exact mistake in
  the scenario.
- The numerical work is small, leaving the representation convention as the
  decisive idea.
- The sensor setting adds purpose without requiring sensor-domain knowledge.
- The explanation reconstructs the value by assigning powers according to
  actual bit significance.

**Minor observations, not defects:**

- The explanation calls the MSB-first assumption “unstated,” although the stem
  explicitly states that the routine gives the leftmost character the highest
  power. The real hidden assumption is that concatenating arrival order creates
  a conventionally ordered bit string. “An MSB-first parser is being applied
  to an LSB-first serialization” would be more precise.
- The naive-parser distractor is excellent; the other two distractors are less
  compelling. They are harmless here, but a future variant could give each
  wrong option a clearer representation-order mistake.
- The stem is intentionally explicit. It is an application question, not a
  deep systems question, and should not be inflated into one.

**Architectural evidence contributed:**

- Positively supports the `hidden-assumption` move for representation
  conventions.
- Shows a useful transfer pattern: preserve the source operation while changing
  the order in which its representation arrives.
- Reinforces the allowed-extension rule: a novel device context is fine when
  every new fact required for the inference is stated in the stem.
- Suggests a wording audit that distinguishes an explicitly stated component
  convention from the genuinely hidden compatibility assumption between two
  components.
- Does not justify a new engine rule or broader repair.

### Q-019 — Accumulating pure-virtual implementations down a class chain

**Decision:** **Keep as an application question. It tests a central abstraction
rule through cumulative inheritance rather than disguising an unrelated
puzzle in OOP vocabulary.**

**Live learner context:**

- Exact source: `Object Oriented Programming/Abstraction.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: application
- Catalog difficulty: medium
- Validated reasoning rung: 2
- Mastery weight: 0.8
- Result shown in the running session: incorrect

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment: 9.5/10
- Learning value: 8.5/10
- Difficulty fit with no prior learner record: 8/10
- Diagnostic value of the response format: 7/10
- Clarity: 8.5/10
- Overall quality: 8.7/10

**Source-fidelity audit:**

The note directly establishes all of the required rules:

- an abstract class cannot be instantiated directly;
- abstract methods have no implementation;
- a subclass inherits its parent's abstract methods;
- an abstract subclass may leave inherited abstract methods unimplemented; and
- a concrete subclass must provide the remaining implementations.

The source's prose is framed partly around Java while its principal example
uses C++ pure-virtual syntax. The delivered question explicitly chooses C++ and
stays within the behavior demonstrated by that example, so this mixed-language
source does not create an unsupported inference here.

**Why this is a genuine OOP question:**

The decisive operation is tracking an inherited interface obligation. Each
class supplies one missing behavior, but partial completion does not make the
class concrete. The learner must understand:

- abstractness is inherited along with unimplemented operations;
- implementing some operations is insufficient;
- a single remaining pure virtual function prevents instantiation; and
- the class becomes concrete only once the whole contract is fulfilled.

Those are directly relevant to abstract classes, contracts, inheritance, and
concrete implementations. The class terminology is not decorative.

**What worked:**

- The chain turns one source rule into a small but meaningful state-tracking
  problem.
- Each level changes the state, so none of the intermediate classes is filler.
- The final class supplies a clear boundary between partial and complete
  implementation.
- The question is self-contained even for a learner who does not remember C++
  declaration syntax.
- The explanation clearly tracks the number of unresolved pure virtual
  functions at each level.
- Medium application is a sensible default when the learner has no prior
  record for this note.

**Minor observations, not defects:**

- Asking only for the count is less diagnostic than asking which classes are
  non-instantiable. An answer of 3 does not reveal whether the learner omitted
  the base class, treated one partially implementing subclass as concrete, or
  made a simple counting error.
- The stem is long because four methods and four subclasses must be named. A
  compact table could improve scanning, but the prose remains unambiguous.
- The catalog's `limiting-case` label is not the most natural description of
  the reasoning. This is closer to tracking deferred obligations until
  completion. One example alone does not justify adding a new move.

**Architectural evidence contributed:**

- Positively demonstrates that OOP questions should make an OOP mechanism
  load-bearing; here inherited abstract obligations determine every step.
- Supports cumulative-state questions for class hierarchies, contracts, and
  interface fulfillment.
- Suggests considering semantic response formats when the identities of the
  misclassified entities are more diagnostic than their count. For this item,
  “Which classes cannot be instantiated?” could reveal the misconception more
  precisely than an integer field.
- Does not justify a mandatory rule: open numeric answers still avoid
  multiple-choice elimination and are acceptable when compactness is valued.

### Q-020 — Exact maximum pass count of the platform sweep

**Decision:** **Keep the question and key, but repair the worked explanation.
This is a strong enrichment question with an invalid intermediate bound.**

**Live learner context:**

- Exact source:
  `DSA/Greedy Algorithms/Minimum number of platforms required for a railway.md`
- Pre-existing Whetstone skill record for this note: none
- Pre-existing recorded observations for this note: none
- Catalog role: enrichment
- Catalog difficulty: medium
- Validated reasoning rung: 1
- Mastery weight: 0.35
- Result shown in the running session: incorrect

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding: 10/10
- Construct alignment: 9.5/10
- Learning value: 8.5/10
- Difficulty fit with no prior learner record: 8/10
- Answer-key correctness: 10/10
- Worked-explanation correctness: 6.5/10
- Overall quality as delivered: 7.8/10
- Expected quality after the explanation repair: about 9/10

**Source-fidelity and answer audit:**

The question reproduces the optimized source loop accurately:

- `i` starts at 1;
- `j` starts at 0;
- the loop continues while both are below \(N\); and
- every pass increments exactly one pointer.

The final maximum \(2N-2\), and therefore 14 for \(N=8\), is correct. It is
attainable by valid interleaved schedules, for example:

```text
arr = [0, 10, 20, 30, 40, 50, 60, 70]
dep = [5, 15, 25, 35, 45, 55, 65, 75]
```

The loop alternates departure and arrival decisions until `i` reaches \(N\),
executing 14 passes.

**The explanation defect:**

The worked solution states that `j`, which begins at 0, can advance at most
\(N-1\) times. That is false when `j` is the pointer that terminates the loop:
it may advance

\[
0\rightarrow1\rightarrow\cdots\rightarrow N,
\]

which is \(N\) advances.

The final bound remains correct because the two termination cases are
asymmetric:

- If `i` reaches \(N\), it has advanced \(N-1\) times, while `j` has advanced
  at most \(N-1\) times.
- If `j` reaches \(N\), it has advanced \(N\) times, but `i` can have advanced
  at most \(N-2\) times because it began at 1 and must still be below \(N\).

Either case gives at most

\[
2N-2
\]

body executions. This is the proof the explanation should use.

An even more compact proof uses the potential `i + j`: it starts at 1 and
increases by exactly 1 per pass. At termination the largest possible sum is
\(2N-1\), attained at either \((N,N-1)\) or \((N-1,N)\). Thus the number of
passes is at most \((2N-1)-1=2N-2\).

**Why this reads the coding note well:**

Unlike the earlier railway-platform instance question, this item tests the
optimized implementation itself. It asks the learner to derive why the
two-pointer sweep is linear and to handle the non-symmetric initial pointer
positions. That is directly relevant to the source's code and complexity
analysis.

The enrichment role and reduced mastery weight are good choices. The exact
constant is useful reasoning practice but is not the primary learning target
of the minimum-platform algorithm.

**What worked:**

- The code structure, rather than the railway story, is load-bearing.
- The open numeric response prevents option-based off-by-one elimination.
- The maximum is not obtained by simply adding the two array lengths.
- Achievability matters as well as an upper bound.
- The starting values `i = 1` and `j = 0` create a worthwhile boundary detail.
- The role and weight prevent this exact-count exercise from dominating the
  algorithm's central learning goals.

**Architectural evidence contributed:**

- Positively supports `conservation-accounting` for pointer-based algorithms.
- Shows that algorithm notes can yield worthwhile complexity/invariant
  questions rather than only input-output traces.
- Exposes a validation gap: a correct numeric key can mask a false lemma in the
  worked explanation.
- The current verifier demonstrates one attaining execution and random-tests
  the final bound, but does not exercise both termination branches or validate
  the prose claim about each pointer's individual maximum.
- Supports claim-level verification of quantitative explanations, especially
  off-by-one statements and case splits.

### Q-021 — Adding a payment type without touching existing code

**Decision:** **Keep the design lesson, but repair the stem and keyed wording.
As delivered, it silently assumes a runtime extension mechanism that
inheritance and polymorphism alone do not provide.**

**Live learner context:**

- Exact source: `Object Oriented Programming/What is OOPS.md`
- Current Whetstone skill estimate for this note: 0.0
- Recorded observations for this note: 2
- Catalog role: enrichment
- Catalog difficulty: medium
- Validated reasoning rung: 1
- Mastery weight: 0.35
- Result shown in the running session: correct

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Source grounding for the general design principle: 9/10
- Construct alignment: 9/10
- Learning value: 8/10
- Difficulty fit for the current learner: 8/10
- Technical completeness of the scenario: 6/10
- Distractor quality: 7.5/10
- Overall quality as delivered: 7/10
- Expected quality after a narrow stem repair: about 9/10

**What the source supports:**

The note explicitly says that:

- inheritance supports extension and reuse;
- polymorphism lets code handle objects generically;
- OOP makes it easier to extend functionality without altering existing code;
  and
- scalability means adding features with minimal disruption.

The question therefore reads the source's instructional center well. It asks
the learner to combine inheritance and runtime polymorphism to satisfy an
extensibility constraint. Unlike an OOP-themed information puzzle, the OOP
mechanisms are load-bearing.

**The missing precondition:**

“Do not edit existing source code” and “do not recompile existing code” are
different requirements.

Adding a subclass can preserve existing source code if the dispatcher already
operates on a base type and calls an overridable `process()` method. But an
already-compiled program must still obtain an instance of the new subclass.
That normally requires some existing runtime extension mechanism, such as:

- a plugin or dynamic-module loader;
- reflection or class-name-based construction;
- dependency injection from outside the dispatcher;
- a registry that supports runtime registration; or
- an already-created `Payment` object supplied to the dispatcher.

The stem says the existing types use one shared `process()` call, which
suggests polymorphism, but it does not state how a new implementation becomes
available to the unchanged binary. A subclass declaration alone does not make
the running dispatcher discover or instantiate it.

The validation metadata partially catches this. Its blind issue says:

> Option B assumes existing `process()` already dispatches polymorphically.

Despite that, the grounded reviewer calls the question self-contained and all
acceptance gates pass. The caveat is decisive under the stem's hard
no-recompilation rule and should have triggered repair.

**Secondary wording issues:**

- The keyed option says the existing `process()` “invokes” the subclass via
  polymorphic dispatch. More precisely, the unchanged dispatcher invokes
  `process()` through a base/interface reference, and dynamic dispatch selects
  the new subclass's override.
- The explanation says the full rebuild option “meets the rule,” even though
  rebuilding working code conflicts with the stated no-recompilation rule.
  The option is still non-minimal, but the rationale should not claim it
  satisfies the hard constraint.

**Recommended narrow repair:**

Either test source-level open/closed design:

> Existing source files may not be modified. The dispatcher already accepts a
> `Payment` interface value and calls `payment.process()`. What is the smallest
> source-code addition needed for a new payment behavior?

Or, if no recompilation is essential, state the deployment extension point:

> The running application already loads separately compiled payment plugins
> and passes each plugin's `Payment` object to an unchanged dispatcher that
> calls `payment.process()`.

The keyed option can then be:

> Add one new `Payment` implementation overriding `process()` and supply it
> through the existing plugin/loading path.

This preserves the intended inheritance-plus-polymorphism reasoning while
making the zero-edit, zero-recompile claim achievable.

**Why the item is still promising:**

- It directly contrasts encapsulation, inheritance, polymorphism, conditional
  dispatch, and wholesale redesign.
- The resource-minimality framing asks which mechanism is sufficient rather
  than merely asking for a definition.
- The scenario is relevant to maintainability and scalability.
- Enrichment with low mastery weight is appropriate at the learner's current
  skill estimate, especially because the note itself states only the general
  principle.

**Architectural evidence contributed:**

- Strongly supports consuming `blind_issue` in acceptance and repair routing.
- Shows that “no source changes” must not be silently upgraded to “no
  recompilation” without deployment assumptions.
- Supports a discovery/instantiation audit whenever a question claims a new
  subtype can enter an unchanged running system.
- Demonstrates a good scenario-repair candidate: the reasoning, answer family,
  and learning goal can remain fixed while one missing extension-point fact is
  added.

### Q-022 — Width-independent bits of a two's-complement representation

**Decision:** **Keep the concept and key, but repair the distractors,
source-facing derivation, and rendered mathematics.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill estimate for this note: 3.0
- Recorded observations for this note: 4
- Result shown in the running session: correct

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Mathematical correctness: 10/10
- Source grounding: 8.5/10
- Construct alignment: 8.5/10
- Learning value: 7.5/10
- Difficulty fit for the current learner: 8.5/10
- Distractor independence: 5.5/10
- Delivered presentation: 6.5/10
- Overall quality as delivered: 7.3/10
- Expected quality after targeted repairs: about 9/10

**Correctness audit:**

For every width \(n\ge5\), the \(n\)-bit representation of positive 13 ends in
`01101`; any additional bits to the left are zeros. Applying the source's
two-step procedure:

1. flip all \(n\) bits, giving low five bits `10010`; and
2. add 1, giving low five bits `10011`.

Therefore the lowest four bits are always `0011`. The full pattern, total
width, and absolute number of 1-bits vary with \(n\). The keyed statement is
correct.

The explanation's equivalent identity,

\[
2^n-13 \pmod {16}=3,
\]

is also mathematically correct for the stated widths.

**Source-fidelity audit:**

The source demonstrates two's complement by fixing an 8-bit representation,
flipping every bit, and adding 1. It does not explicitly state the theorem that
an \(n\)-bit representation of \(-13\) is the unsigned pattern \(2^n-13\), nor
does it discuss modular invariants.

That does not make the question ungrounded: the answer can be derived directly
from the source procedure after adding leading zeros to reach any \(n\ge5\).
For a skill-3 learner, asking what survives a change in bit width is a useful
stretch beyond repeating the 8-bit example.

The worked explanation should lead with the flip-and-add derivation because it
is both simpler and more directly connected to the note. The modular identity
can follow as an optional compact confirmation.

**Distractor problem:**

All three wrong choices describe incidental properties of the single 8-bit
example:

- the entire pattern stays `11110011`;
- the representation always has exactly 8 bits; and
- the pattern always has exactly six 1-bits.

Once the stem says \(n\) varies, all three can be rejected by the same generic
observation without calculating any two's complement. The correct choice is
also the only option phrased as a local bit invariant rather than a fixed
8-bit property. This weakens the intended modal-filter reasoning.

**Better distractor direction:**

Make every option a proposed width-independent low-bit pattern, for example:

- lowest four bits `0011`;
- lowest four bits `0010` — flips but forgets the added 1;
- lowest four bits `1101` — leaves the original bits unchanged;
- lowest four bits `1100` — applies an incorrect decrement.

Then the learner must actually apply the source procedure, and every choice has
the same logical and stylistic form. Another valid design is an open response:
ask directly for the invariant low five-bit suffix.

**Presentation defect:**

The screenshot visibly contains unrendered notation in the explanation,
including literal fragments resembling:

- `(2^{n}-13)\bmod 16`; and
- raw caret/braces in the leading summary.

The underlying mathematical content is understandable, but this is a real
learner-facing quality defect. A presentation gate should render the exact
stored Markdown/TeX through the same path as the application and detect
leftover control sequences or braces.

**What worked:**

- Bit width is a genuinely important hidden assumption in complement
  representations.
- The question moves from one fixed example to a family of representations.
- The key is exact and the \(n\ge5\) condition is sufficient for representing
  13 in signed two's complement.
- The local-suffix invariant is an elegant consequence of the original
  flip-and-add algorithm.
- Stretch is reasonable for the learner's current skill rather than being an
  arbitrary difficulty escalation.

**Architectural evidence contributed:**

- Positively supports hidden-assumption and modal-filter questions about
  representation width.
- Strengthens the case for distractor-independence auditing: surface variety is
  not enough when all wrong choices fail under one generic observation.
- Supports preferring a source-native derivation before introducing a more
  advanced equivalent theorem.
- Adds concrete evidence that presentation validation must exercise the actual
  UI math-rendering path.

### Q-023 — Tight bounds after a right shift

**Decision:** **Keep. Minor explanation wording polish only.**

**Live learner context:**

- Exact source: `DSA/Bit manipulation/Bit manipulation.md`
- Current Whetstone skill estimate for this note: 3.0
- Recorded observations for this note: 4
- Result shown in the running session: correct

The result does not determine the quality judgment. Elapsed response time is
not inspected or used.

**Approximate assessment:**

- Mathematical correctness: 10/10
- Source grounding: 9/10
- Construct alignment: 8.5/10
- Learning value: 8/10
- Difficulty fit for the current learner: 8/10
- Distractor independence: 9/10
- Delivered presentation: 9.5/10
- Overall quality as delivered: 8.6/10

**Correctness audit:**

For a nonnegative integer,

\[
n \mathbin{>>} 3=\left\lfloor\frac n8\right\rfloor.
\]

This function is non-decreasing, so its minimum and maximum on the integer
interval \(100\le n\le130\) occur at the endpoints:

\[
\left\lfloor\frac{100}{8}\right\rfloor=12,\qquad
\left\lfloor\frac{130}{8}\right\rfloor=16.
\]

Both endpoints are attained, so the keyed interval \([12,16]\) is tight. The
worked solution and answer key are correct.

**Source-fidelity audit:**

The source states that a right shift discards bits and gives
`13 >> 1 = 6`; it later repeatedly right-shifts while counting set bits. The
source does not explicitly give the general floor-division identity, but the
stem supplies it directly. The learner therefore needs only the stated
extension plus the note's shift semantics.

This is an appropriate kind of source extension: the new fact is declared,
not smuggled into the answer, and it enables a meaningful consequence of the
operation. The question moves beyond executing one fixed shift while remaining
close to the instructional center of the note.

**Why the question works:**

- It tests the semantic effect of a shift on an uncertain input, rather than
  merely asking for one mechanical binary conversion.
- “Tightest” matters: the learner must find attainable extrema, not merely any
  safe enclosure.
- The range crosses several quotient buckets, so this is not disguised
  single-value arithmetic.
- It is self-contained without over-defining elementary notions.
- For a skill-3 learner, propagating an interval through a discrete,
  non-decreasing operation is a reasonable stretch.

Unlike the weak railway-platform instance in Q-006, this instance is not
trying to stand in for an implementation exercise. It deliberately tests a
general semantic consequence of the operator, and the selected numbers expose
both endpoint-rounding errors.

**Distractor audit:**

The distractors are unusually clean:

- \([13,16]\) rounds the lower endpoint upward;
- \([12,17]\) rounds the upper endpoint upward; and
- \([11,16]\) is a safe but non-tight lower extension.

These are distinct misconceptions, all use the same answer form, and none is
eliminated merely by noticing a superficial wording difference. This is a
positive counterexample to Q-022's distractor collapse.

**Minor wording issue:**

The decisive-insight summary says:

> “the ceiling comes from \(\lfloor130/8\rfloor=16\), not 17.”

“Ceiling” is awkward here because the operation explicitly uses the floor
function and the word can be mistaken for mathematical ceiling. Replace it
with “upper endpoint” or “maximum”:

> Right shift floors rather than rounds, so propagate both endpoints through
> \(\lfloor n/8\rfloor\); the upper endpoint is 16, not 17.

This does not require regenerating or rejecting the question.

**Architectural evidence contributed:**

- Positive evidence for self-contained source extensions: a supplied general
  rule can support useful transfer without claiming that rule came from the
  note.
- Positive evidence for misconception-grid distractors: lower-rounding,
  upper-rounding, and non-tight-bound errors remain independent.
- Supports retaining bounds-propagation as a useful reasoning move for
  discrete operators.
- No new engine change is justified by this example.

## Evidence log

| Date | Example | Decision | Architecture impact |
|---|---|---|---|
| 23 Jul 2026 | Q-001: encapsulated unit but no signal | Keep + targeted repair | Supports optionless solve, distractor audit, and scenario repair |
| 23 Jul 2026 | Q-002: front-only decapsulation | Keep | Positive example; no new change |
| 23 Jul 2026 | Q-003: vibration-resistant connector | Keep and observe | Clarifies allowed source extensions; no new engine change |
| 23 Jul 2026 | Q-004: `n & (n - 1)` pass count | Keep + repair one option | Skill 3 validates Stretch role; supports option-scope audit |
| 23 Jul 2026 | Q-005: hub versus switch aggregate throughput | Keep | Strong first-observation application; validates invariant/instrument reasoning and misconception-grid distractors |
| 23 Jul 2026 | Q-006: one railway-platform instance | Reclassify as low-weight foundation | Exposes instructional-center mismatch for coding notes; engine-change candidate |
| 23 Jul 2026 | Q-007: repeated power-set rebuilding | Keep | Positive coding-note transfer; supports multi-label instructional centers and complexity tasks |
| 23 Jul 2026 | Q-008: clearing a bit in a minimal pair | Keep as application | Valid instance question; refines alignment to seed/section-level instructional intent |
| 23 Jul 2026 | Q-009: rat-in-a-maze without restoration | Keep | Strong debugging instance; validates code mutation and counterexample question families |
| 23 Jul 2026 | Q-010: sequential interval insertion | Keep and observe distractors | Strong composition/invariant task; wrong choices cluster around stale intermediate states |
| 23 Jul 2026 | Q-011: coaxial shield removal | Immediate correction | Key ignores outward leakage and real transmission-line roles; supports complete lost-function audit |
| 23 Jul 2026 | Q-012: XOR swap with aliased indices | Keep | Excellent hidden-precondition counterexample; validates aliasing and regime-shift mutations |
| 23 Jul 2026 | Q-013: virtual dispatch in abstract constructor | Immediate grounding correction | Correct C++ but decisive dispatch rule is absent; requires inference-level provenance |
| 23 Jul 2026 | Q-014: subset size over a bitmask interval | Keep as application | Good structured bounds task; explanation should foreground fixed/varying bits |
| 23 Jul 2026 | Q-015: one-way half-duplex transfer | Keep as foundation | Strong limiting case; tests when the usual duplex performance difference disappears |
| 23 Jul 2026 | Q-016: isolated maze destination | Targeted stem repair; retain as enrichment | Missing orthogonal-movement rule; positive role/mastery downgrade |
| 23 Jul 2026 | Q-017: interval copy-before condition | Keep as foundation | Strong necessary/sufficient specification task with exhaustive logical distractors |
| 23 Jul 2026 | Q-018: LSB-first sensor serialization | Keep as application | Positive hidden-assumption transfer; novel context is fully self-contained |
| 23 Jul 2026 | Q-019: inherited pure-virtual obligations | Keep as application | Genuine OOP mechanism; identity-based response could diagnose errors better than a count |
| 23 Jul 2026 | Q-020: exact platform-sweep pass bound | Keep; repair explanation | Correct key and strong code alignment, but proof falsely caps `j` at \(N-1\) advances |
| 23 Jul 2026 | Q-021: payment extension under no recompilation | Repair stem and keyed wording | Good open/closed lesson, but subclass discovery requires an unstated runtime extension point |
| 23 Jul 2026 | Q-022: variable-width two's complement | Keep; repair distractors and rendering | Correct invariant, but three width-specific distractors collapse under one shortcut and TeX is visibly broken |
| 23 Jul 2026 | Q-023: right-shift interval bounds | Keep; wording polish only | Strong self-contained transfer and independent endpoint-error distractors; no new engine change |

## Update procedure for future questions

For each new question:

1. Inspect the running app and live learner state for the exact source note.
2. Read the relevant notes, not merely the displayed source label.
3. Judge the question according to the learner's skill and its intended level.
4. Do not inspect or use elapsed response time.
5. Identify the note's instructional center and compare it with the capability
   demonstrated by answering the question.
6. Record positive qualities before weaknesses.
7. Choose a decision from the five-level review policy.
8. Add architecture evidence only when the example reveals a systematic issue.
9. Strengthen an engine recommendation only after repeated supporting examples,
   unless the issue is an objective correctness or plumbing defect.
10. Record positive counterexamples when the existing system works well; they
   prevent over-correction.
