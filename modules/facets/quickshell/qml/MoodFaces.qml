// MoodFaces.qml — the kaomoji vocabulary, in one place.
//
// Every roster row wears a face. WORKING is the only animated state: the face
// cycles the frames of ONE set for as long as that session works. Which set it
// gets is decided by its session id, so it looks arbitrary across the roster
// (two rows working side by side almost never move in step) but never changes
// under the row — not on the next lap, and not when the delegate is rebuilt
// out from under it by a roster refresh. Every resting state holds one pose.
//
// No unifying theme by design — these are just lively, popular-style
// kaomoji animations (a cheer, a dance, a table flip, a wave). Plain
// descriptive names, no forced direction, no forced concept per set.
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
//     The one sanctioned exception is the parcel glyph 󰏗 (U+F03D7, plus roll's
//     mid-tumble circle U+F111): those live in the Nerd Font's private-use
//     range, and since BOTH pools now carry them, every Text that draws these
//     frames must set font.family to the Nerd Font (both gadgets do).

import QtQuick

QtObject {
    id: root

    // ── WORKING — the general animated pool, one set assigned per session ────
    // Third redesign: no unifying theme, no fixed direction — each set is just
    // a lively web-style kaomoji animation. Motion is POSITIONAL and BIG: the
    // figure or its prop visibly changes cell position every frame. Every
    // frame in a set carries the same glyph-width composition (same count of
    // fullwidth cells and ASCII chars), so the right-aligned box never
    // stretches. U+3000 ideographic spaces hold the width.
    readonly property var working: [
        { name: "whistle",   // strolling whistle, hands-in-pockets chill: flat
          // 一一 eyes never move, the pucker still cycles ｏ↔・, and a single
          // lazy ♪ ambles across the box one cell per beat — unhurried, not
          // startled.
          frames: ["　　　♪( 一ｏ一)", "　　♪　( 一・一)", "　♪　　( 一ｏ一)", "♪　　　( 一・一)"] },
        { name: "cheer",     // jumping side to side, arms thrown up ＼／, then
          // swung down ／＼, then up again ヽノ — a full-body bounce.
          frames: ["　＼( ＾ｏ＾)／", "／( ＾ｏ＾)＼　", "　ヽ( ＾ｏ＾)ノ", "＼( ＾ｏ＾)／　"] },
        { name: "wiggle",    // the cat-dance shuffle — the leading arm ～ flips
          // across the body while the whole figure slides between slots.
          frames: ["　(～・ω・)～", "～(・ω・～)　", "(～・ω・)～　", "　～(・ω・～)"] },
        { name: "dash",      // full-box sprint — the runner crosses the entire
          // frame one cell per beat, double speed-lines ＝＝ trailing, then
          // wraps hard back to the start.
          frames: ["　　　( ｀ー´)＝＝", "　　( ｀ー´)＝＝　", "　( ｀ー´)＝＝　　", "( ｀ー´)＝＝　　　"] },
        { name: "tableflip", // the table ＿＿ is hurled: cartwheels away ／／,
          // lands upside-down ￣￣, tumbles back ＼＼, resets flat. The
          // thrower holds the double-ノ hurl pose throughout.
          frames: ["(ノ｀ｏ´)ノ＿＿　　", "(ノ｀ｏ´)ノ　／／　", "(ノ｀ｏ´)ノ　　￣￣", "(ノ｀ｏ´)ノ　＼＼　"] },
        { name: "wave",      // the ノシ greeting — the arm blurs into one then
          // two shake-marks while the body hops between slots.
          frames: ["( ´ω｀)ノ　　", "　( ´ω｀)ノシ", "( ´ω｀)ノシシ", "　( ´ω｀)ノ　"] },
        { name: "jab",       // the fist ⊃ fires out three cells to full
          // extension and snaps back — a straight one-two punch.
          frames: ["( ｀ω´)⊃　　", "( ｀ω´)　⊃　", "( ｀ω´)　　⊃", "( ｀ω´)　⊃　"] },
        { name: "dig",       // shovel duty: each scoop of dirt 彡 is flung a
          // cell further until it sails clean off the edge, then one
          // empty-handed beat before the next spadeful.
          frames: ["( ｀ω´)⊃彡　　", "( ｀ω´)⊃　彡　", "( ｀ω´)⊃　　彡", "( ｀ω´)⊃　　　"] },
        { name: "juggle",    // two balls ping in and out on both sides of the
          // face, out of phase — near/near, far/far, split, swapped.
          frames: ["　ｏ( ＾ω＾)ｏ　", "ｏ　( ＾ω＾)　ｏ", "　ｏ( ＾ω＾)　ｏ", "ｏ　( ＾ω＾)ｏ　"] },
        { name: "boogie",    // arms-up dancer sweeps left→mid→right→mid across
          // the box while a single ♪ flits from side to side around it.
          frames: ["♪ヽ( ・ω・)ノ　　", "　ヽ( ・ω・)ノ♪　", "　　ヽ( ・ω・)ノ♪", "　♪ヽ( ・ω・)ノ　"] },
        // toss/duet/weave assume ⊃/⊂ render at 2 cells (matching ノ/ヽ) in the
        // shell's Nerd Font, unlike every other ⊃/⊂ set here which just
        // reorders a constant glyph multiset. Re-check frame width by
        // East-Asian-Width cell count if that font ever changes.
        { name: "toss",      // a lazy game of catch between two ´ω｀ twins who
          // wear the same soft face the whole rally; the arms do the acting —
          // ⊃ hurl, hold through the flight, ⊂ scoop on the landing, then
          // the return throw, mirrored.
          frames: ["(´ω｀)⊃ｏ　　ヽ(´ω｀)", "(´ω｀)ノ　ｏ　ヽ(´ω｀)", "(´ω｀)ノ　　ｏ⊂(´ω｀)", "(´ω｀)ノ　ｏ　⊂(´ω｀)"] },
        { name: "duet",      // the same ´ω｀ twins, different game: a single ♪
          // lobbed back and forth — the tune is the ball, passed one cell per
          // beat down the same throw/scoop arc.
          frames: ["(´ω｀)⊃♪　　ヽ(´ω｀)", "(´ω｀)ノ　♪　ヽ(´ω｀)", "(´ω｀)ノ　　♪⊂(´ω｀)", "(´ω｀)ノ　♪　⊂(´ω｀)"] },
        { name: "weave",     // and their juggling act: a ball each, thrown up
          // together ノ…ヽ, crossing ｏｏ in the middle, caught ⊃…⊂ on the
          // other side — same soft faces the whole exchange.
          frames: ["(´ω｀)⊃ｏ　　ｏ⊂(´ω｀)", "(´ω｀)ノｏ　　ｏヽ(´ω｀)", "(´ω｀)ノ　ｏｏ　ヽ(´ω｀)", "(´ω｀)ノｏ　　ｏヽ(´ω｀)"] },
        { name: "scribble",  // the pen φ walks the page line ＿＿＿ one cell per
          // beat, left to right, then wraps — a new line started.
          frames: ["( ・ω・)φ＿＿＿", "( ・ω・)＿φ＿＿", "( ・ω・)＿＿φ＿", "( ・ω・)＿＿＿φ"] },
        { name: "sip",       // tea break: the cup つ口 lifts a cell to the lips
          // and back while the steam ～ curls around it; eyes go ＾＾ on the
          // sip and 一一 on the contented exhale.
          frames: ["( ・ω・)　つ口～", "( ・ω・)つ口　～", "( ＾ω＾)つ口～　", "( 一ω一)　つ口～"] },
        { name: "read",      // book ⊂口, eyes down 一一 scanning; a page peels
          // up ／, tumbles over ＼ a cell further, and lands — a beat of
          // ＾＾ satisfaction before the next page.
          frames: ["( 一ω一)⊂口　　", "( 一ω一)⊂口／　", "( 一ω一)⊂口　＼", "( ＾ω＾)⊂口　　"] },
        { name: "lift",      // reps: the barbell ｏ＝ｏ is pressed out to full
          // extension one cell per beat and pulled back, strain face ｀´ held
          // the whole set.
          frames: ["( ｀ω´)⊃ｏ＝ｏ　　", "( ｀ω´)　⊃ｏ＝ｏ　", "( ｀ω´)　　⊃ｏ＝ｏ", "( ｀ω´)　⊃ｏ＝ｏ　"] },
        { name: "catch",     // incoming delivery: a parcel 󰏗 sails in from the
          // right a cell per beat, arms ノ up and ready, and lands in the
          // hand つ with a ＾ω＾ — the loop restart is the next drop.
          frames: ["( ・ω・)ノ　　󰏗", "( ・ω・)ノ　󰏗　", "( ・ω・)ノ󰏗　　", "( ＾ω＾)つ󰏗　　"] },
        { name: "inbox",     // the other doorstep: parcels 󰏗 slide in from the
          // left edge and each one is scooped in behind the arm ⊂ the moment
          // it arrives — received, tucked, next.
          frames: ["󰏗　　⊂( ・ω・)", "　󰏗　⊂( ・ω・)", "　　󰏗⊂( ・ω・)", "　　⊂󰏗( ＾ω＾)"] }
    ]

    // ── PACKAGES — the subagent pool: a courier delivering work back to the
    // main agent that dispatched it. A subagent always wears one of THESE
    // sets, never the general pool above — see pickFor()/randomIndex() below
    // for the policy split. Not locked to any count; add more as they come.
    // Every set carries the parcel 󰏗 somewhere; same fixed-width/U+3000
    // rules as the general pool.
    readonly property var packages: [
        { name: "haul",     // the delivery run itself — courier marches the
          // full width of the box, parcel leading in the outstretched arm;
          // the loop restart reads as the next run.
          frames: ["( ｀ー´)⊃󰏗　　　", "　( ｀ー´)⊃󰏗　　", "　　( ｀ー´)⊃󰏗　", "　　　( ｀ー´)⊃󰏗"] },
        { name: "handoff",  // step-and-drop gait: carry → thrust forward →
          // release (a gap opens between hand つ and 󰏗 as it sits down) →
          // long reach to re-grab, then carry again.
          frames: ["( ・ω・)つ󰏗　　", "( ・ω・)　つ󰏗　", "( ・ω・)つ　󰏗　", "( ・ω・)　　つ󰏗"] },
        { name: "stack",    // shuttle run to a pile at the left edge: arrive
          // with a box, close in, DROP (the pile grows 󰏗→󰏗󰏗, the hand
          // empties), step back for the next — "where the main agent picks
          // it up," made literal.
          frames: ["󰏗　　( ・ω・)つ󰏗", "󰏗　( ・ω・)つ󰏗　", "󰏗󰏗( ・ω・)つ　　", "󰏗󰏗　( ・ω・)つ　"] },
        { name: "roll",     // barrel freight — the parcel rolls out ahead and
          // back, tumbling 󰏗→→󰏗 as it turns, the arm つ chasing it.
          frames: ["( ｀ω´)つ󰏗　　", "( ｀ω´)つ　　", "( ｀ω´)つ　　󰏗", "( ｀ω´)つ　　"] }
    ]

    // Which set a SUBAGENT wears, for its whole life. Hashed from the row's key
    // (its sessionId) rather than drawn at random, so it survives the delegate
    // being rebuilt by a roster refresh — a Math.random() pick re-rolled every
    // time the model churned read as the face changing its mind for no reason,
    // and a subagent's courier deserves a consistent identity for its life.
    // `pool` defaults to the general `working` table; pass `packages` for a
    // subagent row.
    function pickFor(key, pool) {
        pool = pool || working;
        return Math.abs(_hash(key, 31)) % pool.length;
    }
    // Where in its set a subagent STARTS. Two subagents born at the same
    // instant that happen to hash onto the same set would otherwise march in
    // perfect lockstep, which reads as one animation drawn twice; a second,
    // independent hash offsets them so a collision still looks like two
    // separate couriers.
    function phaseFor(key, len) {
        var n = Math.max(1, len || 1);
        return Math.abs(_hash(key, 131)) % n;
    }
    // A genuinely random index into a pool of size `n` — used for everything
    // that ISN'T a subagent (plain terminals, top-level agents). Deliberately
    // the opposite policy from pickFor: these rows re-roll their set every
    // time their delegate is (re)created, i.e. whenever the roster refreshes.
    // That's a real design choice, not an oversight — subagents get a stable
    // identity because their courier tells a small continuing story; everyone
    // else just gets variety.
    function randomIndex(n) {
        return Math.floor(Math.random() * Math.max(1, n || 1));
    }
    function _hash(key, mult) {
        var s = "" + (key || "");
        var h = 0;
        for (var i = 0; i < s.length; i++)
            h = (h * mult + s.charCodeAt(i)) | 0;
        return h;
    }
    // Frames/name of set `i` in `pool` (defaults to `working`), wrapped so a
    // stale index from a shrinking table (or a negative one) can never index
    // past the end.
    function workingFrames(i, pool) {
        pool = pool || working;
        var n = pool.length;
        return pool[((i % n) + n) % n].frames;
    }
    function workingName(i, pool) {
        pool = pool || working;
        var n = pool.length;
        return pool[((i % n) + n) % n].name;
    }

    // ── RESTING — one still pose per state ───────────────────────────────────
    // Takes the CANONICAL state (the caller normalizes; this file does not own
    // the state vocabulary). Five canonical states since the bridge split
    // `idle`: working · awaiting · stopped · idle · done. `agent` splits the
    // one pose that differs by caller: an idle AGENT (Conductor row) sits
    // blank-faced ('_'), while an idle TERMINAL just sleeps.
    function still(canonState, agent) {
        switch (canonState) {
        case "awaiting": return "(；･∀･)?";    // needs the human — anxious, asking
        case "stopped":  return "( ･ω･)b";     // turn over, hands off, standing by
        case "idle":     return agent ? "('_')"          // cold agent: vacant stare
                                      : "(－ω－) zzZ";  // cold tty: fast asleep
        case "done":     return "( ´▽｀ )";     // content, retired
        case "working":  return working[0].frames[0];   // animated by the caller
        default:         return "( ･_･)";       // puzzled
        }
    }
}
