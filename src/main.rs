#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::multiple_crate_versions,
    clippy::too_many_lines,
    clippy::redundant_pub_crate
)]

use anyhow::Result;
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::delete;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Json, Router,
};
use dotenvy::dotenv;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use optional_default::OptionalDefault;
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::{Sqlite, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{broadcast, watch, Mutex};
use tokio::time::{self, Duration};
use ts_rs::TS;

static INDEX_HTML: &str = "index.html";

#[derive(Embed)]
#[folder = "client/dist/"]
struct Assets;

#[derive(sqlx::FromRow, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PartialRoomState {
    room_id: String,
    content: Option<String>,
}

/// State of a room
#[derive(Debug)]
struct RoomState {
    users: Mutex<HashSet<String>>,
    tx: broadcast::Sender<String>,
    content_tx: watch::Sender<String>,
    content_rx: watch::Receiver<String>,
}

impl RoomState {
    fn new(room_id: String, db: &Option<SqlitePool>) -> Self {
        let (content_tx, content_rx) = watch::channel(String::new());
        let content_rx_clone = content_rx.clone();

        if let Some(db) = db {
            let db = db.clone();

            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_secs(2));
                let mut last_content = content_rx.borrow().clone();
                loop {
                    interval.tick().await;
                    if *content_rx.borrow() != last_content {
                        last_content.clone_from(&content_rx.borrow());
                        if let Err(e) =
                            update_room_content(&db, room_id.clone(), last_content.clone()).await
                        {
                            eprintln!("Failed to update room content in database: {e}");
                        }
                    }
                }
            });
        }

        Self {
            users: Mutex::new(HashSet::new()),
            tx: broadcast::channel(100).0,
            content_tx,
            content_rx: content_rx_clone,
        }
    }
}

/// State of the app
struct AppState {
    rooms: Mutex<HashMap<String, RoomState>>,
    db: Option<SqlitePool>,
}

fn app(app_state: Arc<AppState>) -> Router {
    let rooms = Router::new()
        .route("/", get(get_rooms))
        .route("/:room_id", delete(remove_room));

    let api = Router::new().nest("/rooms", rooms);

    Router::new()
        .route("/ws", get(handler))
        .nest("/api", api)
        .with_state(app_state)
        .fallback(static_handler)
}

#[tokio::main]
async fn main() -> Result<()> {
    if dotenv().is_err() {
        eprintln!("No .env file found");
    }

    let port = std::env::var("PORT")
        .map(|val| val.parse::<u16>())
        .unwrap_or(Ok(3001))?; // Default port is 3001

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let db = if let Ok(db_url) = std::env::var("DATABASE_URL") {
        println!("Database URL: {db_url}");

        let db_url = db_url.as_str();

        if Sqlite::database_exists(db_url).await.unwrap_or(false) {
            println!("Database already exists");
        } else {
            println!("Creating database {db_url}");
            match Sqlite::create_database(db_url).await {
                Ok(()) => println!("Create db success"),
                Err(error) => {
                    eprintln!("error: {error}");
                    std::process::exit(1);
                }
            }
        }

        let db = SqlitePool::connect(db_url).await?;

        // Migrate the database
        let migration_results = sqlx::migrate!().run(&db).await;
        match migration_results {
            Ok(()) => println!("Migration success"),
            Err(error) => {
                eprintln!("error: {error}");
                std::process::exit(1);
            }
        }
        println!("migration: {migration_results:?}");

        Some(db)
    } else {
        println!("No DATABASE_URL found in .env file, disabling database support");
        None
    };

    // Restore rooms from the database
    let mut rooms = HashMap::new();

    {
        if let Some(ok_db) = &db {
            for room in sqlx::query!("SELECT * FROM rooms").fetch_all(ok_db).await? {
                println!(
                    "Restoring room: {} with content: {}",
                    room.room_id, room.content
                );
                let room_state = RoomState::new(room.room_id.clone(), &db);
                room_state.content_tx.send(room.content.clone())?;
                rooms.insert(room.room_id, room_state);
            }
        }

        // If no "general" room is found, create one
        let default_room = "general";
        if !rooms.contains_key(default_room) {
            rooms.insert(
                default_room.to_string(),
                RoomState::new(default_room.to_string(), &db),
            );
        }
    }

    let app_state = Arc::new(AppState {
        rooms: Mutex::new(rooms),
        db,
    });

    let app = app(app_state);

    let listener = tokio::net::TcpListener::bind(addr.to_string()).await?;

    println!("listening on {}", listener.local_addr()?);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

/// Handler
async fn handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Send a pong frame in response to a ping frame
async fn send_pong_frame(sender: &Arc<Mutex<SplitSink<WebSocket, Message>>>, b: Vec<u8>) {
    if b[0] == 0x9 {
        let _ = sender.lock().await.send(Message::Binary(vec![0xA])).await;
    }
}

/// Update the room content
async fn update_room_content(db: &SqlitePool, room_id: String, new_content: String) -> Result<()> {
    println!("Updating room content : {new_content}");
    sqlx::query!(
        r#"
        INSERT OR REPLACE INTO rooms (room_id, content) VALUES (?, ?)
        "#,
        room_id,
        new_content
    )
    .execute(db)
    .await?;

    Ok(())
}

#[derive(TS, Serialize, Debug)]
enum SocketMessageType {
    #[serde(rename = "join")]
    Join,
    #[serde(rename = "leave")]
    Leave,
    #[serde(rename = "message")]
    Message,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "update-rooms-list")]
    UpdateRoomsList,
}

