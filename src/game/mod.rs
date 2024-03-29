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
            // TODO check error type
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
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomExport {
    pub name: String,
    pub order: VecDeque<u64>,
    pub players: HashSet<u64>,
}

#[derive(Debug)]
pub struct Room {
    pub name: String,
    pub order: VecDeque<u64>,
    pub players: HashSet<u64>,
    rng: SmallRng,
}

impl Room {
    pub fn new(name: String) -> Self {
        Room {
            name,
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

    pub async fn boardcast(&self, res: Response, players: &mut HashMap<u64, Player>) {
        for player_id in &self.players {
            players.get_mut(player_id).unwrap().send(res.clone()).await;
        }
    }

    pub fn kill_players(&mut self, player_id: &[u64]) {
        let currect = self.currect_player_id();
        self.order.rotate_left(1);

        while self.currect_player_id() != currect {
            if player_id.contains(&self.currect_player_id()) {
                self.order.pop_front();
            } else {
                self.order.rotate_left(1);
            }
        }

        if self.order.len() != 1 && player_id.contains(&currect) {
            self.order.pop_front();
        }
    }

    #[cfg(test)]
    pub fn export(&self) -> RoomExport {
        RoomExport {
            name: self.name.clone(),
            order: self.order.clone(),
            players: self.players.clone(),
        }
    }
}

// impl Default for Room {
//     fn default() -> Self {
//         Self::new()
//     }
// }

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
            $s.remove_player(id).await;
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
                    // TODO: check exists
                    let id = self.insert_player(name, sender);
                    if let Err(id) = id_sender.send(id) {
                        self.remove_player(id).await;
                    }
                }
                PlayerAction { player, action } => self.perform_action(player, action).await,
                Disconnected(id) => {
                    self.remove_player(id).await;
                }
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

    async fn remove_player(&mut self, id: u64) -> bool {
        let entry = match self.players.entry(id) {
            Entry::Occupied(entry) => entry.remove(),
            Entry::Vacant(_) => return false,
        };
        if let Some(room) = entry.room {
            if let Entry::Occupied(mut o) = self.rooms.entry(room) {
                let r = o.get_mut();
                r.players.remove(&id);

                if r.players.is_empty() {
                    o.remove();
                    return true;
                }

                if let Some(index) = r.order.iter().position(|&x| x == id) {
                    r.order.remove(index);
                }

                r.boardcast(
                    Response::Event(Event::Disconnected, entry.id),
                    &mut self.players,
                )
                .await;

                if let Some(pl) = r.winner() {
                    r.boardcast(Response::Event(Event::GameEnd, pl), &mut self.players)
                        .await;
                }
            }
        }
        true
    }

