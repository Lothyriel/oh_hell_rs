#[cfg(test)]
mod tests {
    use futures::{stream::FusedStream, SinkExt, StreamExt};
    use oh_hell::{
        infra::{
            auth::{ProfileParams, TokenResponse},
            lobby::CreateLobbyResponse,
            ClientGameMessage, ClientMessage, JoinLobbyDto, ServerMessage,
        },
        models::Card,
    };
    use reqwest::Client;
    use tokio::{net::TcpStream, task};
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    const URL: &str = "http://localhost:3000/";

    #[tokio::test]
    async fn test_example() {
        task::spawn(oh_hell::start_app());

        let mut client = reqwest::Client::new();

        let tokens = get_players(&mut client, 2).await;

        let mut connections = join_lobby(&mut client, &tokens).await;

        ready(&mut connections).await;

        let mut cards_count = 1;

        loop {
            let decks = decks(&mut connections, cards_count).await;

            // todo need to loop cycle the players between sets
            play_set(&mut connections, decks, cards_count).await;

            cards_count += 1;

            for p in &mut connections {
                if assert_game_or_set_ended(p).await {
                    return;
                }
            }

            connections.rotate_right(1);
        }

    }

    async fn get_players(client: &mut Client, count: usize) -> Vec<String> {
        let mut players = vec![];

        for i in 0..count {
            let player = login(client, i).await;
            players.push(player);
        }

        players
    }

    async fn assert_game_or_set_ended(socket: &mut WebSocket) -> bool {
        match recv_msg(socket).await {
            ServerMessage::SetEnded(lifes) => {
                println!("Asserted game msg {:?}", ServerMessage::SetEnded(lifes));
                false
            }
            ServerMessage::GameEnded { winner, lifes } => {
                if lifes.iter().filter(|(_, &lifes)| lifes > 0).count() == 1 {
                    let msg = ServerMessage::GameEnded { lifes, winner };

                    println!("Asserted game msg {:?}", msg);

                    true
                } else {
                    panic!("The game ended with more than 1 winner")
                }
            }
            a => panic!("Expected Set or Game end | {a:?}"),
        }
    }

    async fn play_set(players: &mut Vec<WebSocket>, decks: Vec<Deck>, rounds_count: usize) {
        bidding(players, rounds_count).await;

        for i in 0..rounds_count {
            play_round(players, &decks, i == rounds_count - 1).await;
        }
    }

    type Deck = Vec<Card>;

    async fn play_round(players: &mut [WebSocket], decks: &[Deck], last: bool) {
        for i in 0..players.len() {
            play_turn(players, decks[i][0], i).await;
        }

        if !last {
            for p in players.iter_mut() {
                assert_game_msg(p, validate_round_ended).await;
            }
        }
    }

    async fn play_turn(players: &mut [WebSocket], deck: Card, index: usize) {
        for p in players.iter_mut() {
            assert_game_msg(p, validate_player_turn).await;
        }

        let msg = ClientGameMessage::PlayTurn { card: deck };

        send_msg(&mut players[index], msg).await;

        for p in players.iter_mut() {
            assert_game_msg(p, validate_turn_played).await;
        }
    }

    async fn bidding(players: &mut Vec<WebSocket>, bid: usize) {
        for i in 0..players.len() {
            bid_turn(players, i, bid).await;
        }
    }

    async fn bid_turn(players: &mut Vec<WebSocket>, index: usize, bid: usize) {
        for p in players.iter_mut() {
            assert_game_msg(p, validate_bidding_turn).await;
        }

        send_msg(&mut players[index], ClientGameMessage::PutBid { bid }).await;

        for p in players {
            assert_game_msg(p, validate_player_bidded).await;
        }
    }

    async fn decks(players: &mut Vec<WebSocket>, cards_count: usize) -> Vec<Deck> {
        for p in players.iter_mut() {
            assert_game_msg(p, validate_set_start).await;
        }

        let mut decks = vec![];

        for p in players {
            let deck = get_deck(p).await;

            assert!(deck.len() == cards_count);

            decks.push(deck);
        }

        decks
    }

