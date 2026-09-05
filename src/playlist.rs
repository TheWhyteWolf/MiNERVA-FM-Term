//! Track selection. Shuffle mirrors the shell radio (`shuf -n 1` per track:
//! independent random picks, repeats allowed); sequential is a round-robin.

use crate::library::Library;
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Shuffle,
    Sequential,
}

pub struct Playlist {
    /// Every recognised format is playable, so the playlist is the whole
    /// library: `next` returns an index into `library.tracks` directly and
    /// only the track count is needed to pick one.
    len: usize,
    order: Order,
    pos: usize,
}

impl Playlist {
    pub fn new(library: &Library, order: Order) -> Playlist {
        Playlist {
            len: library.tracks.len(),
            order,
            pos: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Index into `library.tracks` of the next track to play.
    pub fn next(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        match self.order {
            Order::Shuffle => Some(rand::rng().random_range(0..self.len)),
            Order::Sequential => {
                let idx = self.pos;
                self.pos = (self.pos + 1) % self.len;
                Some(idx)
            }
        }
    }
}
