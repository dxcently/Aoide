//! The window's commands. Each one runs its body on a blocking thread, never on
//! the main thread, and holds no lock across a confirm or a child.

use std::sync::Arc;

use aoide_desktop_shell::palette::Row;
use tauri::ipc::Channel;

use crate::wire::{AskPush, AskSink, Push, Sink};
use crate::window::{FlagIn, Reply, Window};

type Shared<'a> = tauri::State<'a, Arc<Window>>;

async fn off_thread<T: Send + 'static>(
    name: &'static str,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    note_launch(name);
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())
}

#[cfg(feature = "smoke")]
fn note_launch(name: &'static str) {
    crate::smoke::record(name);
}

#[cfg(not(feature = "smoke"))]
fn note_launch(_name: &'static str) {}

#[tauri::command]
pub async fn palette(window: Shared<'_>) -> Result<Vec<Row>, String> {
    let w = window.inner().clone();
    off_thread("palette", move || w.palette()).await
}

#[tauri::command]
pub async fn run(
    window: Shared<'_>,
    path: Vec<String>,
    flags: Vec<FlagIn>,
    positionals: Vec<String>,
) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread("run", move || w.run(&path, &flags, &positionals)).await?
}

#[tauri::command]
pub async fn sessions(window: Shared<'_>) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread("sessions", move || w.sessions()).await?
}

#[tauri::command]
pub async fn chat_open(window: Shared<'_>, id: String, channel: Channel<Push>) -> Result<(), String> {
    let w = window.inner().clone();
    let sink: Sink = Arc::new(move |push| {
        let _ = channel.send(push);
    });
    off_thread("chat_open", move || w.chat_open(&id, sink)).await?
}

#[tauri::command]
pub async fn chat_say(window: Shared<'_>, id: String, text: String) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread("chat_say", move || w.chat_say(&id, &text)).await?
}

#[tauri::command]
pub async fn chat_close(window: Shared<'_>, id: String) -> Result<(), String> {
    let w = window.inner().clone();
    off_thread("chat_close", move || w.chat_close(&id)).await?
}

#[tauri::command]
pub async fn asks_open(window: Shared<'_>, channel: Channel<AskPush>) -> Result<(), String> {
    let w = window.inner().clone();
    let sink: AskSink = Arc::new(move |push| {
        let _ = channel.send(push);
    });
    off_thread("asks_open", move || w.asks_open(sink)).await?
}

#[tauri::command]
pub async fn asks_close(window: Shared<'_>) -> Result<(), String> {
    let w = window.inner().clone();
    off_thread("asks_close", move || w.asks_close()).await?
}

/// The smoke run's note: its text is appended to the record as data. Only a
/// smoke build has it, and only the smoke capability grants it.
#[cfg(feature = "smoke")]
#[tauri::command]
pub async fn smoke_note(note: String) -> Result<(), String> {
    off_thread("smoke_note", move || crate::smoke::note(&note)).await?
}
