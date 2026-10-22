.pragma library

// The RICE menu's rows, for every surface that offers them (the G3, the Start menu).
// A surface paints rows and calls act(); it never decides what a row means.
// `answer` is bridge.riceMenu's reply, `mode` is theme.riceMode.
// A row: { label, mark ("✓" in force, "○" where staging returns, ""), live, kind, name, here }.
// A row of kind "" is an inert note (never live).

function ready(answer) { return !!answer && answer.ok === true }

function note(label) { return { label: label, mark: "", live: false, kind: "", name: "", here: false } }

function waiting(answer) {
    return note(!answer ? "…" : "" + (answer.message || "no answer"))
}

// every rice in the runtime rices, a "rice.nix changed" note under one whose rice.nix differs from the build's copy;
// then the declared build (kind "declarative"), and a "next rebuild: <rice>" note when the host file declares another rice
function switchRows(answer, mode) {
    if (!ready(answer)) return [waiting(answer)]
    var rows = []
    var rices = Array.isArray(answer.rices) ? answer.rices : []
    for (var i = 0; i < rices.length; i++) {
        var name = "" + rices[i].name
        var here = mode === "staging" && name === answer.rice
        var back = mode === "declarative" && name === answer.stagingRice
        rows.push({ label: rices[i].ok ? name + (rices[i].pending === true ? " · pending" : "") : name + " · unreadable",
                    mark: here ? "✓" : (back ? "○" : ""), live: !!rices[i].ok, kind: "stage", name: name, here: here })
        if (rices[i].nixChanged === true) rows.push(note("rice.nix changed"))
    }
    var decl = mode === "declarative"
    rows.push({ label: "Declarative" + (answer.declared ? ": " + answer.declared : ""), mark: decl ? "✓" : "",
                live: true, kind: "declarative", name: "", here: decl })
    if (answer.nextRebuild) rows.push(note("next rebuild: " + answer.nextRebuild))
    return rows
}

// the staged rice's saved drafts (answer.draftsRice names it)
function draftRows(answer) {
    if (!ready(answer)) return [waiting(answer)]
    var drafts = Array.isArray(answer.drafts) ? answer.drafts : []
    return drafts.map(function (d) {
        return { label: "" + d.name, mark: d.current ? "✓" : "", live: true, kind: "enter", name: "" + d.name, here: !!d.current }
    })
}

function newDraft() { return { label: "New Draft", mark: "", live: true, kind: "new", name: "", here: false } }

// a draft's delete row (offered from the draft row's context menu); not live on the draft in force
function dropDraft(draft) {
    return { label: "Delete " + draft.name, mark: "", live: !draft.here, kind: "drop", name: "" + draft.name, here: false }
}

function saveDraft(mode) { return { label: "Save as Draft", mark: "", live: mode === "staging", kind: "save", name: "", here: false } }

// the staged rice becomes the one this host performs on its next rebuild; never a draft, never the rice already declared.
// `can` is whether the bridge speaks riceDeclare. The surface asks first (declareQuestion), then calls declare().
function declareRow(answer, mode, can) {
    var rice = ready(answer) && answer.rice ? "" + answer.rice : ""
    var live = !!can && mode === "staging" && rice !== "" && rice !== answer.declared
    return { label: rice !== "" ? "Declare " + rice + "\u2026" : "Declare\u2026", mark: "", live: live, kind: "declare", name: rice, here: false }
}

function declareQuestion(answer, row) {
    var host = ready(answer) && answer.host ? "" + answer.host : "this computer"
    return "Declare " + row.name + " as " + host + "\u2019s rice? It takes effect on the next rebuild."
}

// cb gets the balloon: { ok, title, text }
function declare(bridge, row, answer, cb) {
    var host = ready(answer) && answer.host ? "" + answer.host : ""
    var file = ready(answer) && answer.hostConfig ? "" + answer.hostConfig : ""
    bridge.riceDeclare(row.name, function (reply) { cb(declared(reply, host, file)) })
}

// a reply that never came (reason "no-reply") is not a refusal: the declare may still land.
// `file` is the menu's hostConfig, the file the declare rewrites; without it, the default path
function declared(reply, host, file) {
    if (reply && reply.ok !== true && reply.reason === "no-reply")
        return { ok: false, title: "No answer yet",
                 text: "The declare may still land. Check " + (file || "hosts/" + (host || "<host>") + "/default.nix") + " before declaring again." }
    if (!reply || reply.ok !== true)
        return { ok: false, title: "Not declared", text: "" + ((reply && reply.message) || "No answer from the shell bridge.") }
    var text = reply.changed === false
        ? reply.name + " is already " + (reply.host || "this computer") + "\u2019s rice. It takes effect on the next rebuild."
        : reply.name + " is declared. It takes effect on the next rebuild."
    if (reply.stagedDiffers === true)
        text += " The staged copy has edits the rices copy lacks; the rebuild builds the rices copy."
    return { ok: true, title: "Declared", text: text }
}

// runs a row; true when the surface should show "…" until the theme's mode/rice/draft changes (or 10 s pass);
// save and drop change none of those, so they never wait; declare asks first, so act never runs it
function act(bridge, row) {
    if (!row.live || row.here || row.kind === "" || row.kind === "declare") return false
    if (row.kind === "stage") bridge.riceMode("stage", row.name)
    else if (row.kind === "declarative") bridge.riceMode("declarative")
    else bridge.riceDraft(row.kind, row.name)
    return row.kind !== "save" && row.kind !== "drop"
}
