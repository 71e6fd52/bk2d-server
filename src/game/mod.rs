#[cfg(test)]
mod tests;

use rand::prelude::*;
use std::collections::{hash_map::Entry, HashMap, HashSet};

use crate::utils::*;

#[derive(Debug)]
pub struct Player {
    name: String,
    room: Option<u64>,
    sender: Sender<Response>,
}

impl Player {
    pub async fn send(&mut self, res: Response) -> bool {
        if let Err(e) = self.sender.send(res).await {
            eprintln!("{}", e); // TODO: use log
            false
        } else {
            true
        }
    }
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

    pub async fn main_loop(mut self) -> Self {
        use In::*;
        while let Some(action) = self.receiver.next().await {
            println!("{:?}", action);
            match action {
                NewPlayer(name, sender, id_sender) => {
                    let id = self.insert_player(name, sender);
                    if let Err(id) = id_sender.send(id) {
                        self.remove_player(id);
                    }
                }
                PlayerAction { player, action } => self.perform_action(player, action).await,
            };
        }
        self
    }

    fn insert_player(&mut self, name: String, sender: Sender<Response>) -> u64 {
        loop {
            let id = self.id_rng.next_u64();
            match self.players.entry(id) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    entry.insert(Player {
                        name,
                        room: None,
                        sender,
                    });
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
                r.remove(&id); // TODO: remove empty room
            });
        }
        true
    }

    fn insert_room(&mut self, _name: String) -> u64 {
        loop {
            let id = self.id_rng.next_u64();
            match self.rooms.entry(id) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    entry.insert(HashSet::new());
                    return id;
                }
            }
        }
    }

    async fn perform_action(&mut self, player_id: u64, action: Action) {
        if self.players.get(&player_id).is_none() {
            return;
        }
        match action {
            Action::CreateRoom { name } => {
                let id = self.insert_room(name);
                self.rooms.get_mut(&id).unwrap().insert(player_id);
                let player = self.players.get_mut(&player_id).unwrap();
                player.room = Some(id);
                if !player.send(Response::RoomCreated(id)).await {
                    self.remove_player(player_id);
                }
            }
            Action::JoinRoom { id } => {
                let player = self.players.get_mut(&player_id).unwrap();
                if let Some(room) = self.rooms.get_mut(&id) {
                    room.insert(player_id);
                } else {
                    if !player
                        .send(Response::Error("Room Not Found".to_string())) // TODO: Error type
                        .await
                    {
                        self.remove_player(player_id);
                    }
                    return;
                }
                player.room = Some(id);
                if !player.send(Response::RoomJoined).await {
                    self.remove_player(player_id);
                }
            }
        }
    }
}
