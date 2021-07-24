#[cfg(test)]
mod tests;

use rand::prelude::*;
use std::collections::{hash_map::Entry, HashMap, HashSet, VecDeque};

use crate::utils::*;

#[derive(Debug)]
pub struct Player {
    pub id: u64,
    pub name: String,
    pub room: Option<u64>,
    sender: Sender<Response>,
    pub ingame: Option<IngameProp>,
    pub ready: bool,
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

    #[cfg(test)]
    pub fn export(&self) -> PlayerExport {
        PlayerExport {
            id: self.id,
            name: self.name.clone(),
            room: self.room,
            ingame: self.ingame.clone(),
            ready: self.ready,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerExport {
    pub id: u64,
    pub name: String,
    pub room: Option<u64>,
    pub ingame: Option<IngameProp>,
    pub ready: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IngameProp {
    pub position: (u8, u8),
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomExport {
    pub order: VecDeque<u64>,
    pub players: HashSet<u64>,
}

#[derive(Debug)]
pub struct Room {
    pub order: VecDeque<u64>,
    pub players: HashSet<u64>,
    rng: SmallRng,
}

impl Room {
    pub fn new() -> Self {
        Room {
            order: VecDeque::new(),
            players: HashSet::new(),
            rng: SmallRng::from_entropy(),
        }
    }

    pub fn is_gamming(&self) -> bool {
        self.order.len() > 1
    }

    pub fn winner(&self) -> Option<u64> {
        if self.order.len() == 1 && self.players.len() > 1 {
            Some(self.order[0])
        } else {
            None
        }
    }

    pub fn start(&mut self) {
        self.order = self.players.iter().map(|x| x.to_owned()).collect();
        self.order.make_contiguous().shuffle(&mut self.rng);
    }

    #[cfg(test)]
    pub fn export(&self) -> RoomExport {
        RoomExport {
            order: self.order.clone(),
            players: self.players.clone(),
        }
    }
}

impl Default for Room {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExport {
    pub players: HashMap<u64, PlayerExport>,
    pub rooms: HashMap<u64, RoomExport>,
}

pub struct Game {
    receiver: Receiver<In>,
    pub players: HashMap<u64, Player>,
    pub rooms: HashMap<u64, Room>,
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
                #[cfg(test)]
                Export(sender) => {
                    sender.send(self.export()).ok();
                }
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
                        id,
                        name,
                        room: None,
                        sender,
                        ingame: None,
                        ready: false,
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
                r.players.remove(&id); // TODO: remove empty room
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
                    entry.insert(Room::new());
                    return id;
                }
            }
        }
    }

    async fn try_start(&mut self, room_id: u64) {
        let room = self.rooms.get_mut(&room_id).expect("no room found");
        if room.is_gamming() {
            return;
        }
        for i in room.players.clone() {
            if let Some(player) = self.players.get(&i) {
                if !player.ready {
                    return;
                }
            } else {
                room.players.remove(&i);
            }
        }
        let mut to_delete = Vec::new();
        for i in room.players.iter() {
            let p = self.players.get_mut(i).unwrap();
            if !p.send(Response::GameStarted).await {
                to_delete.push(p.id);
            }
        }
        for i in to_delete {
            self.remove_player(i);
        }
        self.rooms.get_mut(&room_id).unwrap().start();
    }

    async fn perform_action(&mut self, player_id: u64, action: Action) {
        if self.players.get(&player_id).is_none() {
            return;
        }
        use Action::*;
        use Response::*;
        match action {
            CreateRoom { name } => {
                let id = self.insert_room(name);
                self.rooms.get_mut(&id).unwrap().players.insert(player_id);
                let player = self.players.get_mut(&player_id).unwrap();
                player.room = Some(id);
                if !player.send(Response::RoomCreated(id)).await {
                    self.remove_player(player_id);
                }
            }
            JoinRoom { id } => {
                let player = self.players.get_mut(&player_id).unwrap();
                if let Some(room) = self.rooms.get_mut(&id) {
                    room.players.insert(player_id);
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
            Ready(x, y) => {
                let player = self.players.get_mut(&player_id).unwrap();
                let room = if let Some(id) = player.room {
                    id
                } else {
                    if !player
                        .send(Error("You need join room first".to_string())) // TODO: Error
                        .await
                    {
                        self.remove_player(player_id);
                    }
                    return;
                };
                player.ingame = Some(IngameProp { position: (x, y) });
                player.ready = true;
                self.try_start(room).await;
            }
            Game(game) => {
                todo!()
            }
        }
    }

    #[cfg(test)]
    fn export(&self) -> GameExport {
        let players = self.players.iter().map(|(&k, v)| (k, v.export())).collect();
        let rooms = self.rooms.iter().map(|(&k, v)| (k, v.export())).collect();
        GameExport { players, rooms }
    }
}
