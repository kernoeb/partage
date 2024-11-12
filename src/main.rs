use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::delete;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use optional_default::OptionalDefault;
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::signal;
use tokio::sync::broadcast;
use ts_rs::TS;

static INDEX_HTML: &str = "index.html";

#[derive(Embed)]
#[folder = "client/dist/"]
struct Assets;

/// State of the app
struct AppState {
    rooms: Mutex<HashMap<String, RoomState>>,
}

/// RoomState
struct RoomState {
    users: Mutex<HashSet<String>>,
    tx: broadcast::Sender<String>,
    content: Mutex<String>,
}

impl RoomState {
    fn new() -> Self {
        Self {
            users: Mutex::new(HashSet::new()),
            tx: broadcast::channel(69).0,
            content: Mutex::new(String::new()),
        }
    }
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT")
        .map(|val| val.parse::<u16>())
        .unwrap_or(Ok(3001))
        .unwrap();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let app_state = Arc::new(AppState {
        rooms: Mutex::new(HashMap::new()),
    });

    {
        // Add a room
        let mut rooms = app_state.rooms.lock().unwrap();
        rooms.insert("channel-1".to_owned(), RoomState::new());
    }

    let app = Router::new()
        .route("/ws", get(handler))
        .route("/rooms", get(get_rooms))
        .route("/rooms/:id", delete(remove_room))
        .with_state(app_state)
        .fallback(static_handler);

    let listener = tokio::net::TcpListener::bind(addr.to_string())
        .await
        .unwrap();

    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

/// Handler
async fn handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Update the room content
fn update_room_content(room: &RoomState, new_content: String) {
    let mut content = room.content.lock().unwrap();
    *content = new_content;
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
            println!("Name: {}", name);
            #[derive(Deserialize)]
            struct Connect {
                username: String,
                channel: String,
            }

            let connect: Connect = match serde_json::from_str(&name) {
                Ok(connect) => connect,
                Err(err) => {
                    println!("{}", &name);
                    eprintln!("{}", err);
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
                let mut rooms = state.rooms.lock().unwrap();
                channel = connect.channel.clone();

                let room = rooms.entry(connect.channel).or_insert_with(RoomState::new);

                tx = Some(room.tx.clone());

                if !room.users.lock().unwrap().contains(&connect.username) {
                    room.users
                        .lock()
                        .unwrap()
                        .insert(connect.username.to_owned());
                    username = connect.username.clone();
                }

                content = room.content.lock().unwrap().clone();
            }

            if tx.is_some() && !username.is_empty() {
                {
                    let rooms = state.rooms.lock().unwrap();
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
            } else {
                println!("Failed to connect to room! Username already taken");
                let _ = sender
                    .send(Message::Text(
                        json!(SocketMessage! {
                            message_type: SocketMessageType::Error,
                            value: Some("Username already taken".to_string()),
                        })
                        .to_string(),
                    ))
                    .await;

                return;
            }
        }
    }

    let tx = tx;
    let tx = match tx {
        Some(tx) => tx,
        None => {
            println!("Failed to connect to room!");
            return;
        }
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
            println!("Received: {}", msg);
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
                println!("{}: {}", name, text);

                // Update the room content
                let rooms = state.rooms.lock().unwrap();
                update_room_content(rooms.get(&channel).unwrap(), text.clone());

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
        _ = (&mut send_messages) => recv_messages.abort(),
        _ = (&mut recv_messages) => send_messages.abort(),
    };

    let _ = tx.send(
        json!(SocketMessage! {
            message_type: SocketMessageType::Leave,
            username: username.clone(),
        })
        .to_string(),
    );

    let mut rooms = state.rooms.lock().unwrap();
    let room = rooms.get_mut(&channel);

    if let Some(room) = room {
        room.users.lock().unwrap().remove(&username);
    } else {
        eprintln!("Failed to remove user from room!");
    }
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
    let mut rooms = state.rooms.lock().unwrap();

    // If only 1 room exists, don't remove it, return an error
    if rooms.len() == 1 {
        return Err(CustomError {
            message: "Cannot remove the last room.".to_owned(),
        });
    }

    // If the room has more than 1 user, don't remove it, return an error
    if rooms.get(&room.0).unwrap().users.lock().unwrap().len() > 1 {
        return Err(CustomError {
            message: "Room has more than 1 user.".to_owned(),
        });
    }

    rooms.remove(&room.0);

    // Notify all users that the room has been removed
    for (_, room_state) in rooms.iter() {
        let _ = room_state.tx.send(
            json!(SocketMessage! {
                message_type: SocketMessageType::UpdateRoomsList,
            })
            .to_string(),
        );
    }

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
    if let Ok(rooms) = state.rooms.lock() {
        let rooms = rooms
            .iter()
            .map(|(id, room)| Room {
                id: id.clone(),
                users: room.users.lock().unwrap().iter().cloned().collect(),
            })
            .collect();
        Json(rooms)
    } else {
        Json(vec![])
    }
}

/// Static file handler
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == INDEX_HTML {
        return index_html().await;
    }

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();

            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            if path.contains('.') {
                return not_found().await;
            }

            index_html().await
        }
    }
}

/// Index HTML handler
async fn index_html() -> Response {
    match Assets::get(INDEX_HTML) {
        Some(content) => Html(content.data).into_response(),
        None => not_found().await,
    }
}

/// 404 handler
async fn not_found() -> Response {
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
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
