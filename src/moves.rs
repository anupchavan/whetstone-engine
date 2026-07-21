//! The analytical-moves engine.
//!
//! A strong question is not a fact wrapped in four options; it is a *move* — a
//! reusable, domain-general reasoning maneuver instantiated on source material.
//! Novelty comes from `seed × move-composition × cue-stripping`, not from
//! asking a model to "be novel". Difficulty comes from composition depth and
//! cue concealment (architecture §2.2: cue visibility is a difficulty
//! dimension), and is then *measured* by probes and learners, never trusted
//! from the author's label.
//!
//! Ported from the Obsidian adaptive-practice plugin's catalog, plus the
//! composition operators harvested from the IMO 2011 "windmill" analysis:
//! its solution = lift(invariant-under-change) ∘ cycle ∘ arm(balanced start).

use crate::model::{MoveAssignment, SeedProblem};

pub struct AnalyticalMove {
    pub key: &'static str,
    pub name: &'static str,
    /// What in the material makes this move available.
    pub trigger: &'static str,
    /// The general shape of a question built on the move — no domain baked in.
    pub shape: &'static str,
    /// Whether instantiations usually yield a machine-checkable key.
    pub usually_computable: bool,
}

pub const MOVES: &[AnalyticalMove] = &[
    AnalyticalMove {
        key: "approximation-breakdown",
        name: "Approximation breakdown",
        trigger: "the material teaches a model, formula, or rule that holds under stated (often unstated) assumptions",
        shape: "Put the learner in the regime where a reflexively-applied idealization stops holding, and ask what actually changes and why.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "false-symmetry",
        name: "False symmetry",
        trigger: "two cases, views, or methods look interchangeable on the surface",
        shape: "Present the apparent equivalence, invoke a principle that seems to confirm it, and force the level at which the two genuinely diverge.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "minimal-pair",
        name: "Minimal pair",
        trigger: "two setups differing in a single feature lead to different outcomes",
        shape: "Present both cases side by side, identical except for one feature, and ask why the outcomes diverge — the lone difference isolates the concept doing the real work.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "modal-filter",
        name: "Necessary vs. possible",
        trigger: "several conclusions are plausible but only some are forced by the premises",
        shape: "Ask what can be concluded with certainty (or what MUST vs. merely CAN happen), so plausible-but-unforced options fail.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "limiting-case",
        name: "Limiting case",
        trigger: "a quantity, count, or parameter can be pushed toward zero, one, infinity, or a boundary",
        shape: "Drive a parameter to an extreme and ask for the resulting behavior — or use an extreme to eliminate answers that break there.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "counterexample-hunt",
        name: "Counterexample hunt",
        trigger: "the material states a general claim, rule, or invariant",
        shape: "Ask whether the claim always holds; if not, require the minimal case that breaks it or the exact range of inputs where it fails — never a hand-wave that it 'sometimes' fails.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "consistency-check",
        name: "Consistency / dimensional check",
        trigger: "answers carry units, types, sizes, or structural shape that must agree",
        shape: "Offer candidates that are wrong on units, type, dimensionality, or shape, answerable by a consistency check before any calculation.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "invariant-under-change",
        name: "Invariant under transformation",
        trigger: "the setup can be re-framed, reordered, relabeled, or viewed from another reference",
        shape: "Change the frame/order/representation and ask what is preserved and what is not — separating the essential from the incidental.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "faithful-translation",
        name: "Faithful translation",
        trigger: "the same content exists in two representations (source and target language, formula and code, spec and implementation, notation and meaning)",
        shape: "Ask for the faithful counterpart in the other representation — or plant one subtle infidelity (wrong scale, dropped sign, reordered effect) among near-miss translations and ask which preserves the meaning.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "resource-minimality",
        name: "Resource minimality",
        trigger: "a task can be done with more or fewer units of a bounded resource (steps, operations, memory, queries, assumptions)",
        shape: "Ask for the minimum of the resource that still works, or whether a proposed floor is achievable — with distractors that are correct-but-wasteful or infeasibly tight.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "capacity-limit",
        name: "Capacity limit",
        trigger: "a representation, container, or channel has a fixed budget (bits, digits, range, precision, slots)",
        shape: "Ask what exactly fits, what happens one step past the limit, or which workaround the system needs once the value exceeds the field.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "edge-case",
        name: "Boundary / edge case",
        trigger: "a procedure or rule has a general path plus seams (empty, single, duplicate, maximum, tie, degenerate input)",
        shape: "Aim the question at the seam where the general rule needs special handling, and ask what happens or what breaks there.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "necessary-sufficient",
        name: "Necessary vs. sufficient",
        trigger: "a condition, feature, or step is involved in producing an outcome",
        shape: "Ask whether the condition is necessary, sufficient, both, or neither — with distractors for each of the other three verdicts.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "causal-direction",
        name: "Causal direction / confound",
        trigger: "two factors co-occur or one appears to drive another",
        shape: "Ask which way the causation runs, or what third factor explains both — distinguishing correlation from mechanism.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "ordering-dependence",
        name: "Order / sequencing dependence",
        trigger: "multiple operations, steps, or events could occur in different orders",
        shape: "Ask whether the outcome depends on order — whether the steps commute — and where a reordering silently changes the result.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "constrained-tradeoff",
        name: "Constrained trade-off",
        trigger: "improving one property costs another, and the material implies a choice",
        shape: "State a concrete constraint and ask which option is right GIVEN it — never which is 'better' in the abstract.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "double-edge",
        name: "Double edge",
        trigger: "a single design change, intervention, or parameter shift plausibly helps through one mechanism and hurts through another",
        shape: "Ask for both edges of one change — the mechanism by which it improves things AND the mechanism by which the same change degrades them — or for the condition that decides which edge dominates.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "hidden-assumption",
        name: "Hidden assumption",
        trigger: "an argument, proof, or procedure quietly relies on an unstated premise",
        shape: "Present reasoning that works only if an unstated premise holds; ask the learner to name the premise or the input that voids it.",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "symptom-to-cause",
        name: "Symptom to cause",
        trigger: "the material describes mechanisms whose failure produces observable effects",
        shape: "Give an observed anomaly or failure and ask which mechanism produces exactly that symptom (and not the near-miss ones).",
        usually_computable: false,
    },
    AnalyticalMove {
        key: "flawed-argument",
        name: "Flawed argument",
        trigger: "the material supports a worked solution, derivation, proof, or chain of reasoning",
        shape: "Present a plausible worked attempt containing one specific wrong step, and ask the learner to locate or characterize the flaw — not merely notice the conclusion is wrong.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "instrument-vs-truth",
        name: "Instrument vs reality",
        trigger: "a quantity is known only through a recording, reading, or observation process that distorts it systematically (delay, relative reference, sampling, bias)",
        shape: "Ask what the instrument will record given the reality, or recover the reality from the recording — with distractors that quietly equate the two.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "bounds-propagation",
        name: "Tight bounds",
        trigger: "inputs are known only within ranges, or a constraint caps a rate, size, or duration",
        shape: "Ask for the tightest range or extreme value of the outcome consistent with the data — the strongest claim the information licenses; distractors are falsely precise points or looser-than-needed bounds.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "conservation-accounting",
        name: "Conservation / accounting",
        trigger: "some quantity is conserved, bounded, or must balance across a process",
        shape: "Use the conserved/balancing quantity to constrain the answer, so options that violate the balance are eliminable by accounting.",
        usually_computable: true,
    },
    AnalyticalMove {
        key: "regime-shift",
        name: "Scale / regime shift",
        trigger: "behavior changes qualitatively across scales or regimes (small vs. large, linear vs. nonlinear, sparse vs. dense)",
        shape: "Ask how the behavior or right choice changes when the scale or regime crosses the threshold where the character flips.",
        usually_computable: true,
    },
];