    async fn join_lobby(client: &mut Client, players: &[String]) -> Vec<WebSocket> {
        let lobby_id = create_lobby(client, &players[0]).await;

        for (i, p) in players.iter().enumerate() {
            let lobby = join_lobby_http(client, p, &lobby_id).await;
            assert!(lobby.players.len() == i + 1);
        }

        let mut connections = vec![];

        for p in players {
            connections.push(connect_ws(p.clone()).await);
        }

        connections
    }

    async fn ready(players: &mut [WebSocket]) {
        let msg = ClientGameMessage::PlayerStatusChange { ready: true };

        for p in players.iter_mut() {
            send_msg(p, msg).await;
        }

        for i in 0..players.len() {
            for _ in 0..players.len() {
                let player = &mut players[i];
                assert_game_msg(player, validate_player_status_change).await;
            }
        }
    }

    fn validate_round_ended(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::RoundEnded(_))
    }

    fn validate_turn_played(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::TurnPlayed { pile: _ })
    }

    fn validate_player_turn(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::PlayerTurn { player_id: _ })
    }

    fn validate_bidding_turn(m: &ServerMessage) -> bool {
        matches!(
            m,
            ServerMessage::PlayerBiddingTurn {
                player_id: _,
                possible_bids: _
            }
        )
    }

    fn validate_player_bidded(m: &ServerMessage) -> bool {
        matches!(
            m,
            ServerMessage::PlayerBidded {
                player_id: _,
                bid: _
            }
        )
    }

    fn validate_player_status_change(m: &ServerMessage) -> bool {
        matches!(
            m,
            ServerMessage::PlayerStatusChange {
                player_id: _,
                ready: _
            }
        )
    }

    fn validate_set_start(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::SetStart { trump: _ })
    }

    async fn get_deck(stream: &mut WebSocket) -> Deck {
        match assert_game_msg(stream, |m| matches!(m, ServerMessage::PlayerDeck(_))).await {
            ServerMessage::PlayerDeck(c) => c,
            _ => panic!("Should be a deck message"),
        }
    }

    async fn assert_game_msg<F>(stream: &mut WebSocket, predicate: F) -> ServerMessage
    where
        F: FnOnce(&ServerMessage) -> bool,
    {
        let msg = recv_msg(stream).await;

        match predicate(&msg) {
            true => {
                println!("Asserted game msg {msg:?}");
                msg
            }
            false => panic!("Message not expected {msg:?}"),
        }
    }

    async fn send_msg(stream: &mut WebSocket, msg: ClientGameMessage) {
        let msg = ClientMessage::Game(msg);

        let msg = serde_json::to_string(&msg).unwrap();

        stream.send(Message::Text(msg)).await.unwrap();
    }

    type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    async fn connect_ws(token: String) -> WebSocket {
        let (mut stream, _) = connect_async("ws://localhost:3000/game").await.unwrap();

        let msg = ClientMessage::Auth { token };

        let json = serde_json::to_string(&msg).unwrap();

        stream.send(Message::Text(json)).await.unwrap();

        assert!(!stream.is_terminated());

        stream
    }

    async fn recv_msg(stream: &mut WebSocket) -> ServerMessage {
        let msg = stream.next().await.unwrap().unwrap();

        let msg: ServerMessage = match msg {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            m => panic!("Error: {m}"),
        };

        msg
    }

    async fn join_lobby_http(client: &mut Client, token: &str, lobby_id: &str) -> JoinLobbyDto {
        let res = client
            .put(format!("{URL}lobby/{lobby_id}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();

        res.json().await.unwrap()
    }

    async fn create_lobby(client: &mut Client, token: &str) -> String {
        let res = client
            .post(format!("{URL}lobby"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();

        let res: CreateLobbyResponse = res.json().await.unwrap();

        res.lobby_id
    }

    async fn login(client: &mut Client, number: usize) -> String {
        let params = ProfileParams {
            picture: "picture.jpg".to_string(),
            nickname: format!("Player {number}"),
        };

        let res = client
            .post(format!("{URL}auth/login"))
            .json(&params)
            .send()
            .await
            .unwrap();

        let res: TokenResponse = res.json().await.unwrap();

        res.token
    }
}
