use super::*;

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

#[async_std::test]
async fn test_setup() {
    setup!(_game_sender);
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
    let (response_sender, mut response_receiver) = mpsc::unbounded();
    let (send, recv) = oneshot::channel();
    game_sender
        .send(In::NewPlayer("yahvk".to_string(), response_sender, send))
        .await?;
    let player = recv.await?;

    game_sender
        .send(In::PlayerAction {
            player,
            action: Action::Create {
                name: "room".to_string(),
            },
        })
        .await?;
    let room = response_receiver.next().await;
    let room = if let Response::RoomCreated(id) = room.unwrap() {
        id
    } else {
        panic!("Can't get room id")
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

    let mut players = game.rooms.get(&room).expect("room not exists").iter();
    assert_eq!(players.next(), Some(&player));
    assert_eq!(players.next(), None);
    Ok(())
}
