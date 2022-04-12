#![allow(clippy::bool_assert_comparison)]
#![allow(unused_variables)]

use super::*;
use enum_macro::em;
use futures::{select, FutureExt};
use std::time::Duration;

macro_rules! setup {
    ($sender:ident, $handle:ident) => {
        let (mut $sender, game_receiver) = mpsc::unbounded();
        let game = crate::game::Game::new(game_receiver);
        let $handle = task::spawn(game.main_loop());
    };
    ($sender:ident) => {
        setup!($sender, _game_handle)
    };
}

macro_rules! new_player {
    ($sender:ident, $name:expr, $player:ident, $response_receiver:ident) => {
        let (response_sender, mut $response_receiver) = mpsc::unbounded();
        let (send, recv) = oneshot::channel();
        $sender
            .send(In::NewPlayer($name, response_sender, send))
            .await?;
        let $player = recv.await?;

        println!("create new player {} {}", $name, $player);
    };
}

macro_rules! export {
    ($sender:ident) => {{
        let (sender, receiver) = oneshot::channel();
        $sender.send(In::Export(sender)).await?;
        receiver.await?
    }};
}

macro_rules! receive {
    ($rec:ident) => {{
        let dur = Duration::from_secs(1);
        let a = async_std::future::timeout(dur, $rec.next())
            .await
            .expect("Not receive anything")
            .expect("receiver closed");
        println!("{} received {:?}", stringify!($rec), a);
        a
    }};
    (nothing in $rec:ident) => {{
        let dur = Duration::from_secs(1);
        let a = async_std::future::timeout(dur, $rec.next()).await;
        if a.is_ok() {
            let a = a.expect("receiver closed");
            panic!(
                "{} should receive nothing but received {:?}",
                stringify!($rec),
                a
            )
        }
    }};
}

#[async_std::test]
async fn test_setup() {
    setup!(_game_sender, _game_handle);
}

#[async_std::test]
async fn test_add_player() -> Result<()> {
    setup!(game_sender, game_handle);
    let (send, recv) = oneshot::channel();
    let (response_sender, _response_receiver) = mpsc::unbounded();
    game_sender
        .send(In::NewPlayer("yahvk".to_string(), response_sender, send))
        .await?;
    let id = recv.await?;
    drop(game_sender);
    let game = game_handle.await;
    assert_eq!(
        &game.players.get(&id).expect("player yahvk not exists").name,
        "yahvk"
    );
    Ok(())
}

#[async_std::test]
async fn test_add_three_player() -> Result<()> {
    setup!(game_sender, game_handle);

    let mut ids = Vec::new();

    for name in ["aa", "bb", "cc"] {
        let (send, recv) = oneshot::channel();
        let (response_sender, _response_receiver) = mpsc::unbounded();

        game_sender
            .send(In::NewPlayer(name.to_string(), response_sender, send))
            .await?;

        ids.push(recv.await?);
    }
    drop(game_sender);

    let game = game_handle.await;

    for (i, name) in ["aa", "bb", "cc"].iter().enumerate() {
        assert_eq!(
            &game
                .players
                .get(&ids[i])
                .unwrap_or_else(|| panic!("player {} not exists", name))
                .name,
            name
        );
    }
    Ok(())
}