/// Setup-side "what-if-not" mutations (Brown & Walter's attribute negation;
/// the same mechanism as Evol-Instruct's in-depth evolving). Solver-side
/// MOVES shape the reasoning demanded; MUTATIONS reshape the problem WORLD by
/// negating an assumption the standard treatment holds silently. This is the
/// generator behind "a lens made of two materials", "a rod instead of a point
/// object", "the mirror itself moves".
pub struct SetupMutation {
    pub key: &'static str,
    pub instruction: &'static str,
}

pub const MUTATIONS: &[SetupMutation] = &[
    SetupMutation {
        key: "heterogenize",
        instruction: "Make non-uniform a property the standard setup silently holds uniform (material, density, spacing, rate, composition): split it into two or more regions/regimes and ask for the observable consequence — the answer must expose WHY the standard result depended on uniformity.",
    },
    SetupMutation {
        key: "extend-object",
        instruction: "Replace the pointlike/instantaneous subject of the standard law with an extended or structured one (a rod, a distribution, an interval, a population), so the law must be applied per-part and the question extracts a geometric/aggregate property of the result.",
    },
    SetupMutation {
        key: "set-in-motion",
        instruction: "Let something the standard treatment holds fixed change in time (the object, the apparatus, a boundary, a rate): the answer requires combining the topic's own transformation law with rate/vector reasoning.",
    },
    SetupMutation {
        key: "duplicate-couple",
        instruction: "Compose two copies (or two variants) of the standard device/process in series, parallel, or contact, and ask for the behavior of the composite — the wrong-but-tempting route treats the composite as a single standard instance.",
    },
    SetupMutation {
        key: "partial-obstruct",
        instruction: "Remove, block, or damage part of the standard apparatus (half the aperture, one term, one pathway, one member) and ask what survives and what degrades — probing which parts of the mechanism carry which parts of the output.",
    },
    SetupMutation {
        key: "clamp-agent",
        instruction: "Introduce an ideal external agent that holds constant a quantity the standard process would let vary (current, temperature, length, price, concentration), and move the system quasistatically. Ask for the complete accounting: which reservoir supplies what, which absorbs what, and the ratio/sign the clamp forces — the classic trap is the naive count that forgets the clamp must pay.",
    },
    SetupMutation {
        key: "misalign",
        instruction: "Break an alignment or symmetry the standard treatment assumes (on-axis, perpendicular, synchronized, balanced) by a controlled amount, and ask for the new relationship.",
    },
];

