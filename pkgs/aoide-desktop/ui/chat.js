"use strict";

const { invoke, Channel } = window.__TAURI__.core;

const sessionList = document.getElementById("session-list");
const chatTitle = document.getElementById("chat-title");
const statusLine = document.getElementById("status");
const log = document.getElementById("log");
const errorLine = document.getElementById("error");
const asksBox = document.getElementById("asks");
const asksNote = document.getElementById("asks-note");
const composer = document.getElementById("composer");
const input = document.getElementById("input");
const sendButton = document.getElementById("send");

let open = null;
const pending = new Map();

function node(tag, className, text) {
  const made = document.createElement(tag);
  if (className) made.className = className;
  if (text !== undefined && text !== null) made.textContent = String(text);
  return made;
}

function showError(text) {
  errorLine.textContent = text ? String(text) : "";
}

function stacked(line, body) {
  if (!line.from) return body;
  const stack = node("div", "stack");
  stack.appendChild(node("div", "from", line.from));
  stack.appendChild(body);
  return stack;
}

function row(className, children) {
  const made = node("div", "row " + className);
  for (const child of children) made.appendChild(child);
  log.appendChild(made);
  return made;
}

function bubble(line, side) {
  const body = node("div", "bubble " + side);
  if (line.from) body.appendChild(node("div", "from", line.from));
  body.appendChild(node("div", "text", line.text));
  if (line.clipped) body.appendChild(node("div", "clipped", "clipped"));
  return body;
}

function thinking(line) {
  const fold = node("details", "fold thought");
  fold.appendChild(node("summary", "", "thinking"));
  fold.appendChild(node("div", "text", line.text));
  return fold;
}

function toolFold(line) {
  const tool = line.tool;
  const fold = node("details", "fold tool" + (tool.error ? " failed" : ""));
  const summary = node("summary", "");
  summary.appendChild(node("span", "tool-name", tool.name));
  summary.appendChild(node("span", "tool-kind", tool.result === undefined ? "call" : tool.error ? "failed" : "result"));
  fold.appendChild(summary);
  if (tool.input !== undefined) fold.appendChild(node("pre", "text", tool.input));
  if (tool.result !== undefined) fold.appendChild(node("pre", "text", tool.result));
  return fold;
}

function askCard(line) {
  const ask = line.ask;
  const card = node("div", "ask");
  card.appendChild(node("div", "ask-kind", ask.kind));
  card.appendChild(node("div", "ask-prompt", ask.prompt));
  const choices = node("ul", "ask-choices");
  for (const answer of ask.answers) choices.appendChild(node("li", "", answer));
  for (const option of ask.options) {
    const item = node("li", "");
    item.appendChild(node("span", "ask-label", option.label));
    if (option.description) item.appendChild(node("span", "ask-description", option.description));
    choices.appendChild(item);
  }
  card.appendChild(choices);
  const session = open ? open.id : "<session>";
  card.appendChild(node("div", "ask-how", "answer with: aoide ask answer " + session + " " + ask.id + " <label> --expect <fingerprint>"));
  return card;
}

function choicesList(view) {
  const choices = node("ul", "ask-choices");
  for (const answer of view.answers) choices.appendChild(node("li", "", answer));
  for (const option of view.options) {
    const item = node("li", "");
    item.appendChild(node("span", "ask-label", option.label));
    if (option.description) item.appendChild(node("span", "ask-description", option.description));
    choices.appendChild(item);
  }
  return choices;
}

function openAskCard(view) {
  const card = node("div", "ask");
  card.appendChild(node("div", "ask-kind", view.kind));
  card.appendChild(node("div", "ask-prompt", view.prompt));
  if (view.call !== undefined) card.appendChild(node("pre", "ask-call", view.call));
  card.appendChild(choicesList(view));
  card.appendChild(node("div", "ask-how", "fingerprint " + view.fingerprint));
  return card;
}

function drawAsks() {
  asksBox.textContent = "";
  const mine = open ? pending.get(open.id) : undefined;
  if (mine) for (const view of mine.values()) asksBox.appendChild(openAskCard(view));
  for (const item of sessionList.children) {
    const held = pending.get(item.dataset.session);
    item.classList.toggle("asking", held !== undefined && held.size > 0);
  }
}