#[derive(TS, Serialize, Debug, OptionalDefault)]
#[ts(export)]
struct SocketMessage {
    #[serde(rename = "type")]
    message_type: SocketMessageType,
    #[optional(default = None)]
    #[ts(type = "string | undefined")]
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[optional(default = String::new())]
    #[ts(type = "string | undefined")]
    #[serde(skip_serializing_if = "String::is_empty")]
    username: String,
}

/// Handle sending and receiving messages
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender)); // Wrap the sender in an Arc<Mutex<>>
    let sender_recv_task = sender.clone(); // Clone the Arc for the recv_messages task

    let mut username = String::new();
    let mut channel = String::new();
    let content;
    let mut tx = None::<broadcast::Sender<String>>;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Binary(msg) = msg {
            send_pong_frame(&sender, msg).await;
            continue;
        } else if let Message::Text(text) = msg {
            #[derive(Deserialize)]
            struct Connect {
                username: String,
                channel: String,
            }

            println!("Name: {text}");

            let connect: Connect = match serde_json::from_str(&text) {
                Ok(connect) => connect,
                Err(err) => {
                    println!("{}", &text);
                    eprintln!("{err}");
                    let _ = sender_recv_task
                        .lock()
                        .await
                        .send(Message::Text(
                            json!(SocketMessage! {
                                message_type: SocketMessageType::Error,
                                value: Some("Invalid JSON".to_string()),
                            })
                            .to_string(),
                        ))
                        .await;
                    break;
                }
            };

            {
                channel.clone_from(&connect.channel);

                let mut rooms = state.rooms.lock().await;
                let room = rooms
                    .entry(connect.channel.clone())
                    .or_insert_with(|| RoomState::new(connect.channel.clone(), &state.db));

                tx = Some(room.tx.clone());

                // Add the user to the room, if they are not already in it
                room.users.lock().await.insert(connect.username.clone());

                // A user can join the room multiple times, so we need to update the username
                // Anyone can take the username of another user, but we don't care
                username.clone_from(&connect.username);
                content = room.content_rx.borrow().clone();

                drop(rooms);
            }

            if tx.is_some() && !username.is_empty() {
                {
                    let rooms = state.rooms.lock().await;
                    for (room_name, room_state) in rooms.iter() {
                        if room_name != &channel {
                            let _ = room_state.tx.send(
                                json!(SocketMessage! {
                                    message_type: SocketMessageType::UpdateRoomsList,
                                })
                                .to_string(),
                            );
                        }
                    }
                }

                // Send the user the current room content
                let _ = sender_recv_task
                    .lock()
                    .await
                    .send(Message::Text(
                        json!(SocketMessage {
                            message_type: SocketMessageType::Message,
                            value: Some(content),
                            username: "Server".to_string(),
                        })
                        .to_string(),
                    ))
                    .await;

                break;
            }
            println!("Failed to connect to room!");
            let _ = sender_recv_task
                .lock()
                .await
                .send(Message::Text(
                    json!(SocketMessage! {
                        message_type: SocketMessageType::Error,
                        value: Some("Failed to connect to room!".to_string()),
                    })
                    .to_string(),
                ))
                .await;

            return;
        }
    }

    let tx = tx;
    let Some(tx) = tx else {
        println!("Failed to connect to room!");
        return;
    };

    let mut rx = tx.subscribe();

    let _ = tx.send(
        json!(SocketMessage! {
            message_type: SocketMessageType::Join,
            username: username.clone(),
        })
        .to_string(),
    );

    let mut recv_messages = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            println!("Received: {msg}");
            if sender_recv_task
                .lock()
                .await
                .send(Message::Text(msg))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut send_messages = {
        let tx = tx.clone();
        let name = username.clone();
        let channel = channel.clone();
        let state = state.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Binary(b) = msg {
                    send_pong_frame(&sender, b).await;
                    continue;
                } else if let Message::Text(text) = msg {
                    println!("{name}: {text}");

                    // Update the room content
                    let rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get(&channel) {
                        // ignore errors but log them
                        room.content_tx.send(text.clone()).unwrap_or_else(|err| {
                            eprintln!("Failed to send message to room: {err}")
                        });
                    }
                    drop(rooms);

                    let _ = tx.send(
                        json!(SocketMessage {
                            message_type: SocketMessageType::Message,
                            value: Some(text),
                            username: name.clone(),
                        })
                        .to_string(),
                    );
                }
            }
        })
    };

    tokio::select! {
        _ = &mut send_messages => recv_messages.abort(),
        _ = &mut recv_messages => send_messages.abort(),
    }

    let _ = tx.send(
        json!(SocketMessage! {
            message_type: SocketMessageType::Leave,
            username: username.clone(),
        })
        .to_string(),
    );

    let mut rooms = state.rooms.lock().await;
    let room = rooms.get_mut(&channel);

    if let Some(room) = room {
        room.users.lock().await.remove(&username);
    } else {
        eprintln!("Failed to remove user from room!");
    }

    drop(rooms);
}