#[allow(dead_code)] // prompt-side catalog lookup, used by render/tests
pub fn find_mutation(key: &str) -> Option<&'static SetupMutation> {
    MUTATIONS.iter().find(|m| m.key == key)
}

pub struct CompositionOperator {
    pub key: &'static str,
    /// How the operator transforms the base move(s) when instantiating.
    pub instruction: &'static str,
}

/// Operators that mint new maneuvers from the base catalog. Harvested from the
/// windmill decomposition; documented for the author as transformation rules.
pub const OPERATORS: &[CompositionOperator] = &[
    CompositionOperator {
        key: "lift",
        instruction: "Lift the move from a static object to a PROCESS unfolding over time or steps: apply it to what the process preserves, consumes, or accumulates, not to any single state.",
    },
    CompositionOperator {
        key: "cycle",
        instruction: "Run the setup through one full period of an underlying symmetry (a half-turn, a round trip, a complete cycle) and compare the start and end states; the forced relabeling or mismatch carries the question.",
    },
    CompositionOperator {
        key: "arm",
        instruction: "Choose the initial condition or configuration that makes the move's quantity extremal or balanced, so the move alone forces the conclusion; ask which starting choice works and why.",
    },
    CompositionOperator {
        key: "dualize",
        instruction: "Swap what is held fixed and what varies (the dual question): if the move normally fixes the setup and asks for the outcome, fix the outcome and ask which setups can produce it.",
    },
];

/// One difficulty rung of the ladder. Elo priors are ANCHOR GUESSES
/// (anchor_version 1); probes and learner data correct them.
pub struct Rung {
    pub rung: u8,
    pub label: &'static str,
    pub moves_per_item: usize,
    pub use_operator: bool,
    pub cue_visibility: &'static str,
    pub prior_rating: f64,
    pub prior_deviation: f64,
}

