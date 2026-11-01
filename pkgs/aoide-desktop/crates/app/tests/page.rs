//! The chat page's code, and its title (the window's own title is read from the
//! real window by the smoke run: the mock runtime keeps none). Every string the
//! page draws is hostile data, so the page builds its DOM from text nodes only:
//! no markup is parsed from a string and no string is run as code.

use std::fs;
use std::path::{Path, PathBuf};

fn ui() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui")
}

/// DOM events the page must never assign as a property. Tauri's channel
/// `onmessage` is not one of them.
const EVENTS: &[&str] = &[
    "click", "dblclick", "input", "change", "keydown", "keyup", "keypress", "submit", "load", "error", "focus",
    "blur", "mouseover", "mousedown", "mouseup", "mousemove",
];

/// What a script may not hold: a way to parse markup from a string, or to run
/// a string as code.
const SCRIPT_BANNED: &[&str] = &[
    "innerHTML",
    "outerHTML",
    "insertAdjacentHTML",
    "document.write",
    "new Function",
    "srcdoc",
    "javascript:",
    "setAttribute(\"on",
    "setAttribute('on",
];

fn word_before(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Every place `name` stands as a whole call: `name(`, with no letter before it.
fn calls(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        !word_before(text, at) && text[at + name.len()..].trim_start().starts_with('(')
    })
}

/// A `.onclick =` style assignment of a DOM event.
fn assigns_event(text: &str) -> bool {
    EVENTS.iter().any(|event| {
        let head = format!(".on{event}");
        text.match_indices(&head).any(|(at, _)| {
            let rest = text[at + head.len()..].trim_start();
            rest.starts_with('=') && !rest.starts_with("==")
        })
    })
}

/// An `onclick=` style attribute in markup.
fn has_event_attribute(text: &str) -> bool {
    text.match_indices(|c: char| c.is_whitespace()).any(|(at, space)| {
        let rest = &text[at + space.len()..];
        let Some(rest) = rest.strip_prefix("on") else { return false };
        let name = rest.chars().take_while(|c| c.is_ascii_alphabetic()).count();
        name > 0 && rest[name..].trim_start().starts_with('=')
    })
}

/// A `<script>` tag with no `src`: inline code.
fn has_inline_script(text: &str) -> bool {
    text.match_indices("<script").any(|(at, _)| {
        let tag = &text[at..];
        let end = tag.find('>').unwrap_or(tag.len());
        !tag[..end].contains("src=")
    })
}

/// What is wrong with one page file, by its extension.
fn violations(name: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut flag = |what: &str| found.push(format!("{name}: {what}"));
    if name.ends_with(".js") {
        for banned in SCRIPT_BANNED {
            if text.contains(banned) {
                flag(banned);
            }
        }
        if calls(text, "eval") || text.contains(".eval(") {
            flag("eval");
        }
        if calls(text, "Function") {
            flag("Function(");
        }
        if assigns_event(text) {
            flag("an event handler property");
        }
    } else if name.ends_with(".html") {
        if has_event_attribute(text) {
            flag("an on-attribute");
        }
        if has_inline_script(text) {
            flag("an inline script");
        }
        for banned in ["<style", " style=", "javascript:", "srcdoc"] {
            if text.contains(banned) {
                flag(banned);
            }
        }
    } else if name.ends_with(".css") && (text.contains("http:") || text.contains("https:") || text.contains("@import")) {
        flag("a remote or imported style");
    }
    found
}

fn page_files() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(ui()).expect("ui/ is read") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().expect("a name").to_string_lossy().into_owned();
        files.push((name, fs::read_to_string(&path).expect("a page file is text")));
    }
    files
}

#[test]
fn the_page_parses_no_markup_from_a_string_and_runs_no_string() {
    let files = page_files();
    for kind in [".html", ".js", ".css"] {
        assert!(files.iter().any(|(name, _)| name.ends_with(kind)), "ui/ holds a {kind} file");
    }
    let found: Vec<String> = files.iter().flat_map(|(name, text)| violations(name, text)).collect();
    assert!(found.is_empty(), "the page builds markup from text or runs text as code:\n{}", found.join("\n"));
}

#[test]
fn the_scan_flags_each_construct_it_bars() {
    for planted in [
        "el.innerHTML = text;",
        "el.outerHTML = text;",
        "el.insertAdjacentHTML('beforeend', text);",
        "document.write(text);",
        "eval(text);",
        "window.eval(text);",
        "const f = new Function(text);",
        "Function(text)();",
        "el.onclick = go;",
        "el.setAttribute('onclick', text);",
        "frame.srcdoc = text;",
        "location = 'javascript:alert(1)';",
    ] {
        assert!(!violations("x.js", planted).is_empty(), "{planted} is flagged");
    }
    for planted in [
        "<button onclick=\"go()\">",
        "<img src=x onerror=go()>",
        "<script>go()</script>",
        "<p style=\"color: red\">",
        "<style>p {}</style>",
    ] {
        assert!(!violations("x.html", planted).is_empty(), "{planted} is flagged");
    }
    assert!(!violations("x.css", "@import url(https://example.test/a.css);").is_empty());
}

#[test]
fn the_scan_lets_the_constructs_the_page_uses_through() {
    let used = "channel.onmessage = onPush; el.textContent = text; document.createElement('div'); const medieval = 1; input.addEventListener('keydown', go);";
    assert!(violations("x.js", used).is_empty(), "{:?}", violations("x.js", used));
    let html = "<script src=\"chat.js\"></script><button id=\"refresh\" type=\"button\">";
    assert!(violations("x.html", html).is_empty(), "{:?}", violations("x.html", html));
}

#[test]
fn the_page_is_titled_aoide() {
    let html = fs::read_to_string(ui().join("index.html")).expect("the page is read");
    assert!(html.contains("<title>Aoide</title>"), "the page's title");
}
