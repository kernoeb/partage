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
                Err(error) => panic!("error: {error}"),
            }
        }

        let db = SqlitePool::connect(db_url).await?;

        // Migrate the database
        let migration_results = sqlx::migrate!().run(&db).await;
        match migration_results {
            Ok(()) => println!("Migration success"),
            Err(error) => {
                panic!("error: {error}");
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

    let app = Router::new()
        .route("/ws", get(handler))
        .route("/rooms", get(get_rooms))
        .route("/rooms/:id", delete(remove_room))
        .with_state(app_state)
        .fallback(static_handler);

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
    let (mut sender, mut receiver) = socket.split();
    let mut username = String::new();
    let mut channel = String::new();
    let content;
    let mut tx = None::<broadcast::Sender<String>>;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(name) = msg {
            #[derive(Deserialize)]
            struct Connect {
                username: String,
                channel: String,
            }

            println!("Name: {name}");

            let connect: Connect = match serde_json::from_str(&name) {
                Ok(connect) => connect,
                Err(err) => {
                    println!("{}", &name);
                    eprintln!("{err}");
                    let _ = sender
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
                let _ = sender
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
            let _ = sender
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
            if sender.send(Message::Text(msg)).await.is_err() {
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
            while let Some(Ok(Message::Text(text))) = receiver.next().await {
                println!("{name}: {text}");

                // Update the room content
                let rooms = state.rooms.lock().await;
                if let Some(room) = rooms.get(&channel) {
                    // ignore errors but log them
                    room.content_tx
                        .send(text.clone())
                        .unwrap_or_else(|err| eprintln!("Failed to send message to room: {err}"));
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
#[derive(TS, serde::Serialize)]
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

/// Static file handler
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == INDEX_HTML {
        return index_html();
    }

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();

        ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
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
