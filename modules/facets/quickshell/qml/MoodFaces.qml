// MoodFaces.qml — the kaomoji vocabulary, in one place.
//
// Every roster row wears a face. WORKING is the only animated state: the face
// cycles the frames of ONE set for as long as that session works. Which set it
// gets is decided by its session id, so it looks arbitrary across the roster
// (two rows working side by side almost never move in step) but never changes
// under the row — not on the next lap, and not when the delegate is rebuilt
// out from under it by a roster refresh. Every resting state holds one pose.
//
// The sets are scored, not generic: this is a music desk, so the hands beat
// time, pluck a string, pump to the beat, or write the part out. Latin present
// participles, matching the score-hand voice the rest of the dock uses.
//
// Instantiated per gadget (`MoodFaces { id: faces }`), not wired through
// shell.qml, because it holds no state: it is a table and four pure lookups.
// The alternative — threading it down from shell.qml the way DrachmaState is —
// buys nothing when there is nothing to keep in sync.
//
// FRAME AUTHORING RULES
//   • Motion must be POSITIONAL. A prop that travels along a fixed track (the
//     pen along the page, the note across the bar) reads as movement; frames
//     that only swap the mouth read as a twitching face.
//   • Prefer sets whose frames are all the SAME width — the face is drawn into
//     a fixed-width right-aligned box, so a frame that is merely wider shifts
//     its own left edge, which reads as a stretch rather than as motion.
//   • Trailing ASCII spaces are unreliable: Qt trims them when measuring, so a
//     frame that ends in " " does not actually shift. Use ideographic space
//     (U+3000) when a frame needs to sit left of its own right edge.
//   • Stay inside the CJK/kana/music repertoire the shell's fonts actually
//     cover. Anything exotic renders as tofu, and tofu is the same width every
//     frame, so it looks like a working animation that has simply stopped.

import QtQuick

QtObject {
    id: root

    // ── WORKING — ten animated sets, one assigned per session ────────────────
    readonly property var working: [
        { name: "sibilans",  // whistling — a puckered ｏ mouth with a two-note
          // conveyor streaming away across the bar: each note advances a cell
          // per frame while a fresh one is born at the lips, so the tune travels
          // and never stops. The one the desk hums to itself.
          frames: ["( ･ｏ･)♪　♫", "( ･ｏ･)　♪　", "( ･ｏ･)♫　♪", "( ･ｏ･)　♫　"] },
        { name: "moderans",  // conducting — TWO independent rates: the arm
          // sweeps every frame (ノ→ー→＼→ー), while the note only advances on
          // the ictus, half the arm's rate (frames 1/3), so the beat and the
          // sound it lands on visibly move at different speeds.
          frames: ["( ･ω･)ノ♪　", "( ･ω･)ー♪　", "( ｀∀´)＼　♪", "( ･ω･)ー　♪"] },
        { name: "pulsans",   // the flying fist — wound up, launched across the
          // gap, contact with the ●, and back for the next punch. Two rates
          // here too: the fist travels every frame, the face only snaps on
          // impact (frame 3) and settles on recoil (frame 4).
          frames: ["( ｀ω´)つ　　●", "( ｀ω´)　つ　●", "( ＾∀＾)　　つ●", "( ･ω･)　つ　●"] },
        { name: "battuens",  // drumming — TWO independent rates: the sticks
          // alternate on their own beat (left frame 2, right frame 4 — every
          // other frame), while a hi-hat ♪ ticks the off-beats (frames 1/3),
          // twice the stick's rate.
          frames: ["ノ( ･ω･)ノ♪", "＿( ｀∀´)ノ　", "ノ( ･ω･)ノ♪", "ノ( ｀∀´)＿　"] },
        { name: "saltans",   // side-step dance — the whole body travels the box,
          // left → centre → right → centre, arms up on the steps.
          frames: ["ヽ( ･∀･)ノ　　", "　ｏ( ･ω･)ｏ　", "　　ヽ( ･∀･)ノ", "　ｏ( ･ω･)ｏ　"] },
        { name: "oscillans", // the metronome — a pendulum ticking ＼ ノ ／ ノ
          // beside a face that keeps its own counsel. A swing you can count.
          frames: ["＼　( ･ω･)", "ノ　( ･ω･)", "／　( ･ω･)", "ノ　( ･ω･)"] },
        { name: "psallens",  // plucking — the hand travels the string
          frames: ["＝＝つ( ･ω･)", "＝つ＝( ･ω･)", "つ＝＝( ･ω･)", "＝つ＝( ･ω･)"] },
        { name: "scribens",  // writing the part out — pen sweeping the page
          // The paper (＿＿) is a fixed track and the pen (φ) travels along it,
          // so the stroke reads as writing, not as a twitching mouth.
          frames: ["＿＿φ( ･ω･)", "＿φ＿( ･ω･)", "φ＿＿( ･ω･)", "＿φ＿( ･ω･)"] },
        { name: "cantans",   // both fists pumping to the beat, note turning over
          frames: ["♪ｏ( ･∀･)ｏ", "♪ヽ( ･∀･)ノ", "♫ｏ( ｀∀´)ｏ", "♫ヽ( ･∀･)ノ"] },
        { name: "inclinans", // the curtain call — arms flung up, compose, a deep
          // bow with hands presented low, rise. A performer taking it forever.
          frames: ["ヽ( ･∀･)ノ", "　( ･ω･)　", "ｏ( ＿＿)ｏ", "　( ･ω･)　"] }
    ]

    // Which set a session wears, for its whole life. Hashed from the row's key
    // (its sessionId) rather than drawn at random, so it survives the delegate
    // being rebuilt by a roster refresh — a Math.random() pick re-rolled every
    // time the model churned, which read as the face changing its mind for no
    // reason. Different ids land on different sets; the same id always lands on
    // the same one.
    function pickFor(key) {
        return Math.abs(_hash(key, 31)) % working.length;
    }
    // Where in its set a session STARTS. Two sessions born at the same instant
    // that happen to hash onto the same set would otherwise march in perfect
    // lockstep, which reads as one animation drawn twice; a second, independent
    // hash offsets them so a collision still looks like two separate hands.
    function phaseFor(key, len) {
        var n = Math.max(1, len || 1);
        return Math.abs(_hash(key, 131)) % n;
    }
    function _hash(key, mult) {
        var s = "" + (key || "");
        var h = 0;
        for (var i = 0; i < s.length; i++)
            h = (h * mult + s.charCodeAt(i)) | 0;
        return h;
    }
    // Frames of set `i`, wrapped so a stale index from a shrinking table (or a
    // negative one) can never index past the end.
    function workingFrames(i) {
        var n = working.length;
        return working[((i % n) + n) % n].frames;
    }
    function workingName(i) {
        var n = working.length;
        return working[((i % n) + n) % n].name;
    }

    // ── RESTING — one still pose per state ───────────────────────────────────
    // Takes the CANONICAL state (the caller normalizes; this file does not own
    // the state vocabulary). Five canonical states since the bridge split
    // `idle`: working · awaiting · stopped · idle · done.
    function still(canonState) {
        switch (canonState) {
        case "awaiting": return "(；･∀･)?";    // needs the human — anxious, asking
        case "stopped":  return "( ･ω･)b";     // turn over, hands off, standing by
        case "idle":     return "(－ω－) zzZ";  // cold: long at rest, or fresh/resumed
        case "done":     return "( ´▽｀ )";     // content, retired
        case "working":  return working[0].frames[0];   // animated by the caller
        default:         return "( ･_･)";       // puzzled
        }
    }
}
