use rand::prelude::*;
use std::collections::{hash_map::Entry, HashMap, HashSet};

use crate::utils::*;

#[derive(Debug)]
pub struct Player {
    name: String,
    room: Option<u64>,
}

pub struct Game {
    receiver: Receiver<In>,
    players: HashMap<u64, Player>,
    rooms: HashMap<u64, HashSet<u64>>,
    id_rng: SmallRng,
}

impl Game {
    pub fn new(receiver: Receiver<In>) -> Game {
        Game {
            receiver,
            players: HashMap::new(),
            rooms: HashMap::new(),
            id_rng: SmallRng::from_entropy(),
        }
    }

    pub async fn main_loop(mut self) {
        use In::*;
        while let Some(action) = self.receiver.next().await {
            println!("{:?}", action);
            match action {
                NewPlayer(name, sender) => {
                    let id = self.insert_player(name);
                    if let Err(id) = sender.send(id) {
                        self.remove_player(id);
                    }
                }
                PlayerAction { player, action } => todo!(),
            };
        }
    }

    fn insert_player(&mut self, name: String) -> u64 {
        loop {
            let id = self.id_rng.next_u64();
            match self.players.entry(id) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    entry.insert(Player { name, room: None });
                    return id;
                }
            }
        }
    }

    fn remove_player(&mut self, id: u64) -> bool {
        let entry = match self.players.entry(id) {
            Entry::Occupied(entry) => entry.remove(),
            Entry::Vacant(_) => return false,
        };
        if let Some(room) = entry.room {
            self.rooms.entry(room).and_modify(|r| {
                r.remove(&id);
            });
        }
        true
    }
}