pub const LADDER: &[Rung] = &[
    Rung { rung: 1, label: "foundation",  moves_per_item: 1, use_operator: false, cue_visibility: "high",   prior_rating: 1100.0, prior_deviation: 350.0 },
    Rung { rung: 2, label: "application", moves_per_item: 1, use_operator: false, cue_visibility: "medium", prior_rating: 1500.0, prior_deviation: 350.0 },
    Rung { rung: 3, label: "stretch",     moves_per_item: 2, use_operator: false, cue_visibility: "low",    prior_rating: 1900.0, prior_deviation: 350.0 },
    Rung { rung: 4, label: "frontier",    moves_per_item: 2, use_operator: true,  cue_visibility: "low",    prior_rating: 2200.0, prior_deviation: 350.0 },
];

/// The Settings "Editorial depth" tiers steer the request's rung mix.
/// Percentages per rung 1..4; each row sums to 100.
pub fn tier_shares(quality_tier: &str) -> [usize; 4] {
    match quality_tier {
        "scholar" => [45, 35, 15, 5],
        "olympiad_studio" => [10, 30, 30, 30],
        _ => [25, 35, 25, 15], // deep_work and anything unknown
    }
}

/// Per-topic adaptivity: a learner's demonstrated skill on the SOURCE
/// (0.0 weak … 1.0 strong, 0.5 neutral/unknown) shifts the tier's rung
/// shares — someone already good at a topic starts at the difficulty they
/// actually need, someone struggling gets more footing. At most 20 share
/// points move, and every rung keeps at least 5 so no band disappears.
pub fn learner_shares(mut shares: [usize; 4], learner_level: f64) -> [usize; 4] {
    let shift = ((learner_level.clamp(0.0, 1.0) - 0.5) * 40.0).round() as i64;
    let (from, to): ([usize; 2], [usize; 2]) = if shift >= 0 {
        ([0, 1], [2, 3])
    } else {
        ([3, 2], [1, 0])
    };
    let mut amount = shift.unsigned_abs() as usize;
    for (&f, &t) in from.iter().zip(to.iter()) {
        let take = amount.min(shares[f].saturating_sub(5));
        shares[f] -= take;
        shares[t] += take;
        amount -= take;
    }
    shares
}

pub fn rung(number: u8) -> &'static Rung {
    LADDER
        .iter()
        .find(|r| r.rung == number)
        .unwrap_or(&LADDER[0])
}

pub fn find_move(key: &str) -> Option<&'static AnalyticalMove> {
    MOVES.iter().find(|m| m.key == key)
}

