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
    game_sender
        .send(In::NewPlayer("yahvk".to_string(), send))
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

        game_sender
            .send(In::NewPlayer(name.to_string(), send))
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
