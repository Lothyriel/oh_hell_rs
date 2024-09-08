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

        // TODO make this test work for 2-10 players

        let p1_token = login(&mut client).await;
        let p2_token = login(&mut client).await;

        let (mut p1, mut p2) = join_lobby(&mut client, p1_token, p2_token).await;

        ready(&mut p1, &mut p2).await;

        let mut cards_count = 1;

        loop {
            let (p1_deck, p2_deck) = decks(&mut p1, &mut p2, cards_count).await;

            cards_count += 1;

            play_set(&mut p1, &mut p2, &p1_deck, &p2_deck).await;

            if assert_game_or_set_ended(&mut p1).await & assert_game_or_set_ended(&mut p2).await {
                break;
            }
        }
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
            _ => panic!("Expected Set or Game end"),
        }
    }

    async fn play_set(p1: &mut WebSocket, p2: &mut WebSocket, p1_deck: &Deck, p2_deck: &Deck) {
        let rounds_count = p1_deck.len();

        bidding(p1, p2, rounds_count).await;

        for i in 0..rounds_count {
            play_round(p1, p2, p1_deck, p2_deck, i == rounds_count - 1).await;
        }
    }

    type Deck = Vec<Card>;

    async fn play_round(
        p1: &mut WebSocket,
        p2: &mut WebSocket,
        p1_deck: &Deck,
        p2_deck: &Deck,
        last: bool,
    ) {
        assert_game_msg(p1, validate_player_turn).await;
        assert_game_msg(p2, validate_player_turn).await;

        let msg = ClientGameMessage::PlayTurn { card: p1_deck[0] };
        send_msg(p1, msg).await;

        assert_game_msg(p1, validate_turn_played).await;
        assert_game_msg(p2, validate_turn_played).await;

        assert_game_msg(p1, validate_player_turn).await;
        assert_game_msg(p2, validate_player_turn).await;

        let msg = ClientGameMessage::PlayTurn { card: p2_deck[0] };
        send_msg(p2, msg).await;

        assert_game_msg(p1, validate_turn_played).await;
        assert_game_msg(p2, validate_turn_played).await;

        if !last {
            assert_game_msg(p1, validate_round_ended).await;
            assert_game_msg(p2, validate_round_ended).await;
        }
    }

    async fn bidding(p1: &mut WebSocket, p2: &mut WebSocket, bid: usize) {
        bid_turn(bid, p1, p2).await;
        bid_turn(bid, p1, p2).await;
    }

    async fn bid_turn(bid: usize, p1: &mut WebSocket, p2: &mut WebSocket) {
        let msg = ClientGameMessage::PutBid { bid };

        assert_game_msg(p1, validate_bidding_turn).await;
        assert_game_msg(p2, validate_bidding_turn).await;

        send_msg(p1, msg).await;

        assert_game_msg(p1, validate_player_bidded).await;
        assert_game_msg(p2, validate_player_bidded).await;
    }

    async fn decks(p1: &mut WebSocket, p2: &mut WebSocket, cards_count: usize) -> (Deck, Deck) {
        assert_game_msg(p1, validate_set_start).await;
        assert_game_msg(p2, validate_set_start).await;

        let p1_deck = get_deck(p1).await;
        let p2_deck = get_deck(p2).await;
        assert!(p1_deck.len() == cards_count);
        assert!(p2_deck.len() == cards_count);

        (p1_deck, p2_deck)
    }

    async fn join_lobby(client: &mut Client, p1: String, p2: String) -> (WebSocket, WebSocket) {
        let lobby_id = create_lobby(client, &p1).await;

        let lobby = join_lobby_http(client, &p1, lobby_id.clone()).await;
        assert!(lobby.players.len() == 1);

        let lobby = join_lobby_http(client, &p2, lobby_id).await;
        assert!(lobby.players.len() == 2);

        let p1 = connect_ws(p1).await;
        let p2 = connect_ws(p2).await;

        (p1, p2)
    }

    async fn ready(p1: &mut WebSocket, p2: &mut WebSocket) {
        let msg = ClientGameMessage::PlayerStatusChange { ready: true };

        send_msg(p1, msg).await;
        send_msg(p2, msg).await;

        assert_game_msg(p1, validate_player_status_change).await;
        assert_game_msg(p1, validate_player_status_change).await;
        assert_game_msg(p2, validate_player_status_change).await;
        assert_game_msg(p2, validate_player_status_change).await;
    }

    fn validate_round_ended(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::RoundEnded(_))
    }

    fn validate_turn_played(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::TurnPlayed { turn: _ })
    }

    fn validate_player_turn(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::PlayerTurn { player_id: _ })
    }

    fn validate_bidding_turn(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::PlayerBiddingTurn { player_id: _ })
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

    async fn join_lobby_http(client: &mut Client, token: &str, lobby_id: String) -> JoinLobbyDto {
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

    async fn login(client: &mut Client) -> String {
        let params = ProfileParams {
            picture: "picture.jpg".to_string(),
            nickname: "JX".to_string(),
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