    fn insert_room(&mut self, name: String) -> u64 {
        loop {
            let id = self.id_rng.next_u64();
            match self.rooms.entry(id) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    entry.insert(Room::new(name));
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
        if room.players.len() == 1 {
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
            self.remove_player(i).await;
        }
        let room = self.rooms.get_mut(&room_id).unwrap();
        room.start();
        // room.boardcast(
        //     Response::Data(Data::PlayersOrder(room.order.clone().into())),
        //     &mut self.players,
        // )
        // .await;

        for id in room.players.clone() {
            self.send_data(id, DataType::Player).await;
            self.send_data(id, DataType::PlayersOrder).await;
            self.send_data(id, DataType::PlayersName).await;
        }
        // let res = self
        //     .rooms
        //     .get(&room_id)
        //     .unwrap()
        //     .players
        //     .iter()
        //     .map(|id| (*id, self.players.get(id).unwrap().name.clone()))
        //     .collect();
        let room = self.rooms.get_mut(&room_id).unwrap();
        // room.boardcast(Response::Data(Data::PlayersName(res)), &mut self.players)
        //     .await;

        send_or_delete!(
            self,
            self.players.get_mut(&room.currect_player_id()).unwrap(),
            Response::Event(Event::TurnStart, room.currect_player_id())
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
                    self.remove_player(player_id).await;
                    return;
                }
                self.send_data(player_id, DataType::PlayersName).await;
                self.send_data(player_id, DataType::PlayersOrder).await;
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
                let name = player.name.clone();
                send_or_delete!(self, player, Response::RoomJoined(id));
                self.rooms
                    .get_mut(&id)
                    .unwrap()
                    .boardcast(
                        Response::Event(Event::NewPlayer(name), player_id),
                        &mut self.players,
                    )
                    .await;
                self.send_data(player_id, DataType::PlayersName).await;
                self.send_data(player_id, DataType::PlayersOrder).await;
            }
            Ready(x, y) => {
                let player = self.players.get_mut(&player_id).unwrap();
                let room = if let Some(id) = player.room {
                    id
                } else {
                    send_or_delete!(self, player, Response::Error(Error::NotJoinedRoom));
                    return;
                };
                player.ingame = Some(IngameProp {
                    position: (x, y),
                    stage: 0,
                });
                player.ready = true;
                self.try_start(room).await;
            }
            Game(game) => {
                self.perform_game_action(player_id, game).await;

                let room = self.players.get(&player_id).unwrap().room.unwrap();
                if let Some(pl) = self.rooms.get(&room).unwrap().winner() {
                    self.rooms
                        .get_mut(&room)
                        .unwrap()
                        .boardcast(Response::Event(Event::GameEnd, pl), &mut self.players)
                        .await;
                }
            }
            RequestData(ty) => {
                self.send_data(player_id, ty).await;
            }
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
            if !player.send(Response::Error(Error::NotJoinedRoom)).await {
                self.remove_player(player_id).await;
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
                if (x, y).distance(&player.ingame().position) > 1 {
                    send_or_delete!(self, player, Response::Error(Error::IllegalParameter));
                    return;
                }
                player.ingame_mut().stage = 2;
                room.boardcast(
                    Response::Event(Event::Attack(x, y), player_id),
                    &mut self.players,
                )
                .await;
                let mut to_kill = vec![];
                room.order.rotate_left(1);
                for pl in &room.order {
                    let player = self.players.get_mut(pl).unwrap();
                    if player.ingame().position == (x, y) {
                        room.boardcast(Response::Event(Event::Die, *pl), &mut self.players)
                            .await;
                        to_kill.push(*pl);
                    }
                }
                room.order.rotate_right(1);
                room.kill_players(&to_kill);
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
                let (oldx, oldy) = player.ingame().position;
                player.ingame_mut().position = (x, y);
                player.ingame_mut().stage = 1;
                room.boardcast(
                    Response::Event(Event::Run(oldx, oldy), player_id),
                    &mut self.players,
                )
                .await;
            }
            End => {
                player.ingame_mut().stage = 0;
                let pl = room.push_player();
                room.boardcast(
                    Response::Data(Data::PlayersOrder(room.order.clone().into())),
                    &mut self.players,
                )
                .await;
                send_or_delete!(
                    self,
                    self.players.get_mut(&pl).unwrap(),
                    Response::Event(Event::TurnStart, pl)
                );
            }
        }
    }

    async fn send_data(&mut self, player_id: u64, ty: DataType) {
        use DataType::*;

        let player = self.players.get_mut(&player_id).unwrap();

        match ty {
            Player => {
                let ingame = if let Some(ingame) = player.ingame.clone() {
                    ingame
                } else {
                    send_or_delete!(self, player, Response::Error(Error::NotInGame));
                    return;
                };
                send_or_delete!(
                    self,
                    player,
                    Response::Data(Data::Player {
                        name: player.name.clone(),
                        id: player.id,
                        position: ingame.position
                    })
                );
            }
            PlayersOrder => {
                let room = if let Some(id) = player.room {
                    self.rooms.get(&id).unwrap()
                } else {
                    send_or_delete!(self, player, Response::Error(Error::NotJoinedRoom));
                    return;
                };
                let res = if room.order.is_empty() {
                    room.players.iter().map(u64::to_owned).collect()
                } else {
                    room.order.clone().into()
                };

                send_or_delete!(
                    self,
                    self.players.get_mut(&player_id).unwrap(),
                    Response::Data(Data::PlayersOrder(res))
                );
            }
            PlayersName => {
                let room = if let Some(id) = player.room {
                    self.rooms.get(&id).unwrap()
                } else {
                    send_or_delete!(self, player, Response::Error(Error::NotJoinedRoom));
                    return;
                };
                let res = room
                    .players
                    .iter()
                    .map(|id| (*id, self.players.get(id).unwrap().name.clone()))
                    .collect();
                send_or_delete!(
                    self,
                    self.players.get_mut(&player_id).unwrap(),
                    Response::Data(Data::PlayersName(res))
                );
            }
            RoomList => {
                let player = self.players.get_mut(&player_id).unwrap();

                let res = self
                    .rooms
                    .iter()
                    .map(|(&k, v)| (k, v.name.clone()))
                    .collect();
                send_or_delete!(self, player, Response::Data(Data::RoomList(res)));
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
