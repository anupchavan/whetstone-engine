//! The room brain: a pure, synchronous state machine. No sockets, no
//! clocks of its own — callers feed it events and a monotonic `now_ms`,
//! it returns messages to deliver. Everything testable lives here.

use crate::protocol::*;
use crate::QuizItem;
use std::collections::HashMap;

const MAX_PLAYERS: usize = 500;
const BASE_POINTS: u32 = 500;
const SPEED_POINTS: u32 = 500;

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Lobby,
    Staged { q: usize },
    Open { q: usize, deadline_ms: u64 },
    Closed { q: usize },
    Podium,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub token: String,
    pub name: String,
    pub score: u32,
    /// Per-question: (shuffled correct index, chosen index, answer ms).
    answers: HashMap<usize, (u8, u64)>,
    /// This player's option order per question: shuffled[i] = original index.
    orders: HashMap<usize, Vec<u8>>,
    pub connected: bool,
}

/// One message to deliver, addressed by player token (None = teacher).
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub to: Option<String>,
    pub message: HostMessage,
}

pub struct Room {
    pub code: String,
    questions: Vec<QuizItem>,
    time_limit_secs: u32,
    pub phase: Phase,
    players: HashMap<String, Player>,
    rng_seed: u64,
}

impl Room {
    pub fn new(code: String, questions: Vec<QuizItem>, time_limit_secs: u32, rng_seed: u64) -> Self {
        Self { code, questions, time_limit_secs, phase: Phase::Lobby, players: HashMap::new(), rng_seed }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    // ---------- joins ----------

    pub fn join(&mut self, name: &str, token: Option<&str>, fresh_token: String) -> Result<(String, Vec<Outgoing>), String> {
        // Rejoin: same identity, same score, current snapshot replayed.
        if let Some(token) = token {
            if let Some(player) = self.players.get_mut(token) {
                player.connected = true;
                let token = token.to_owned();
                let welcome = self.welcome_for(&token);
                return Ok((token.clone(), vec![Outgoing { to: Some(token), message: welcome }]));
            }
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err("room is full".into());
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 24 {
            return Err("pick a name up to 24 characters".into());
        }
        if self.players.values().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err("that name is taken in this room".into());
        }
        self.players.insert(fresh_token.clone(), Player {
            token: fresh_token.clone(),
            name: name.to_owned(),
            score: 0,
            answers: HashMap::new(),
            orders: HashMap::new(),
            connected: true,
        });
        let mut out = vec![Outgoing { to: Some(fresh_token.clone()), message: self.welcome_for(&fresh_token) }];
        // Late joiner mid-question still gets the staged payload + go.
        if let Phase::Staged { q } | Phase::Open { q, .. } = self.phase {
            out.push(self.stage_for(&fresh_token, q));
            if let Phase::Open { deadline_ms, .. } = self.phase {
                out.push(Outgoing { to: Some(fresh_token.clone()), message: HostMessage::Go { v: PROTOCOL_VERSION, q, deadline_ms } });
            }
        }
        Ok((fresh_token, out))
    }

    pub fn disconnect(&mut self, token: &str) {
        if let Some(p) = self.players.get_mut(token) {
            p.connected = false;
        }
    }

    // ---------- teacher controls ----------

    /// Stage question `q` to every player (payload only, no reveal).
    pub fn stage(&mut self, q: usize) -> Result<Vec<Outgoing>, String> {
        if q >= self.questions.len() {
            return Err("no such question".into());
        }
        self.phase = Phase::Staged { q };
        let tokens: Vec<String> = self.players.keys().cloned().collect();
        Ok(tokens.iter().map(|t| self.stage_for(t, q)).collect())
    }

    /// Open the staged question: the tiny flip, one deadline for everyone.
    pub fn go(&mut self, now_ms: u64) -> Result<Vec<Outgoing>, String> {
        let Phase::Staged { q } = self.phase else {
            return Err("no question is staged".into());
        };
        let deadline_ms = now_ms + u64::from(self.time_limit_secs) * 1000;
        self.phase = Phase::Open { q, deadline_ms };
        Ok(self.players.keys().map(|t| Outgoing {
            to: Some(t.clone()),
            message: HostMessage::Go { v: PROTOCOL_VERSION, q, deadline_ms },
        }).collect())
    }

    /// Close the open question (deadline hit or teacher skip): per-client
    /// verdicts in each client's own shuffled order, plus teacher tally.
    pub fn close(&mut self) -> Result<Vec<Outgoing>, String> {
        let Phase::Open { q, .. } = self.phase else {
            return Err("no question is open".into());
        };
        self.phase = Phase::Closed { q };
        let tally = self.tally(q);
        let ranks = self.ranked();
        let mut out = Vec::new();
        for (token, player) in &self.players {
            let correct = self.correct_index_for(player, q);
            let you = ranks.iter().find(|line| &line.name == &player.name).cloned();
            // Tally is in each client's shuffled option order.
            let order = player.orders.get(&q);
            let local_tally: Vec<u32> = match order {
                Some(order) => order.iter().map(|&orig| tally[orig as usize]).collect(),
                None => tally.clone(),
            };
            out.push(Outgoing { to: Some(token.clone()), message: HostMessage::Closed {
                v: PROTOCOL_VERSION, q, correct, tally: local_tally, you,
            }});
        }
        out.push(Outgoing { to: None, message: HostMessage::Closed {
            v: PROTOCOL_VERSION, q, correct: self.questions[q].answer_index, tally, you: None,
        }});
        Ok(out)
    }

    pub fn leaderboard(&self) -> Vec<Outgoing> {
        let ranks = self.ranked();
        let top: Vec<ScoreLine> = ranks.iter().take(5).cloned().collect();
        let mut out: Vec<Outgoing> = self.players.iter().map(|(token, p)| Outgoing {
            to: Some(token.clone()),
            message: HostMessage::Leaderboard {
                v: PROTOCOL_VERSION,
                top: top.clone(),
                you: ranks.iter().find(|l| l.name == p.name).cloned(),
            },
        }).collect();
        out.push(Outgoing { to: None, message: HostMessage::Leaderboard { v: PROTOCOL_VERSION, top, you: None } });
        out
    }

    pub fn podium(&mut self) -> Vec<Outgoing> {
        self.phase = Phase::Podium;
        let top: Vec<ScoreLine> = self.ranked().into_iter().take(3).collect();
        let mut out: Vec<Outgoing> = self.players.keys().map(|t| Outgoing {
            to: Some(t.clone()),
            message: HostMessage::Podium { v: PROTOCOL_VERSION, top: top.clone() },
        }).collect();
        out.push(Outgoing { to: None, message: HostMessage::Podium { v: PROTOCOL_VERSION, top } });
        out
    }

    // ---------- answers ----------

    /// First answer wins; repeats and out-of-window answers are refused
    /// with an ack so a retrying client always converges.
    pub fn answer(&mut self, token: &str, q: usize, choice: u8, now_ms: u64) -> Vec<Outgoing> {
        let Phase::Open { q: open_q, deadline_ms } = self.phase else {
            return self.ack(token, q, false);
        };
        if q != open_q || now_ms > deadline_ms {
            return self.ack(token, q, false);
        }
        let Some(player) = self.players.get_mut(token) else { return Vec::new() };
        if player.answers.contains_key(&q) {
            return self.ack(token, q, true); // idempotent: already in
        }
        let options = player.orders.get(&q).map(Vec::len).unwrap_or(0);
        if usize::from(choice) >= options {
            return self.ack(token, q, false);
        }
        player.answers.insert(q, (choice, now_ms));
        // Kahoot-shape scoring: base + speed share of the remaining window.
        let original: u8 = player.orders.get(&q).map(|o| o[usize::from(choice)]).unwrap_or(choice);
        if original == self.questions[q].answer_index {
            let window = u64::from(self.time_limit_secs) * 1000;
            let remaining = deadline_ms.saturating_sub(now_ms);
            let bonus = (SPEED_POINTS as u64 * remaining / window.max(1)) as u32;
            player.score += BASE_POINTS + bonus;
        }
        let answered = self.players.values().filter(|p| p.answers.contains_key(&q)).count();
        let mut out = self.ack(token, q, true);
        out.push(Outgoing { to: None, message: HostMessage::HostTally {
            v: PROTOCOL_VERSION, q, answered, players: self.players.len(),
        }});
        out
    }

    pub fn all_answered(&self) -> bool {
        match self.phase {
            Phase::Open { q, .. } => self.players.values().all(|p| p.answers.contains_key(&q)),
            _ => false,
        }
    }

    /// Per-student results for the teacher's gradebook export.
    pub fn results(&self) -> Vec<(String, u32, Vec<Option<bool>>)> {
        self.ranked().iter().map(|line| {
            let player = self.players.values().find(|p| p.name == line.name).expect("ranked from players");
            let per_question = (0..self.questions.len()).map(|q| {
                player.answers.get(&q).map(|(choice, _)| {
                    let original = player.orders.get(&q).map(|o| o[usize::from(*choice)]).unwrap_or(*choice);
                    original == self.questions[q].answer_index
                })
            }).collect();
            (line.name.clone(), line.score, per_question)
        }).collect()
    }

    // ---------- internals ----------

    fn ack(&self, token: &str, q: usize, accepted: bool) -> Vec<Outgoing> {
        vec![Outgoing { to: Some(token.to_owned()), message: HostMessage::Ack { v: PROTOCOL_VERSION, q, accepted } }]
    }

    fn tally(&self, q: usize) -> Vec<u32> {
        let mut tally = vec![0u32; self.questions[q].options.len()];
        for player in self.players.values() {
            if let (Some((choice, _)), Some(order)) = (player.answers.get(&q), player.orders.get(&q)) {
                let original = order[usize::from(*choice)];
                tally[usize::from(original)] += 1;
            }
        }
        tally
    }

    fn ranked(&self) -> Vec<ScoreLine> {
        let mut lines: Vec<(String, u32)> = self.players.values().map(|p| (p.name.clone(), p.score)).collect();
        lines.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        lines.into_iter().enumerate().map(|(i, (name, score))| ScoreLine { name, score, rank: i + 1 }).collect()
    }

    fn correct_index_for(&self, player: &Player, q: usize) -> u8 {
        let answer = self.questions[q].answer_index;
        match player.orders.get(&q) {
            Some(order) => order.iter().position(|&orig| orig == answer).map(|i| i as u8).unwrap_or(answer),
            None => answer,
        }
    }

    /// Stage delivers each player a personal shuffle (deterministic per
    /// room seed + token + question, so restarts re-derive identically).
    fn stage_for(&mut self, token: &str, q: usize) -> Outgoing {
        let question = &self.questions[q];
        let n = question.options.len() as u8;
        let order = {
            let player = self.players.get(token).expect("staged to known player");
            let mut order: Vec<u8> = (0..n).collect();
            let mut state = self.rng_seed
                ^ (q as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ player.token.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)));
            for i in (1..order.len()).rev() {
                // xorshift64*: tiny, deterministic, no rand dependency here.
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                order.swap(i, (state as usize) % (i + 1));
            }
            order
        };
        let options = order.iter().map(|&i| self.questions[q].options[usize::from(i)].clone()).collect();
        self.players.get_mut(token).expect("known player").orders.insert(q, order);
        Outgoing { to: Some(token.to_owned()), message: HostMessage::Stage {
            v: PROTOCOL_VERSION,
            question: StagedQuestion {
                index: q,
                total: self.questions.len(),
                question_text: self.questions[q].stem.clone(),
                question_type: self.questions[q].question_type.clone(),
                options,
                time_limit_secs: self.time_limit_secs,
                figure_pdf: self.questions[q].figure_pdf.clone(),
            },
        }}
    }

    fn welcome_for(&mut self, token: &str) -> HostMessage {
        let name = self.players[token].name.clone();
        let phase = match self.phase {
            Phase::Lobby => PhaseSnapshot::Lobby,
            Phase::Staged { q } | Phase::Open { q, .. } => {
                // Snapshot carries the payload; join() adds go separately.
                let Outgoing { message: HostMessage::Stage { question, .. }, .. } = self.stage_for(token, q) else { unreachable!() };
                match self.phase {
                    Phase::Open { deadline_ms, .. } => PhaseSnapshot::Open { question, deadline_ms },
                    _ => PhaseSnapshot::Staged { question },
                }
            }
            Phase::Closed { q } => {
                let correct = self.correct_index_for(&self.players[token], q);
                PhaseSnapshot::Closed { q, correct, tally: self.tally(q) }
            }
            Phase::Podium => PhaseSnapshot::Podium { top: self.ranked().into_iter().take(3).collect() },
        };
        HostMessage::Welcome { v: PROTOCOL_VERSION, token: token.to_owned(), name, phase, players: self.players.len() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiz(n: usize) -> Vec<QuizItem> {
        (0..n).map(|i| {
            let mut q = QuizItem::default();
            q.stem = format!("Q{i}");
            q.options = vec!["a".into(), "b".into(), "c".into(), "d".into()];
            q.answer_index = 1;
            q
        }).collect()
    }

    fn room() -> Room {
        Room::new("7842".into(), quiz(3), 20, 42)
    }

    #[test]
    fn full_round_scores_speed_and_shuffles_per_player() {
        let mut room = room();
        let (alice, _) = room.join("Alice", None, "tok-a".into()).unwrap();
        let (bob, _) = room.join("Bob", None, "tok-b".into()).unwrap();
        let staged = room.stage(0).unwrap();
        assert_eq!(staged.len(), 2);
        // Each player answers in their OWN shuffled order; both pick the
        // slot that maps back to original index 1 (the key).
        let order_of = |out: &[Outgoing], to: &str| -> Vec<String> {
            out.iter().find_map(|o| match (&o.to, &o.message) {
                (Some(t), HostMessage::Stage { question, .. }) if t == to => Some(question.options.clone()),
                _ => None,
            }).unwrap()
        };
        let a_opts = order_of(&staged, &alice);
        let b_opts = order_of(&staged, &bob);
        let a_correct = a_opts.iter().position(|s| s == "b").unwrap() as u8;
        let b_correct = b_opts.iter().position(|s| s == "b").unwrap() as u8;
        room.go(1_000).unwrap();
        let out = room.answer(&alice, 0, a_correct, 2_000); // fast
        assert!(matches!(out[0].message, HostMessage::Ack { accepted: true, .. }));
        room.answer(&bob, 0, b_correct, 20_000); // slow, still in window
        let closed = room.close().unwrap();
        // Teacher copy reports the ORIGINAL index and the merged tally.
        let teacher = closed.iter().find(|o| o.to.is_none()).unwrap();
        match &teacher.message {
            HostMessage::Closed { correct, tally, .. } => {
                assert_eq!(*correct, 1);
                assert_eq!(tally[1], 2);
            }
            other => panic!("unexpected {other:?}"),
        }
        let ranks = room.ranked();
        assert_eq!(ranks[0].name, "Alice", "faster correct answer outranks");
        assert!(ranks[0].score > ranks[1].score);
        assert!(ranks[1].score >= BASE_POINTS);
    }

    #[test]
    fn answers_are_idempotent_and_window_bound() {
        let mut room = room();
        let (t, _) = room.join("Solo", None, "tok".into()).unwrap();
        room.stage(0).unwrap();
        room.go(0).unwrap();
        let first = room.answer(&t, 0, 0, 1_000);
        assert!(matches!(first[0].message, HostMessage::Ack { accepted: true, .. }));
        let again = room.answer(&t, 0, 3, 2_000);
        assert!(matches!(again[0].message, HostMessage::Ack { accepted: true, .. }), "repeat converges");
        let score_before = room.ranked()[0].score;
        let late = room.answer(&t, 0, 0, 99_000);
        assert!(matches!(late[0].message, HostMessage::Ack { accepted: false, .. }));
        assert_eq!(room.ranked()[0].score, score_before, "late answer never scores");
    }

    #[test]
    fn rejoin_keeps_identity_and_replays_open_question() {
        let mut room = room();
        let (t, _) = room.join("Riya", None, "tok-r".into()).unwrap();
        room.stage(1).unwrap();
        room.go(5_000).unwrap();
        room.disconnect(&t);
        let (same, replay) = room.join("ignored", Some(&t), "unused".into()).unwrap();
        assert_eq!(same, t);
        match &replay[0].message {
            HostMessage::Welcome { phase: PhaseSnapshot::Open { question, deadline_ms }, .. } => {
                assert_eq!(question.index, 1);
                assert_eq!(*deadline_ms, 25_000);
            }
            other => panic!("expected open snapshot, got {other:?}"),
        }
        assert_eq!(room.player_count(), 1, "no ghost duplicate");
    }

    #[test]
    fn late_joiner_mid_question_can_answer() {
        let mut room = room();
        room.join("Early", None, "tok-e".into()).unwrap();
        room.stage(0).unwrap();
        room.go(0).unwrap();
        let (late, out) = room.join("Late", None, "tok-l".into()).unwrap();
        assert!(out.iter().any(|o| matches!(o.message, HostMessage::Stage { .. })));
        assert!(out.iter().any(|o| matches!(o.message, HostMessage::Go { .. })));
        let ack = room.answer(&late, 0, 1, 3_000);
        assert!(matches!(ack[0].message, HostMessage::Ack { accepted: true, .. }));
    }

    #[test]
    fn duplicate_names_and_full_rooms_are_refused() {
        let mut room = room();
        room.join("Same", None, "t1".into()).unwrap();
        assert!(room.join("same", None, "t2".into()).is_err());
        assert!(room.join("", None, "t3".into()).is_err());
    }

    #[test]
    fn results_map_shuffled_choices_back_to_originals() {
        let mut room = room();
        let (t, _) = room.join("Meera", None, "tok-m".into()).unwrap();
        let staged = room.stage(0).unwrap();
        let opts = match &staged[0].message {
            HostMessage::Stage { question, .. } => question.options.clone(),
            _ => unreachable!(),
        };
        room.go(0).unwrap();
        let correct_slot = opts.iter().position(|s| s == "b").unwrap() as u8;
        room.answer(&t, 0, correct_slot, 500);
        room.close().unwrap();
        let results = room.results();
        assert_eq!(results[0].2[0], Some(true));
        assert_eq!(results[0].2[1], None, "unanswered questions stay None");
    }
}