#[async_std::test]
async fn test_create_room() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "yahvk".to_string(), player, response_receiver);

    game_sender
        .send(In::PlayerAction {
            player,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = response_receiver.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    drop(game_sender);
    let game = game_handle.await;
    assert_eq!(
        game.players
            .get(&player)
            .expect("player yahvk not exists")
            .room,
        Some(room)
    );

    let mut players = game
        .rooms
        .get(&room)
        .expect("room not exists")
        .players
        .iter();
    assert_eq!(players.next(), Some(&player));
    assert_eq!(players.next(), None);
    Ok(())
}

#[async_std::test]
async fn test_join_room() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "yahvk".to_string(), player, response_receiver);
    new_player!(
        game_sender,
        "yahvk2".to_string(),
        player2,
        response_receiver2
    );

    game_sender
        .send(In::PlayerAction {
            player,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = response_receiver.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: player2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    let res = response_receiver2.next().await;
    if !em!(res.unwrap() => is Response::RoomJoined|) {
        panic!("Can't join room")
    };

    drop(game_sender);
    let game = game_handle.await;
    assert_eq!(
        game.players
            .get(&player)
            .expect("player yahvk not exists")
            .room,
        Some(room)
    );
    assert_eq!(
        game.players
            .get(&player2)
            .expect("player yahvk2 not exists")
            .room,
        Some(room)
    );

    let players = game
        .rooms
        .get(&room)
        .expect("room not exists")
        .players
        .to_owned();
    let mut should = HashSet::new();
    should.insert(player);
    should.insert(player2);
    assert_eq!(players, should);
    Ok(())
}

#[async_std::test]
async fn test_start_game() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "yahvk".to_string(), player, response_receiver);
    new_player!(game_sender, "yahv".to_string(), player2, response_receiver2);

    game_sender
        .send(In::PlayerAction {
            player,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = response_receiver.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: player2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    let res = response_receiver2.next().await;
    if !em!(res.unwrap() => is Response::RoomJoined|) {
        panic!("Can't join room")
    };

    game_sender
        .send(In::PlayerAction {
            player,
            action: Action::Ready(1, 2),
        })
        .await?;

    {
        let data = export!(game_sender);

        let p1 = data.players.get(&player).unwrap();
        let p2 = data.players.get(&player2).unwrap();

        assert_eq!(p1.ready, true);
        assert_eq!(p2.ready, false);
        assert_eq!(
            p1.ingame.as_ref().expect("player not have ingame").position,
            (1, 2)
        );
        assert!(p2.ingame.is_none());

        let room = data.rooms.get(&room).expect("room not exists");

        assert!(room.order.is_empty());
    }

    game_sender
        .send(In::PlayerAction {
            player: player2,
            action: Action::Ready(1, 1),
        })
        .await?;

    if !em!(receive!(response_receiver) => is Response::GameStarted|) {
        panic!("player1 game not start")
    };
    if !em!(receive!(response_receiver2) => is Response::GameStarted|) {
        panic!("player2 game not start")
    };

    drop(game_sender);
    let game = game_handle.await;

    let p1 = game.players.get(&player).unwrap();
    let p2 = game.players.get(&player2).unwrap();

    assert_eq!(p1.ready, true);
    assert_eq!(p2.ready, true);
    assert_eq!(
        p1.ingame
            .as_ref()
            .expect("player1 not have ingame")
            .position,
        (1, 2)
    );
    assert_eq!(
        p2.ingame
            .as_ref()
            .expect("player2 not have ingame")
            .position,
        (1, 1)
    );

    let room = game.rooms.get(&room).expect("room not exists");

    assert_eq!(room.is_gamming(), true);
    assert_eq!(room.order.len(), 2);

    Ok(())
}

async fn start_game() -> Result<(
    Sender<In>,
    task::JoinHandle<Game>,
    ((u64, Receiver<Response>), (u64, Receiver<Response>)),
)> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "pl_a".to_string(), pl1, rec1);
    new_player!(game_sender, "pl_b".to_string(), pl2, rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = rec1.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    let res = rec2.next().await;
    if !em!(res.unwrap() => is Response::RoomJoined|) {
        panic!("Can't join room")
    };

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Ready(1, 1),
        })
        .await?;

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Ready(1, 1),
        })
        .await?;

    if !em!(receive!(rec1) => is Response::GameStarted|) {
        panic!("player1 game not start")
    };
    if !em!(receive!(rec2) => is Response::GameStarted|) {
        panic!("player2 game not start")
    };

    let swap = select! {
        a_res = rec1.next().fuse() => {
            if let Response::Event(Event::TurnStart) = a_res.unwrap() {
                false
            } else {
                panic!("No turn started")
            }
        },
        b_res = rec2.next().fuse() => {
            if let Response::Event(Event::TurnStart) = b_res.unwrap() {
                true
            } else {
                panic!("No turn started")
            }
        },
    };

    Ok((
        game_sender,
        game_handle,
        if swap {
            ((pl2, rec2), (pl1, rec1))
        } else {
            ((pl1, rec1), (pl2, rec2))
        },
    ))
}

