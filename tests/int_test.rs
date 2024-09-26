#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use futures::{stream::FusedStream, SinkExt, StreamExt};
    use oh_hell::{
        infra::{
            auth::{get_claims_from_token, ProfileParams, TokenResponse},
            lobby::CreateLobbyResponse,
            ClientGameMessage, ClientMessage, JoinLobbyDto, ServerMessage,
        },
        models::{Card, MAX_PLAYER_COUNT},
    };
    use reqwest::Client;
    use tokio::{net::TcpStream, task};
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    const URL: &str = "http://localhost:3000/";

    #[tokio::test]
    async fn test_example() {
        task::spawn(oh_hell::start_app());

        for p in 2..=MAX_PLAYER_COUNT {
            let mut client = reqwest::Client::new();

            let tokens = get_players(&mut client, p).await;

            let mut player_data = join_lobby(&mut client, tokens).await;

            ready(&mut player_data).await;

            'game: loop {
                get_decks(&mut player_data).await;

                play_set(&mut player_data).await;

                for p in player_data.values_mut() {
                    if assert_game_or_set_ended(&mut p.connection).await {
                        break 'game;
                    }
                }
            }
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
            ServerMessage::SetEnded { lifes } => {
                println!("Asserted game msg {:?}", ServerMessage::SetEnded { lifes });
                false
            }
            ServerMessage::GameEnded { winner, lifes } => {
                let expected_life_values = if winner.is_some() { 1 } else { 0 };

                assert!(lifes.len() == expected_life_values);

                let msg = ServerMessage::GameEnded { lifes, winner };

                println!("Asserted game msg {:?}", msg);

                true
            }
            msg => panic!("Expected Set or Game end | {msg:?}"),
        }
    }

    async fn play_set(players: &mut HashMap<String, PlayerData>) {
        let rounds_count = players.values().next().unwrap().deck.len();

        bidding(players, rounds_count).await;

        for i in 0..rounds_count {
            play_round(players, i == rounds_count - 1).await;
        }
    }

    type Deck = Vec<Card>;

    async fn play_round(players: &mut HashMap<String, PlayerData>, last: bool) {
        for _ in 0..players.len() {
            play_turn(players).await;
        }

        if !last {
            for p in players.values_mut() {
                assert_game_msg(&mut p.connection, validate_round_ended).await;
            }
        }
    }

    async fn play_turn(players: &mut HashMap<String, PlayerData>) {
        let first_connection = players.values_mut().next().unwrap();

        let next = get_next_turn_player(&mut first_connection.connection).await;

        for p in players.values_mut().skip(1) {
            get_next_turn_player(&mut p.connection).await;
        }

        let next = players.get_mut(&next).unwrap();

        let msg = ClientGameMessage::PlayTurn {
            card: next.deck.swap_remove(0),
        };

        send_msg(&mut next.connection, msg).await;

        for p in players.values_mut() {
            assert_game_msg(&mut p.connection, validate_turn_played).await;
        }
    }

    async fn bidding(players: &mut HashMap<String, PlayerData>, bid: usize) {
        for _ in 0..players.len() {
            bid_turn(players, bid).await;
        }
    }

    async fn bid_turn(players: &mut HashMap<String, PlayerData>, bid: usize) {
        let data = players.values_mut().next().unwrap();

        let next = get_next_bidding_player(&mut data.connection).await;

        for p in players.values_mut().skip(1) {
            get_next_bidding_player(&mut p.connection).await;
        }

        let next = players.get_mut(&next).unwrap();

        send_msg(&mut next.connection, ClientGameMessage::PutBid { bid }).await;

        for p in players.values_mut() {
            assert_game_msg(&mut p.connection, validate_player_bidded).await;
        }
    }

    struct PlayerData {
        connection: WebSocket,
        deck: Deck,
    }

    async fn get_decks(players: &mut HashMap<String, PlayerData>) {
        for p in players.values_mut() {
            assert_game_msg(&mut p.connection, validate_set_start).await;
        }

        for p in players.values_mut() {
            p.deck = get_deck(&mut p.connection).await;
        }
    }

    async fn join_lobby(client: &mut Client, tokens: Vec<String>) -> HashMap<String, PlayerData> {
        let lobby_id = create_lobby(client, &tokens[0]).await;

        for (i, p) in tokens.iter().enumerate() {
            let lobby = join_lobby_http(client, p, &lobby_id).await;
            assert!(lobby.players.len() == i + 1);
        }

        let mut connections = HashMap::new();

        for p in tokens {
            let claims = get_claims_from_token(&p).await.unwrap();

            let data = PlayerData {
                connection: connect_ws(p.clone()).await,
                deck: Vec::new(),
            };

            connections.insert(claims.id(), data);
        }

        connections
    }

    async fn ready(players: &mut HashMap<String, PlayerData>) {
        let msg = ClientGameMessage::PlayerStatusChange { ready: true };

        for p in players.values_mut() {
            send_msg(&mut p.connection, msg).await;
        }

        for _ in 0..players.len() {
            for p in players.values_mut() {
                assert_game_msg(&mut p.connection, validate_player_status_change).await;
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
        matches!(m, ServerMessage::SetStart { upcard: _ })
    }

    async fn get_next_turn_player(stream: &mut WebSocket) -> String {
        match assert_game_msg(stream, validate_player_turn).await {
            ServerMessage::PlayerTurn { player_id } => player_id,
            _ => panic!("Should be a PlayerTurn message"),
        }
    }

    async fn get_next_bidding_player(stream: &mut WebSocket) -> String {
        match assert_game_msg(stream, validate_bidding_turn).await {
            ServerMessage::PlayerBiddingTurn {
                player_id,
                possible_bids: _,
            } => player_id,
            _ => panic!("Should be a PlayerTurn message"),
        }
    }

    async fn get_deck(stream: &mut WebSocket) -> Deck {
        match assert_game_msg(stream, |m| matches!(m, ServerMessage::PlayerDeck(_))).await {
            ServerMessage::PlayerDeck(c) => c,
            _ => panic!("Should be a PlayerDeck message"),
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