/// Deterministic, seedless (no RNG — replay-stable) plan: distribute `count`
/// slots across rungs by the tier's shares, then walk seeds and moves in a
/// per-source rotation so two sources with the same count still get
/// different pairings.
pub fn plan_assignments(
    count: usize,
    seeds: &[SeedProblem],
    source_hash: &str,
    quality_tier: &str,
    learner_level: f64,
) -> Vec<MoveAssignment> {
    let shares = learner_shares(tier_shares(quality_tier), learner_level);
    let offset = source_hash
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    let mut slots_per_rung: Vec<(u8, usize)> = LADDER
        .iter()
        .zip(shares)
        .map(|(r, share)| (r.rung, count * share / 100))
        .collect();
    let assigned: usize = slots_per_rung.iter().map(|(_, n)| *n).sum();
    // Largest-remainder allocation: leftover slots go to the rungs the tier
    // weights most (ties rotate per source). Dumping remainders on rung 1
    // flattened every tier to "easy" whenever a source's quota was small.
    let mut leftover_order: Vec<(usize, usize)> = shares
        .iter()
        .enumerate()
        .map(|(index, share)| (index, (count * share) % 100))
        .collect();
    leftover_order.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| ((a.0 + offset) % 4).cmp(&((b.0 + offset) % 4)))
    });
    for slot in 0..count.saturating_sub(assigned) {
        slots_per_rung[leftover_order[slot % leftover_order.len()].0].1 += 1;
    }
    let mut assignments = Vec::with_capacity(count);
    let mut slot_index = 0usize;
    for (rung_number, slots) in slots_per_rung {
        let spec = rung(rung_number);
        for _ in 0..slots {
            let seed = &seeds[(offset + slot_index) % seeds.len().max(1)];
            let mut move_keys = Vec::with_capacity(spec.moves_per_item);
            for move_number in 0..spec.moves_per_item {
                let index = (offset + slot_index * 7 + move_number * 13) % MOVES.len();
                let key = MOVES[index].key.to_owned();
                if !move_keys.contains(&key) {
                    move_keys.push(key);
                } else {
                    move_keys.push(MOVES[(index + 1) % MOVES.len()].key.to_owned());
                }
            }
            let operators = if spec.use_operator {
                vec![OPERATORS[(offset + slot_index) % OPERATORS.len()].key.to_owned()]
            } else {
                Vec::new()
            };
            // Setup mutations and prerequisite bridges are the creative tier:
            // deep_work mutates its upper rungs; olympiad_studio mutates and
            // bridges. Scholar stays with the standard setups.
            let mutations = if rung_number >= 3 && quality_tier != "scholar" {
                vec![MUTATIONS[(offset + slot_index * 3) % MUTATIONS.len()].key.to_owned()]
            } else {
                Vec::new()
            };
            let bridge = rung_number == 4 && quality_tier == "olympiad_studio";
            assignments.push(MoveAssignment {
                seed_id: seed.seed_id.clone(),
                move_keys,
                operators,
                cue_visibility: spec.cue_visibility.to_owned(),
                rung: rung_number,
                mutations,
                bridge,
            });
            slot_index += 1;
        }
    }
    assignments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeds(n: usize) -> Vec<SeedProblem> {
        (0..n)
            .map(|i| SeedProblem {
                seed_id: format!("s{i}"),
                kind: "worked_example".into(),
                statement: "statement".into(),
                givens: "givens".into(),
                known_answer: "42".into(),
                locator: "p. 1".into(),
            })
            .collect()
    }

    #[test]
    fn tier_shares_sum_to_one_hundred() {
        for tier in ["scholar", "deep_work", "olympiad_studio", "unknown"] {
            assert_eq!(tier_shares(tier).iter().sum::<usize>(), 100, "{tier}");
        }
    }

    #[test]
    fn plan_covers_count_and_all_rungs() {
        let plan = plan_assignments(15, &seeds(9), "abc123", "deep_work", 0.5);
        assert_eq!(plan.len(), 15);
        for rung_number in 1..=4u8 {
            assert!(plan.iter().any(|a| a.rung == rung_number));
        }
        assert!(plan.iter().all(|a| !a.move_keys.is_empty()));
        assert!(plan.iter().filter(|a| a.rung == 4).all(|a| a.operators.len() == 1));
        assert!(plan.iter().all(|a| find_move(&a.move_keys[0]).is_some()));
    }

    #[test]
    fn single_slot_plans_respect_the_tier() {
        // With many notes each source often gets a one-question quota; the
        // single slot must land where the tier is weighted, not always rung 1.
        let plan = plan_assignments(1, &seeds(3), "hash", "scholar", 0.5);
        assert_eq!(plan[0].rung, 1);
        let plan = plan_assignments(1, &seeds(3), "hash", "olympiad_studio", 0.5);
        assert!(plan[0].rung >= 2, "olympiad single slot fell to rung 1");
        let plans: Vec<u8> = ["h1", "h2", "h3", "h4"]
            .iter()
            .map(|h| plan_assignments(1, &seeds(3), h, "olympiad_studio", 0.5)[0].rung)
            .collect();
        assert!(plans.iter().any(|r| *r >= 3), "no source got a high rung: {plans:?}");
    }

    #[test]
    fn plan_is_deterministic_and_source_sensitive() {
        let a = plan_assignments(10, &seeds(5), "hash-one", "deep_work", 0.5);
        let b = plan_assignments(10, &seeds(5), "hash-one", "deep_work", 0.5);
        let c = plan_assignments(10, &seeds(5), "hash-two", "deep_work", 0.5);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert_ne!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&c).unwrap()
        );
    }

    #[test]
    fn two_move_rungs_do_not_repeat_a_move() {
        let plan = plan_assignments(40, &seeds(7), "xyz", "olympiad_studio", 0.5);
        for assignment in plan.iter().filter(|a| a.move_keys.len() == 2) {
            assert_ne!(assignment.move_keys[0], assignment.move_keys[1]);
        }
    }
}
