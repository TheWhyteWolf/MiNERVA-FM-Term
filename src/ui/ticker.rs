//! The scrolling ticker: message + "    *** " repeated, slid one column every
//! 120 ms, rerolled (message + colour) every 60 s — straight from ticker.sh.

use super::schemes::{ansi, Rgb};
use rand::Rng;
use std::time::{Duration, Instant};

pub const MESSAGES: &[&str] = &[
    "[ WELCOME TO MiNERVA RADIO] Retro game music from across the ages",
    "Birds aren't real. Seriously though. Have you ever seen a baby pigeon? I think not! They're spying on you. Just you",
    "I'm ashamed of what I did for a klondike bar... ",
    "[ MiNERVA RADIO ] Streaming SID, SPC, VGM, MOD and more",
    "PUBLIC ANNOUNCEMENT: Seeding is sharing and sharing is caring",
    "BE PATIENT! Viewers found using antipatience language will be scoffed at",
    "When was the last time you told your mother you loved her?",
    "Help! I'm trapped in a radio!",
    "[MiNERVA RADIO] song requests in general chat! Anything that get's 25+ hearts will be considered. SNES and older only!",
    "Anyway, back to the bird thing. I reckon they're charging by standing on the power lines, that's why they never get fried.",
    "To download is human, to share is DiVINE",
    "If we're not meant to have midnight snacks, why is there a light in the fridge?",
    "Your numbers this week are 05-17-27-29-35 / 07",
];

const COLOURS: &[Rgb] = &[ansi::YELLOW, ansi::GREEN, ansi::CYAN, ansi::MAGENTA, ansi::RED];
const STEP: Duration = Duration::from_millis(120);
const REROLL: Duration = Duration::from_secs(60);

pub struct Ticker {
    base: String,
    pub colour: Rgb,
    offset: usize,
    next_step: Instant,
    next_reroll: Instant,
}

impl Ticker {
    pub fn new() -> Ticker {
        let mut t = Ticker {
            base: String::new(),
            colour: ansi::YELLOW,
            offset: 0,
            next_step: Instant::now(),
            next_reroll: Instant::now(),
        };
        t.reroll(Instant::now());
        t
    }

    fn reroll(&mut self, now: Instant) {
        let mut rng = rand::rng();
        let msg = MESSAGES[rng.random_range(0..MESSAGES.len())];
        self.colour = COLOURS[rng.random_range(0..COLOURS.len())];
        self.base = format!("{msg}    *** ");
        self.offset = 0;
        self.next_reroll = now + REROLL;
    }

    pub fn update(&mut self, now: Instant) {
        if now >= self.next_reroll {
            self.reroll(now);
        }
        while now >= self.next_step {
            self.offset = (self.offset + 1) % self.base.chars().count();
            self.next_step += STEP;
        }
    }

    /// The visible slice, `width` columns wide.
    pub fn line(&self, width: usize) -> String {
        self.base
            .chars()
            .cycle()
            .skip(self.offset)
            .take(width)
            .collect()
    }
}
