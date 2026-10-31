//! The window's commands. Each one runs its body on a blocking thread, never on
//! the main thread, and holds no lock across a confirm or a child.

use std::sync::Arc;

use aoide_desktop_shell::palette::Row;
use tauri::ipc::Channel;

use crate::wire::{Push, Sink};
use crate::window::{FlagIn, Reply, Window};

type Shared<'a> = tauri::State<'a, Arc<Window>>;

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn palette(window: Shared<'_>) -> Result<Vec<Row>, String> {
    let w = window.inner().clone();
    off_thread(move || w.palette()).await
}

#[tauri::command]
pub async fn run(
    window: Shared<'_>,
    path: Vec<String>,
    flags: Vec<FlagIn>,
    positionals: Vec<String>,
) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread(move || w.run(&path, &flags, &positionals)).await?
}

#[tauri::command]
pub async fn sessions(window: Shared<'_>) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread(move || w.sessions()).await?
}

#[tauri::command]
pub async fn chat_open(window: Shared<'_>, id: String, channel: Channel<Push>) -> Result<(), String> {
    let w = window.inner().clone();
    let sink: Sink = Arc::new(move |push| {
        let _ = channel.send(push);
    });
    off_thread(move || w.chat_open(&id, sink)).await?
}

#[tauri::command]
pub async fn chat_say(window: Shared<'_>, id: String, text: String) -> Result<Reply, String> {
    let w = window.inner().clone();
    off_thread(move || w.chat_say(&id, &text)).await?
}

#[tauri::command]
pub async fn chat_close(window: Shared<'_>, id: String) -> Result<(), String> {
    let w = window.inner().clone();
    off_thread(move || w.chat_close(&id)).await?
}