#[async_std::test]
async fn test_a_full_game() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "pl_a".to_string(), pl1, rec1);
    new_player!(game_sender, "pl_b".to_string(), pl2, rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = rec1.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    let res = rec2.next().await;
    if !em!(res.unwrap() => is Response::RoomJoined|) {
        panic!("Can't join room")
    };

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Ready(1, 1),
        })
        .await?;

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Ready(1, 1),
        })
        .await?;

    if !em!(receive!(rec1) => is Response::GameStarted|) {
        panic!("player1 game not start")
    };
    if !em!(receive!(rec2) => is Response::GameStarted|) {
        panic!("player2 game not start")
    };

    let swap = select! {
        a_res = rec1.next().fuse() => {
            if let Response::Event(Event::TurnStart) = a_res.unwrap() {
                false
            } else {
                panic!("No turn started")
            }
        },
        b_res = rec2.next().fuse() => {
            if let Response::Event(Event::TurnStart) = b_res.unwrap() {
                true
            } else {
                panic!("No turn started")
            }
        },
    };
    let ((pl1, mut rec1), (pl2, mut rec2)) = if swap {
        ((pl2, rec2), (pl1, rec1))
    } else {
        ((pl1, rec1), (pl2, rec2))
    };

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Attack(1, 2)),
        })
        .await?;
    assert_eq!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => get Event::Attack[x, y])
        .expect("No Attack boardcast"),
        (1, 2),
        "Attack location wrong"
    );
    assert_eq!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => get Event::Attack[x, y])
        .expect("No Attack boardcast"),
        (1, 2),
        "Attack location wrong"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::End),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => is Event::TurnStart|),
        "pl2 turn not start"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::Run(2, 2)),
        })
        .await?;
    assert_eq!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => get Event::Run[x, y])
        .expect("No Run boardcast"),
        (1, 1),
        "Run location wrong"
    );
    assert_eq!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => get Event::Run[x, y])
        .expect("No Run boardcast"),
        (1, 1),
        "Run location wrong"
    );
    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::End),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => is Event::TurnStart|),
        "pl1 turn not start"
    );
    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Move(1, 2)),
        })
        .await?;
    receive!(nothing in rec1);
    receive!(nothing in rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Attack(2, 2)),
        })
        .await?;
    assert_eq!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => get Event::Attack[x, y])
        .expect("No Attack boardcast"),
        (2, 2),
        "Attack location wrong"
    );
    assert_eq!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => get Event::Attack[x, y])
        .expect("No Attack boardcast"),
        (2, 2),
        "Attack location wrong"
    );
    assert!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec1) => get Response::Event).expect("Not game respond") => is Event::GameEnd),
        "Game didn't ended"
    );
    assert!(
        em!(em!(receive!(rec2) => get Response::Event).expect("Not game respond") => is Event::GameEnd),
        "Game didn't ended"
    );

    Ok(())
}

#[async_std::test]
async fn test_illegal_action() -> Result<()> {
    let (mut game_sender, game_handle, ((pl1, mut rec1), (pl2, mut rec2))) = start_game().await?;
    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::Attack(1, 2)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec2) => get Response::Error).expect("Not error") => is Error::NotYourTurn|),
        "No not your turn error"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Attack(1, 2)),
        })
        .await?;
    receive!(rec1);
    receive!(rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Move(1, 2)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec1) => get Response::Error).expect("Not error") => is Error::ActionOrderIncorrect|),
        "No action order incorrect error"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::End),
        })
        .await?;
    receive!(rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Attack(1, 2)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec1) => get Response::Error).expect("Not error") => is Error::NotYourTurn|),
        "No not your turn error"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::Move(4, 4)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec2) => get Response::Error).expect("Not error") => is Error::IllegalParameter|),
        "No illegal parameter error"
    );

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::Move(1, 2)),
        })
        .await?;
    receive!(nothing in rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Game(GameAction::Attack(2, 2)),
        })
        .await?;
    assert!(em!(receive!(rec2) => get Response::Error).is_none());
    // TODO: test run

    Ok(())
}

#[async_std::test]
async fn test_once_kill_all() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "pl_a".to_string(), pl1, rec1);
    new_player!(game_sender, "pl_b".to_string(), pl2, rec2);
    new_player!(game_sender, "pl_c".to_string(), pl3, rec3);
    new_player!(game_sender, "pl_d".to_string(), pl4, rec4);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = rec1.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec2);
    game_sender
        .send(In::PlayerAction {
            player: pl3,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec3);
    game_sender
        .send(In::PlayerAction {
            player: pl4,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec4);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl3,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl4,
            action: Action::Ready(1, 1),
        })
        .await?;
    receive!(rec1);
    receive!(rec2);
    receive!(rec3);
    receive!(rec4);

    let (pl, mut rec) = select! {
        a_res = rec1.next().fuse() => {
            if let Response::Event(Event::TurnStart) = a_res.unwrap() {
                (pl1, rec1)
            } else {
                panic!("No turn started")
            }
        },
        b_res = rec2.next().fuse() => {
            if let Response::Event(Event::TurnStart) = b_res.unwrap() {
                (pl2, rec2)
            } else {
                panic!("No turn started")
            }
        },
        c_res = rec3.next().fuse() => {
            if let Response::Event(Event::TurnStart) = c_res.unwrap() {
                (pl3, rec3)
            } else {
                panic!("No turn started")
            }
        },
        d_res = rec4.next().fuse() => {
            if let Response::Event(Event::TurnStart) = d_res.unwrap() {
                (pl4, rec4)
            } else {
                panic!("No turn started")
            }
        },
    };

    game_sender
        .send(In::PlayerAction {
            player: pl,
            action: Action::Game(GameAction::Move(1, 2)),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl,
            action: Action::Game(GameAction::Attack(1, 1)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Attack[]),
        "No attack boardcast"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    let winner = em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => get Event::GameEnd).expect("Game didn't ended");
    drop(game_sender);
    let game = game_handle.await;

    let p = game.players.get(&pl).unwrap();
    let name = p.name.clone();
    assert_eq!(
        p.ingame.as_ref().expect("player not have ingame").position,
        (1, 2)
    );

    let room = game.rooms.get(&room).expect("room not exists");

    assert_eq!(room.is_gamming(), false);
    assert_eq!(room.order.len(), 1);
    assert_eq!(room.winner(), Some(pl));
    assert_eq!(winner, name);

    Ok(())
}

