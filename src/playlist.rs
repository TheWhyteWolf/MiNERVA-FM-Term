//! Track selection. Shuffle mirrors the shell radio (`shuf -n 1` per track:
//! independent random picks, repeats allowed); sequential is a round-robin.

use crate::engine::Format;
use crate::library::Library;
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Shuffle,
    Sequential,
}

pub struct Playlist {
    playable: Vec<usize>,
    order: Order,
    pos: usize,
}

impl Playlist {
    pub fn new(library: &Library, order: Order) -> Playlist {
        let playable = library
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.format.playable())
            .map(|(i, _)| i)
            .collect();
        Playlist {
            playable,
            order,
            pos: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.playable.is_empty()
    }

    /// Index into `library.tracks` of the next track to play.
    pub fn next(&mut self) -> Option<usize> {
        if self.playable.is_empty() {
            return None;
        }
        match self.order {
            Order::Shuffle => {
                let i = rand::rng().random_range(0..self.playable.len());
                Some(self.playable[i])
            }
            Order::Sequential => {
                let idx = self.playable[self.pos % self.playable.len()];
                self.pos += 1;
                Some(idx)
            }
        }
    }
}

#[allow(dead_code)]
pub fn format_playable(f: Format) -> bool {
    f.playable()
}
