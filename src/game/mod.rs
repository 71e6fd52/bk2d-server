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

    pub fn ingame(&self) -> &IngameProp {
        self.ingame.as_ref().unwrap()
    }

    pub fn ingame_mut(&mut self) -> &mut IngameProp {
        self.ingame.as_mut().unwrap()
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
    pub stage: u8,
    pub alive: bool,
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
            Some(*self.order.front().unwrap())
        } else {
            None
        }
    }

    pub fn start(&mut self) {
        self.order = self.players.iter().map(|x| x.to_owned()).collect();
        self.order.make_contiguous().shuffle(&mut self.rng);
    }

    pub fn currect_player_id(&self) -> u64 {
        *self.order.front().unwrap()
    }

    pub fn push_player(&mut self) -> u64 {
        self.order.rotate_left(1);

        self.currect_player_id()
    }

    pub async fn boardcast(&mut self, res: Response, players: &mut HashMap<u64, Player>) {
        for player_id in &self.players {
            players.get_mut(player_id).unwrap().send(res.clone()).await;
        }
    }

    pub fn kill_player(&mut self, player_id: u64) {
        panic!("try kill player {}", player_id)
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

macro_rules! send_or_delete {
    ($s:ident, $player:expr, $res:expr) => {
        if !$player.send($res).await {
            let id = $player.id;
            $s.remove_player(id);
        }
    };
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
                                       // TODO: remove in order
            });
            // TODO: notify other player
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
        let room = self.rooms.get_mut(&room_id).unwrap();
        room.start();
        send_or_delete!(
            self,
            self.players.get_mut(room.order.front().unwrap()).unwrap(),
            Response::Game(Event::TurnStart)
        );
    }

    async fn perform_action(&mut self, player_id: u64, action: Action) {
        if self.players.get(&player_id).is_none() {
            return;
        }
        use Action::*;
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
                    send_or_delete!(self, player, Response::Error(Error::RoomNotFound));
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
                    send_or_delete!(self, player, Response::Error(Error::NotJoinRoom));
                    return;
                };
                player.ingame = Some(IngameProp {
                    position: (x, y),
                    stage: 0,
                    alive: true,
                });
                player.ready = true;
                self.try_start(room).await;
            }
            Game(game) => self.perform_game_action(player_id, game).await,
        }
    }

    async fn perform_game_action(&mut self, player_id: u64, action: GameAction) {
        // TODO: make sure game started
        let player = self.players.get_mut(&player_id).unwrap();
        let room = if let Some(id) = player.room {
            if let Some(room) = self.rooms.get_mut(&id) {
                room
            } else {
                send_or_delete!(self, player, Response::Error(Error::RoomNotFound));
                return;
            }
        } else {
            if !player.send(Response::Error(Error::NotJoinRoom)).await {
                self.remove_player(player_id);
            }
            return;
        };
        if room.currect_player_id() != player_id {
            send_or_delete!(self, player, Response::Error(Error::NotYourTurn));
            return;
        }
        use GameAction::*;
        match action {
            Move(x, y) => {
                if player.ingame().stage > 0 {
                    send_or_delete!(self, player, Response::Error(Error::ActionOrderIncorrect));
                    return;
                }
                if (x, y).distance(&player.ingame().position) > 1 {
                    send_or_delete!(self, player, Response::Error(Error::IllegalParameter));
                    return;
                }
                player.ingame_mut().position = (x, y);
                player.ingame_mut().stage = 1;
            }
            Attack(x, y) => {
                if player.ingame().stage > 1 {
                    send_or_delete!(self, player, Response::Error(Error::ActionOrderIncorrect));
                    return;
                }
                if (x, y).distance(&player.ingame().position) != 1 {
                    send_or_delete!(self, player, Response::Error(Error::IllegalParameter));
                    return;
                }
                player.ingame_mut().stage = 2;
                room.boardcast(Response::Game(Event::Attack(x, y)), &mut self.players)
                    .await;
                let mut to_kill = vec![];
                for pl in &room.order {
                    let player = self.players.get_mut(pl).unwrap();
                    if player.ingame().position == (x, y) {
                        player.ingame_mut().alive = false;
                        to_kill.push(*pl);
                    }
                }
                for pl in to_kill {
                    room.kill_player(pl);
                }
            }
            Run(x, y) => {
                if player.ingame().stage > 0 {
                    send_or_delete!(self, player, Response::Error(Error::ActionOrderIncorrect));
                    return;
                }
                if (x, y).distance(&player.ingame().position) != 2 {
                    send_or_delete!(self, player, Response::Error(Error::IllegalParameter));
                    return;
                }
                player.ingame_mut().position = (x, y);
                player.ingame_mut().stage = 1;
            }
            End => todo!(),
        }
    }

    #[cfg(test)]
    fn export(&self) -> GameExport {
        let players = self.players.iter().map(|(&k, v)| (k, v.export())).collect();
        let rooms = self.rooms.iter().map(|(&k, v)| (k, v.export())).collect();
        GameExport { players, rooms }
    }
}