#[async_std::test]
async fn test_once_kill_all_include_self() -> Result<()> {
    setup!(game_sender, game_handle);
    new_player!(game_sender, "pl_a".to_string(), pl1, rec1);
    new_player!(game_sender, "pl_b".to_string(), pl2, rec2);
    new_player!(game_sender, "pl_c".to_string(), pl3, rec3);
    new_player!(game_sender, "pl_d".to_string(), pl4, rec4);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::CreateRoom {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = rec1.next().await;
    let room = em!(room.unwrap() => get Response::RoomCreated).expect("Can't get room id");

    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec2);
    game_sender
        .send(In::PlayerAction {
            player: pl3,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec3);
    game_sender
        .send(In::PlayerAction {
            player: pl4,
            action: Action::JoinRoom { id: room },
        })
        .await?;
    receive!(rec4);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl2,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl3,
            action: Action::Ready(1, 1),
        })
        .await?;
    game_sender
        .send(In::PlayerAction {
            player: pl4,
            action: Action::Ready(1, 1),
        })
        .await?;
    receive!(rec1);
    receive!(rec2);
    receive!(rec3);
    receive!(rec4);

    let (pl, mut rec, name) = select! {
        a_res = rec1.next().fuse() => {
            if let Response::Event(Event::TurnStart) = a_res.unwrap() {
                (pl1, rec1, "pl_a")
            } else {
                panic!("No turn started")
            }
        },
        b_res = rec2.next().fuse() => {
            if let Response::Event(Event::TurnStart) = b_res.unwrap() {
                (pl2, rec2, "pl_b")
            } else {
                panic!("No turn started")
            }
        },
        c_res = rec3.next().fuse() => {
            if let Response::Event(Event::TurnStart) = c_res.unwrap() {
                (pl3, rec3, "pl_c")
            } else {
                panic!("No turn started")
            }
        },
        d_res = rec4.next().fuse() => {
            if let Response::Event(Event::TurnStart) = d_res.unwrap() {
                (pl4, rec4, "pl_d")
            } else {
                panic!("No turn started")
            }
        },
    };

    game_sender
        .send(In::PlayerAction {
            player: pl,
            action: Action::Game(GameAction::Attack(1, 1)),
        })
        .await?;
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Attack[]),
        "No attack boardcast"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => is Event::Die),
        "Player didn't died"
    );
    assert_eq!(
        em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => get Event::Die)
            .expect("Self didn't died"),
        name
    );
    let winner = em!(em!(receive!(rec) => get Response::Event).expect("Not game respond") => get Event::GameEnd).expect("Game didn't ended");
    drop(game_sender);
    let game = game_handle.await;

    let p = game.players.get(&pl).unwrap();
    let pname = p.name.clone();

    let room = game.rooms.get(&room).expect("room not exists");

    assert_eq!(room.is_gamming(), false);
    assert_eq!(room.order.len(), 1);
    assert_eq!(room.winner(), Some(pl));
    assert_eq!(winner, name);
    assert_eq!(winner, pname);

    Ok(())
}

#[async_std::test]
async fn test_sync_data() -> Result<()> {
    use SyncType::*;

    let (mut game_sender, game_handle, ((pl1, mut rec1), (pl2, mut rec2))) = start_game().await?;

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::Move(1, 2)),
        })
        .await?;

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::Game(GameAction::End),
        })
        .await?;
    receive!(rec2);

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::RequestData(Player),
        })
        .await?;
    let result = em!(receive!(rec1) => get Response::Sync).expect("not Sync");
    let (name, id, (x, y)) =
        em!(result => get ToSync::Player{name, id, position}).expect("not Sync Player");

    game_sender
        .send(In::PlayerAction {
            player: pl1,
            action: Action::RequestData(PlayersOrder),
        })
        .await?;
    let result = em!(receive!(rec1) => get Response::Sync).expect("not Sync");
    let players = em!(result => get ToSync::PlayersOrder).expect("not Sync Player");

    drop(game_sender);
    let game = game_handle.await;

    let p1 = game.players.get(&pl1).unwrap();
    let p2 = game.players.get(&pl2).unwrap();

    assert_eq!(p1.id, id);
    assert_eq!(p1.name, name);
    assert_eq!(x, 1);
    assert_eq!(y, 2);
    assert_eq!(players[0], p2.name);
    assert_eq!(players[1], p1.name);
    Ok(())
}
