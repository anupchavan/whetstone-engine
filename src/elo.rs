//! Item difficulty as a measured rating, not an authored label.
//!
//! Every blind solve is an Elo match between the item and a reference player.
//! The item "wins" when the player answers incorrectly. Ratings start from the
//! rung's structural prior with a wide deviation and are provisional until
//! enough independent observations (model probes now, learner attempts later)
//! shrink the deviation. Anchor values are version-1 guesses on this scale:
//!
//!   ~800 school-easy · ~1200 school-hard · ~1500 JEE Mains
//!   ~1800 JEE Advanced · ~2100 top-Advanced/HMMT · ~2400 olympiad-entry
//!
//! Re-anchoring against real learner data replaces these constants and bumps
//! `anchor_version`; stored ratings are re-mapped, never trusted across
//! versions.

use crate::model::{EloState, ProbeRecord};

pub const ANCHOR_VERSION: u8 = 1;

/// Reference players available to the pipeline. Effort tiers give distinct
/// rungs on one model family; the ratings are anchor guesses like the bands.
pub struct ReferencePlayer {
    pub name: &'static str,
    pub effort: &'static str,
    pub rating: f64,
}

pub const PROBE_STANDARD: ReferencePlayer = ReferencePlayer {
    name: "blind:sonnet:medium",
    effort: "medium",
    rating: 1900.0,
};

/// Escalation probe for items that defeat the standard probe while holding an
/// oracle-proved key — the hard tail worth a second, stronger observation.
pub const PROBE_STRONG: ReferencePlayer = ReferencePlayer {
    name: "blind:sonnet:high",
    effort: "high",
    rating: 2150.0,
};

pub fn new_state(prior_rating: f64, prior_deviation: f64) -> EloState {
    EloState {
        rating: prior_rating,
        deviation: prior_deviation,
        anchor_version: ANCHOR_VERSION,
        provisional: true,
        probes: Vec::new(),
    }
}

fn expected_item_win(item_rating: f64, player_rating: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((player_rating - item_rating) / 400.0))
}

/// Record one match. K scales with remaining uncertainty so early probes move
/// the rating and later ones refine it; deviation shrinks toward a floor that
/// keeps ratings honest about how little evidence a handful of probes is.
pub fn record_probe(state: &mut EloState, player: &ReferencePlayer, player_correct: bool, confidence: f64) {
    let outcome = if player_correct { 0.0 } else { 1.0 };
    let expected = expected_item_win(state.rating, player.rating);
    let k = (state.deviation / 4.0).clamp(16.0, 176.0);
    state.rating += k * (outcome - expected);
    state.deviation = (state.deviation * 0.85).max(140.0);
    state.provisional = true; // model probes alone never de-provision an item
    state.probes.push(ProbeRecord {
        player: player.name.to_owned(),
        player_rating: player.rating,
        player_correct,
        confidence,
    });
}

/// Human-facing band label for a rating; bands mirror the anchor scale.
pub fn band(rating: f64) -> &'static str {
    match rating {
        r if r < 1000.0 => "school-easy",
        r if r < 1350.0 => "school-hard",
        r if r < 1650.0 => "JEE-Mains",
        r if r < 2000.0 => "JEE-Advanced",
        r if r < 2300.0 => "top-Advanced/HMMT",
        _ => "olympiad-entry",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defeating_a_probe_raises_the_rating() {
        let mut state = new_state(1500.0, 350.0);
        record_probe(&mut state, &PROBE_STANDARD, false, 0.7);
        assert!(state.rating > 1500.0);
        assert!(state.deviation < 350.0);
        assert_eq!(state.probes.len(), 1);
    }

    #[test]
    fn losing_to_a_probe_lowers_the_rating() {
        let mut state = new_state(2200.0, 350.0);
        record_probe(&mut state, &PROBE_STANDARD, true, 0.95);
        assert!(state.rating < 2200.0);
    }

    #[test]
    fn a_solved_rung_three_prior_moves_down_substantially() {
        let spec = crate::moves::rung(3);
        let mut state = new_state(spec.prior_rating, spec.prior_deviation);
        record_probe(&mut state, &PROBE_STANDARD, true, 0.95);
        assert!(state.rating <= 1820.0, "rating only moved to {}", state.rating);
    }

    #[test]
    fn easy_item_solved_confidently_barely_moves() {
        // An 1100-rated item solved by a 1900-rated player is expected; the
        // update must be small so priors are not erased by expected outcomes.
        let mut state = new_state(1100.0, 350.0);
        record_probe(&mut state, &PROBE_STANDARD, true, 0.99);
        assert!((state.rating - 1100.0).abs() < 15.0);
    }

    #[test]
    fn deviation_never_collapses_below_floor() {
        let mut state = new_state(1500.0, 350.0);
        for _ in 0..40 {
            record_probe(&mut state, &PROBE_STANDARD, true, 0.9);
        }
        assert!(state.deviation >= 140.0);
        assert!(state.provisional);
    }

    #[test]
    fn bands_cover_the_scale_monotonically() {
        assert_eq!(band(900.0), "school-easy");
        assert_eq!(band(1500.0), "JEE-Mains");
        assert_eq!(band(1900.0), "JEE-Advanced");
        assert_eq!(band(2500.0), "olympiad-entry");
    }
}
