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
    tracks: Vec<usize>,
    order: Order,
    pos: usize,
}

impl Playlist {
    pub fn new(library: &Library, order: Order) -> Playlist {
        // Every recognised format is playable, so the playlist is just all
        // scanned tracks.
        let tracks: Vec<usize> = (0..library.tracks.len()).collect();
        Playlist {
            tracks,
            order,
            pos: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Index into `library.tracks` of the next track to play.
    pub fn next(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        match self.order {
            Order::Shuffle => {
                let i = rand::rng().random_range(0..self.tracks.len());
                Some(self.tracks[i])
            }
            Order::Sequential => {
                let idx = self.tracks[self.pos % self.tracks.len()];
                self.pos += 1;
                Some(idx)
            }
        }
    }
}