function onAskPush(push) {
  switch (push.type) {
    case "open": {
      const held = pending.get(push.ask.session) || new Map();
      held.set(push.ask.ask_id, push.ask);
      pending.set(push.ask.session, held);
      drawAsks();
      break;
    }
    case "gone": {
      const held = pending.get(push.session);
      if (held) {
        held.delete(push.ask_id);
        if (held.size === 0) pending.delete(push.session);
      }
      drawAsks();
      break;
    }
    case "other":
      asksNote.textContent = push.text;
      break;
    case "dropped":
      asksNote.textContent = "an asks line of " + push.bytes + " bytes was dropped";
      break;
    case "end":
      pending.clear();
      asksNote.textContent = "the asks watch closed";
      drawAsks();
      break;
    default:
      break;
  }
}

async function openAsks() {
  const channel = new Channel();
  channel.onmessage = onAskPush;
  try {
    await invoke("asks_open", { channel });
  } catch (error) {
    asksNote.textContent = String(error);
  }
}

function stateText(state) {
  return state === "waiting" ? "waiting for an answer" : "settled";
}

function addLine(line) {
  if (line.state) {
    statusLine.textContent = stateText(line.state);
    statusLine.className = "status " + line.state;
    return;
  }
  const stuck = log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
  if (line.ask) {
    row("agent", [stacked(line, askCard(line))]);
  } else if (line.tool) {
    row("agent", [stacked(line, toolFold(line))]);
  } else if (line.thinking) {
    row("agent", [stacked(line, thinking(line))]);
  } else if (line.role === "user") {
    row("user", [bubble(line, "user")]);
  } else if (line.role === "agent") {
    row("agent", [bubble(line, "agent")]);
  } else {
    row("system", [bubble(line, "system")]);
  }
  if (stuck) log.scrollTop = log.scrollHeight;
}

function addNote(text) {
  row("system", [node("div", "bubble system", text)]);
}

function onPush(push) {
  switch (push.type) {
    case "line":
      addLine(push.line);
      break;
    case "other":
      addNote(push.text);
      break;
    case "record":
      addNote(JSON.stringify(push.value));
      break;
    case "dropped":
      addNote("a line of " + push.bytes + " bytes was dropped");
      break;
    case "end":
      statusLine.textContent = "closed";
      statusLine.className = "status closed";
      break;
    default:
      break;
  }
}

async function closeOpen() {
  if (!open) return;
  const previous = open;
  open = null;
  try {
    await invoke("chat_close", { id: previous.id });
  } catch (error) {
    showError(error);
  }
}

async function openChat(id, title) {
  await closeOpen();
  log.textContent = "";
  statusLine.textContent = "";
  statusLine.className = "status";
  showError("");
  chatTitle.textContent = title;
  open = { id };
  markSelected(id);
  drawAsks();
  const channel = new Channel();
  channel.onmessage = onPush;
  try {
    await invoke("chat_open", { id, channel });
  } catch (error) {
    showError(error);
  }
}

function markSelected(id) {
  for (const item of sessionList.children) {
    item.classList.toggle("selected", item.dataset.session === id);
  }
}

function sessionItem(session) {
  const item = node("li", "session");
  item.dataset.session = session.sessionId;
  const button = node("button", "session-button");
  button.type = "button";
  button.appendChild(node("span", "session-label", session.label || session.sessionId));
  const carries = session.carries || {};
  button.appendChild(node("span", "session-meta", [carries.agent, session.state].filter(Boolean).join(" · ")));
  button.addEventListener("click", () => openChat(session.sessionId, session.label || session.sessionId));
  item.appendChild(button);
  return item;
}

async function loadSessions(autoOpen) {
  showError("");
  let reply;
  try {
    reply = await invoke("sessions");
  } catch (error) {
    showError(error);
    return;
  }
  const sessions = (reply.stdout && reply.stdout.data && reply.stdout.data.sessions) || [];
  sessionList.textContent = "";
  for (const session of sessions) sessionList.appendChild(sessionItem(session));
  if (reply.code !== 0) showError(reply.stderr || "sessions failed");
  if (open) markSelected(open.id);
  drawAsks();
  if (autoOpen && !open && sessions.length > 0) {
    const first = sessions[0];
    await openChat(first.sessionId, first.label || first.sessionId);
  }
}

async function send() {
  const text = input.value;
  if (!open || text.trim() === "") return;
  showError("");
  sendButton.disabled = true;
  try {
    await invoke("chat_say", { id: open.id, text });
    input.value = "";
  } catch (error) {
    showError(error);
  } finally {
    sendButton.disabled = false;
    input.focus();
  }
}

composer.addEventListener("submit", (event) => {
  event.preventDefault();
  send();
});

input.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    send();
  }
});

document.getElementById("refresh").addEventListener("click", () => loadSessions(false));

openAsks();
loadSessions(true);
