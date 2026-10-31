//! The native confirm. The window asks through `Confirm`; the GTK one shows a
//! modal dialog on the main thread, with plain text and Cancel as the default.

use aoide_desktop_shell::exec::untrusted::visible;
use gtk::prelude::*;
use gtk::{ButtonsType, DialogFlags, MessageDialog, MessageType, ResponseType};
use std::sync::mpsc;

/// Asks the person before a call runs. `true` only for the verb's own answer.
pub trait Confirm: Send + Sync {
    fn confirm(&self, verb: &str, text: &str) -> bool;
}

/// The GTK dialog over the main window.
pub struct Gtk {
    app: tauri::AppHandle,
}

impl Gtk {
    pub fn new(app: tauri::AppHandle) -> Gtk {
        Gtk { app }
    }
}

impl Confirm for Gtk {
    fn confirm(&self, verb: &str, text: &str) -> bool {
        let (tx, rx) = mpsc::channel::<bool>();
        let verb = visible(verb);
        let text = visible(text);
        let sent = self.app.run_on_main_thread(move || {
            let _ = tx.send(ask_on_main(&verb, &text));
        });
        if sent.is_err() {
            return false;
        }
        rx.recv().unwrap_or(false)
    }
}

fn ask_on_main(verb: &str, text: &str) -> bool {
    let dialog = MessageDialog::new(
        None::<&gtk::Window>,
        DialogFlags::MODAL,
        MessageType::Question,
        ButtonsType::None,
        "Confirm",
    );
    dialog.set_secondary_text(Some(text));
    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button(verb, ResponseType::Accept);
    dialog.set_default_response(ResponseType::Cancel);
    let answer = dialog.run();
    dialog.close();
    answer == ResponseType::Accept
}