/// Custom error type that can be converted into a JSON response
#[derive(Debug, Serialize, Deserialize)]
struct CustomError {
    message: String,
}

impl IntoResponse for CustomError {
    fn into_response(self) -> Response {
        // Convert the custom error into a JSON response with a specific status code
        let body = Json(json!({ "error": self.message }));
        (StatusCode::BAD_REQUEST, body).into_response()
    }
}

/// Remove a room by id
async fn remove_room(
    State(state): State<Arc<AppState>>,
    room: axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, CustomError> {
    // If general, forbid removal
    if room.0 == "general" {
        return Err(CustomError {
            message: "Cannot remove the default room.".to_owned(),
        });
    }

    let mut rooms = state.rooms.lock().await;

    // If already removed, fail silently
    if !rooms.contains_key(&room.0) {
        println!("Room already removed.");
        return Ok(Json(json!({ "message": "Room already removed." })));
    }

    // If only 1 room exists, don't remove it, return an error
    if rooms.len() == 1 {
        return Err(CustomError {
            message: "Cannot remove the last room.".to_owned(),
        });
    }

    // If the room has more than 1 user, don't remove it, return an error
    if rooms.get(&room.0).unwrap().users.lock().await.len() > 1 {
        return Err(CustomError {
            message: "Room has more than 1 user.".to_owned(),
        });
    }

    rooms.remove(&room.0);

    // Update database
    if let Some(db) = &state.db {
        if let Err(e) = sqlx::query!("DELETE FROM rooms WHERE room_id = $1", room.0)
            .execute(db)
            .await
        {
            eprintln!("Failed to remove room from database: {e:?}");
            return Err(CustomError {
                message: "Failed to remove room from database.".to_owned(),
            });
        }
    }

    // Notify all users that the room has been removed
    for (_, room_state) in rooms.iter() {
        let _ = room_state.tx.send(
            json!(SocketMessage! {
                message_type: SocketMessageType::UpdateRoomsList,
            })
            .to_string(),
        );
    }

    drop(rooms);

    Ok(Json(json!({
        "type": "success",
        "value": "Room removed."
    })))
}

/// Room
#[derive(TS, Serialize, Deserialize)]
#[ts(export)]
struct Room {
    id: String,
    users: Vec<String>,
}

/// Get a list of all rooms
async fn get_rooms(State(state): State<Arc<AppState>>) -> Json<Vec<Room>> {
    let rooms = state.rooms.lock().await;
    let mut room_list = Vec::new();

    for (id, room) in rooms.iter() {
        let users = room.users.lock().await;
        room_list.push(Room {
            id: id.clone(),
            users: users.iter().cloned().collect(),
        });
    }

    drop(rooms);
    Json(room_list)
}

#[cfg(not(debug_assertions))]
const CACHE_EXTENTIONS: [&str; 9] = [
    ".css", ".js", ".wasm", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg",
];

/// Static file handler with conditional caching
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == INDEX_HTML {
        return index_html();
    }

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();

        #[cfg(debug_assertions)]
        {
            // Debug build: no caching, original behavior
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }

        #[cfg(not(debug_assertions))]
        {
            // Release build: add cache-control header for static assets
            let cache_header_value = if CACHE_EXTENTIONS.iter().any(|ext| path.ends_with(ext)) {
                // Cache assets for 1 year
                "public, max-age=31536000"
            } else {
                // No caching for non-static assets or HTML
                "no-cache, no-store, must-revalidate"
            };

            (
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::CACHE_CONTROL, cache_header_value),
                ],
                content.data,
            )
                .into_response()
        }
    } else {
        if path.contains('.') {
            return not_found();
        }

        index_html()
    }
}

