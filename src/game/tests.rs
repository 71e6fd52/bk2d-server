use super::*;
use enum_macro::em;

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
    };
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
                .expect(&format!("player {} not exists", name))
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

    let mut players = game.rooms.get(&room).expect("room not exists").iter();
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

    let players = game.rooms.get(&room).expect("room not exists").to_owned();
    let mut should = HashSet::new();
    should.insert(player);
    should.insert(player2);
    assert_eq!(players, should);
    Ok(())
}