/// Index HTML handler
fn index_html() -> Response {
    match Assets::get(INDEX_HTML) {
        Some(content) => Html(content.data).into_response(),
        None => not_found(),
    }
}

/// 404 handler
fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "404").into_response()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

// Test functions
#[cfg(test)]
mod tests {
    use axum::Router;
    use futures::{SinkExt, StreamExt};
    use serde_json::json;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;

    use crate::{app, get_rooms, handler, remove_room, AppState, Room, RoomState};
    use axum::routing::{delete, get};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio_tungstenite::tungstenite::Message;

    async fn setup_test_server() -> (SocketAddr, Router) {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = TcpListener::bind(addr).await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Create test app state similar to main()
        let app_state = Arc::new(AppState {
            rooms: Mutex::new({
                let mut rooms = HashMap::<String, RoomState>::new();
                rooms.insert(
                    "general".to_string(),
                    RoomState::new("general".to_string(), &None),
                );
                rooms
            }),
            db: None, // Using in-memory state for tests
        });

        let app = app(app_state);

        let app_clone = app.clone();
        tokio::spawn(async move {
            axum::serve(listener, app_clone).await.unwrap();
        });

        (server_addr, app)
    }

    #[tokio::test]
    async fn test_static_file_handling() {
        let (addr, _) = setup_test_server().await;
        let client = reqwest::Client::new();

        // Test root path
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();
        assert_eq!(response.status(), 200);

        // Test index.html
        let response = client
            .get(format!("http://{addr}/index.html"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        // Test favicon.ico
        let response = client
            .get(format!("http://{addr}/favicon.ico"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        // Test non-existent file
        let response = client
            .get(format!("http://{addr}/nonexistent.js"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_websocket_chat_flow() {
        let (addr, _app) = setup_test_server().await;

        // Connect two test users
        let ws_uri = format!("ws://{addr}/ws");
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();
        let (mut ws2, _) = connect_async(&ws_uri).await.unwrap();

        // User 1 joins general channel
        let join_msg1 = json!({
            "type": "join",
            "channel": "general",
            "username": "alice"
        })
        .to_string();
        ws1.send(Message::Text(join_msg1)).await.unwrap();

        // User 2 joins general channel
        let join_msg2 = json!({
            "type": "join",
            "channel": "general",
            "username": "bob"
        })
        .to_string();
        ws2.send(Message::Text(join_msg2)).await.unwrap();

        // Wait for initial messages on both connections
        for _ in 0..2 {
            if let Some(msg) = ws1.next().await {
                let _ = msg.unwrap().into_text().unwrap();
            }
        }
        for _ in 0..2 {
            if let Some(msg) = ws2.next().await {
                let _ = msg.unwrap().into_text().unwrap();
            }
        }

        // Small delay to ensure all messages are processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // User 1 sends a message
        let chat_msg = json!({
            "type": "message",
            "channel": "general",
            "username": "alice",
            "content": "Hello, Bob!"
        })
        .to_string();
        ws1.send(Message::Text(chat_msg)).await.unwrap();

        // Verify Bob receives Alice's message
        if let Some(msg) = ws2.next().await {
            let received = msg.unwrap().into_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
            let inner_msg: serde_json::Value =
                serde_json::from_str(parsed["value"].as_str().unwrap()).unwrap();
            assert_eq!(inner_msg["username"].as_str().unwrap(), "alice");
            assert_eq!(inner_msg["content"].as_str().unwrap(), "Hello, Bob!");
        }
    }

    #[tokio::test]
    async fn test_connection_edge_cases() {
        let (addr, _) = setup_test_server().await;
        let ws_uri = format!("ws://{addr}/ws");

        // Test connection with missing username
        let (mut ws, _) = connect_async(&ws_uri).await.unwrap();
        let invalid_join = json!({
            "channel": "test"
        })
        .to_string();

        ws.send(Message::Text(invalid_join)).await.unwrap();

        if let Some(msg) = ws.next().await {
            let error_msg = msg.unwrap().into_text().unwrap();
            assert!(error_msg.contains("error"));
        }
    }

    #[tokio::test]
    async fn test_room_management() {
        let (addr, _) = setup_test_server().await;
        let client = reqwest::Client::new();
        let base_url = format!("http://{addr}");

        // Get initial rooms (should include general)
        let response = client
            .get(format!("{base_url}/api/rooms"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let rooms: Vec<Room> = response.json().await.unwrap();
        assert!(rooms.iter().any(|r| r.id == "general"));

        // Try to delete general room (should fail)
        let response = client
            .delete(format!("{base_url}/api/rooms/general"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);

        // Connect user to a new room
        let ws_uri = format!("ws://{addr}/ws");
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();

        let join_msg = json!({
            "username": "carol",
            "channel": "new-room"
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        // Verify new room exists
        let response = client
            .get(format!("{base_url}/api/rooms"))
            .send()
            .await
            .unwrap();
        let rooms: Vec<Room> = response.json().await.unwrap();
        assert!(rooms.iter().any(|r| r.id == "new-room"));
    }

    #[tokio::test]
    async fn test_room_removal_edge_cases() {
        let (addr, _) = setup_test_server().await;
        let client = reqwest::Client::new();

        // Test removing non-existent room
        let response = client
            .delete(format!("http://{addr}/api/rooms/nonexistent"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        // Test removing room with active users
        // First create and populate a room
        let ws_uri = format!("ws://{addr}/ws");
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();
        let (mut ws2, _) = connect_async(&ws_uri).await.unwrap();

        // Join two users to the same room
        let join_msg = json!({
            "username": "test_user_1",
            "channel": "test_room"
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        let join_msg = json!({
            "username": "test_user_2",
            "channel": "test_room"
        })
        .to_string();
        ws2.send(Message::Text(join_msg)).await.unwrap();

        // Try to remove the room with active users
        let response = client
            .delete(format!("http://{addr}/api/rooms/test_room"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_concurrent_chat() {
        let (addr, _) = setup_test_server().await;
        let mut connections = HashMap::new();

        // Connect multiple users
        let ws_uri = format!("ws://{addr}/ws");
        for i in 0..5 {
            let (ws, _) = connect_async(&ws_uri).await.unwrap();
            connections.insert(format!("user{i}"), ws);
        }

        // Join all users to the same room
        for (username, ref mut ws) in &mut connections {
            let join_msg = json!({
                "type": "join",
                "channel": "test_room",
                "username": username,
            })
            .to_string();
            ws.send(Message::Text(join_msg)).await.unwrap();
        }

        // Have each user send a message
        for (username, ref mut ws) in &mut connections {
            let msg = json!({
                "type": "message",
                "channel": "test_room",
                "username": username,
                "content": format!("Hello from {}", username),
            })
            .to_string();
            ws.send(Message::Text(msg)).await.unwrap();
        }

        // Verify each user receives messages from others
        for ref mut ws in connections.values_mut() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Some(msg) = ws.next().await {
                let msg = msg.unwrap();
                assert!(msg.is_text());
            }
        }
    }

    #[tokio::test]
    async fn test_reconnection_scenario() {
        let (addr, _) = setup_test_server().await;
        let ws_uri = format!("ws://{addr}/ws");

        // Initial connection
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();

        // Join and send a message
        let join_msg = json!({
            "username": "disconnector",
            "channel": "test-room"
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        let msg = json!({
            "type": "message",
            "value": "Initial message",
            "username": "disconnector"
        })
        .to_string();
        ws1.send(Message::Text(msg)).await.unwrap();

        // Simulate disconnection
        drop(ws1);

        // Reconnect
        let (mut ws2, _) = connect_async(&ws_uri).await.unwrap();

        // Rejoin same room
        let rejoin_msg = json!({
            "username": "disconnector",
            "channel": "test-room"
        })
        .to_string();
        ws2.send(Message::Text(rejoin_msg)).await.unwrap();

        // Verify we receive the current room state
        if let Some(msg) = ws2.next().await {
            let received = msg.unwrap().into_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
            assert_eq!(parsed["type"], "message");
            assert_eq!(parsed["username"], "Server");
        }
    }

    #[tokio::test]
    async fn test_error_handling() {
        let (addr, _) = setup_test_server().await;
        let ws_uri = format!("ws://{addr}/ws");
        let (mut ws, _) = connect_async(&ws_uri).await.unwrap();

        // Send invalid JSON
        ws.send(Message::Text("invalid json".to_string()))
            .await
            .unwrap();

        // Should receive error message
        if let Some(msg) = ws.next().await {
            let msg = msg.unwrap();
            assert!(msg.is_text());
        }

        // Try to delete non-existent room
        let client = reqwest::Client::new();
        let response = client
            .delete(format!("http://{addr}/api/rooms/nonexistent"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200); // Silently fails as specified
    }

    // Keep existing imports and add:
    use sqlx::SqlitePool;

    async fn setup_test_server_with_db() -> (SocketAddr, Router, SqlitePool) {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = TcpListener::bind(addr).await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // Create in-memory SQLite database
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();

        // Run migrations
        sqlx::migrate!().run(&db).await.unwrap();

        // Initialize the general room in the database first
        sqlx::query!(
            "INSERT INTO rooms (room_id, content) VALUES (?, ?)",
            "general",
            ""
        )
        .execute(&db)
        .await
        .unwrap();

        let app_state = Arc::new(AppState {
            rooms: Mutex::new({
                let mut rooms = HashMap::<String, RoomState>::new();
                rooms.insert(
                    "general".to_string(),
                    RoomState::new("general".to_string(), &Some(db.clone())),
                );
                rooms
            }),
            db: Some(db.clone()),
        });

        let app = Router::new()
            .route("/ws", get(handler))
            .route("/api/rooms", get(get_rooms))
            .route("/api/rooms/:id", delete(remove_room))
            .with_state(app_state);

        let app_clone = app.clone();
        tokio::spawn(async move {
            axum::serve(listener, app_clone).await.unwrap();
        });

        (server_addr, app, db)
    }

    #[tokio::test]
    async fn test_database_persistence() {
        let (addr, _, db) = setup_test_server_with_db().await;
        let ws_uri = format!("ws://{addr}/ws");

        // Connect and send messages
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();

        // Join general room
        let join_msg = json!({
            "username": "db_test_user",
            "channel": "general"
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        // Wait for initial messages
        let msg = ws1.next().await.unwrap();
        let _ = msg.unwrap().into_text().unwrap();

        // Send a test message
        let test_content = "Test message for database persistence";
        ws1.send(Message::Text(test_content.to_string()))
            .await
            .unwrap();

        // Wait a bit for the database update interval
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Verify content was saved to database
        let room = sqlx::query!("SELECT * FROM rooms WHERE room_id = ?", "general")
            .fetch_one(&db)
            .await
            .unwrap();

        assert_eq!(room.content, test_content);
    }

    #[tokio::test]
    async fn test_room_persistence() {
        let (addr, _, db) = setup_test_server_with_db().await;
        let ws_uri = format!("ws://{addr}/ws");

        // Create a new room by connecting to it
        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();
        let new_room = "persistent_test_room";

        let join_msg = json!({
            "username": "room_creator",
            "channel": new_room
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        // Send content to the new room
        let test_content = "Content for persistent room";

        // Wait for initial messages
        let msg = ws1.next().await.unwrap();
        let _ = msg.unwrap().into_text().unwrap();

        ws1.send(Message::Text(test_content.to_string()))
            .await
            .unwrap();

        // Wait for database update
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Verify room exists in database
        let room = sqlx::query!("SELECT * FROM rooms WHERE room_id = ?", new_room)
            .fetch_one(&db)
            .await
            .unwrap();

        assert_eq!(room.content, test_content);

        // Test room deletion
        let client = reqwest::Client::new();
        let response = client
            .delete(format!("http://{addr}/api/rooms/{new_room}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        // Verify room was deleted from database
        let result = sqlx::query!("SELECT * FROM rooms WHERE room_id = ?", new_room)
            .fetch_optional(&db)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_multiple_room_persistence() {
        let (addr, _, db) = setup_test_server_with_db().await;
        let ws_uri = format!("ws://{addr}/ws");

        // Create multiple rooms and send messages
        let room_data = vec![
            ("room1", "content1"),
            ("room2", "content2"),
            ("room3", "content3"),
        ];

        for (room_name, content) in &room_data {
            let (mut ws, _) = connect_async(&ws_uri).await.unwrap();

            let join_msg = json!({
                "username": "multi_room_test",
                "channel": room_name
            })
            .to_string();
            ws.send(Message::Text(join_msg)).await.unwrap();

            // Wait for initial messages
            let msg = ws.next().await.unwrap();
            let _ = msg.unwrap().into_text().unwrap();

            ws.send(Message::Text((*content).to_string()))
                .await
                .unwrap();

            // Wait for database update and close connection
            tokio::time::sleep(Duration::from_secs(2)).await;
            drop(ws);

            // Wait a bit before creating next room
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Wait for all database updates to complete
        tokio::time::sleep(Duration::from_secs(4)).await;

        // Verify all rooms are in database with correct content
        for (room_name, expected_content) in &room_data {
            let room = sqlx::query!("SELECT * FROM rooms WHERE room_id = ?", room_name)
                .fetch_one(&db)
                .await
                .unwrap();

            assert_eq!(room.content, *expected_content);
        }

        // Verify total room count
        let count = sqlx::query!("SELECT COUNT(*) as count FROM rooms")
            .fetch_one(&db)
            .await
            .unwrap()
            .count;

        // Count should be room_data.len() + 1 (including general room)
        let count_usize = usize::try_from(count).expect("Failed to convert count to usize");
        assert_eq!(count_usize, room_data.len() + 1);
    }

    #[tokio::test]
    async fn test_content_update() {
        let (addr, _, db) = setup_test_server_with_db().await;
        let ws_uri = format!("ws://{addr}/ws");

        let (mut ws1, _) = connect_async(&ws_uri).await.unwrap();

        // Join test room
        let room_name = "update_test_room";
        let join_msg = json!({
            "username": "content_updater",
            "channel": room_name
        })
        .to_string();
        ws1.send(Message::Text(join_msg)).await.unwrap();

        // Wait for initial messages
        let msg = ws1.next().await.unwrap();
        let _ = msg.unwrap().into_text().unwrap();

        // Send multiple messages and verify database updates
        let messages = vec!["First message", "Second message", "Third message"];

        for message in &messages {
            ws1.send(Message::Text((*message).to_string()))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_secs(6)).await;

            let room = sqlx::query!("SELECT * FROM rooms WHERE room_id = ?", room_name)
                .fetch_one(&db)
                .await
                .unwrap();

            assert_eq!(room.content, *message);
        }
    }
}
