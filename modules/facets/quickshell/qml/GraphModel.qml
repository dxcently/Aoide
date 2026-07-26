// GraphModel.qml — shared DAG row-builder for graph.json consumers.
//
// Canonical home of buildRows(): the pure-JS flattening of the project/session
// DAG (song/stage/graph.json v0) into an indented, cycle-safe row list. Both
// AoideSessionGraph (the toggleable overlay) and DagGraphGadget (the always-on
// dock gadget) instantiate this and read `rows`, so the tree logic lives in ONE
// place. Previously buildRows was inlined in AoideSessionGraph; it is preserved
// here byte-for-byte in behaviour (machine-checked — see the node snippets in
// the task report).
//
// Reading discipline: this object computes only. It holds no colors, no bridge,
// no file I/O — the consumer owns the FileView watch and passes the parsed
// document in via `graph`. Colors are applied by the delegate (GraphRow), never
// here (CONTRACTS.md §1: notes-only, zero hardcoded hex — nothing to hardcode
// here since there is no rendering).

import QtQuick

QtObject {
    id: root

    // ── Parsed graph document (v0: { schemaVersion, nodes, edges }) ────────
    // The consumer binds this to its FileView-parsed JSON. Empty default →
    // buildRows returns [] → consumer shows its empty prompt.
    property var graph: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    // ── Flattened, indented tree rows ──────────────────────────────────────
    // Each row is either a node row { node, depth } or a synthetic root
    // { synthetic, depth }. Recomputes whenever `graph` changes.
    readonly property var rows: buildRows(root.graph)

    // ── Tree builder (cycle-safe) ──────────────────────────────────────────
    // Machine-checked against the v0 schema: normal nesting, unanchored
    // grouping, cycle guard (visited set), empty/absent, dangling edges.
    function buildRows(g) {
        var out = []
        if (!g || !g.nodes)
            return out

        var nodes = g.nodes || []
        var edges = g.edges || []

        // Index nodes by id.
        var byId = ({})
        for (var i = 0; i < nodes.length; i++)
            byId[nodes[i].id] = nodes[i]

        // children[fromId] = [toId, …]; hasIncoming[toId] = true.
        // Dangling edges (either endpoint missing) are ignored so a
        // hand-edited file can't inject phantom rows.
        var children = ({})
        var hasIncoming = ({})
        for (var e = 0; e < edges.length; e++) {
            var edge = edges[e]
            if (!edge || !edge.from || !edge.to)
                continue
            if (!byId[edge.from] || !byId[edge.to])
                continue
            if (!children[edge.from])
                children[edge.from] = []
            children[edge.from].push(edge.to)
            hasIncoming[edge.to] = true
        }

        // Cycle-safety: graph.json is produced cycle-free (the CLI rejects
        // cycle links), but a hand-edited file must not hang QML — guard the
        // traversal with a visited set regardless.
        var visited = ({})
        function walk(id, depth) {
            if (visited[id])
                return
            visited[id] = true
            var node = byId[id]
            if (!node)
                return
            out.push({ "node": node, "depth": depth })
            var kids = children[id] || []
            for (var k = 0; k < kids.length; k++)
                walk(kids[k], depth + 1)
        }

        // Roots: every project node, in node order.
        for (var p = 0; p < nodes.length; p++)
            if (nodes[p].kind === "project")
                walk(nodes[p].id, 0)

        // Unanchored sessions: session nodes with NO incoming edge.
        var unanchored = []
        for (var s = 0; s < nodes.length; s++) {
            var n = nodes[s]
            if (n.kind === "session" && !hasIncoming[n.id] && !visited[n.id])
                unanchored.push(n.id)
        }
        if (unanchored.length > 0) {
            out.push({ "synthetic": "(unanchored)", "depth": 0 })
            for (var u = 0; u < unanchored.length; u++)
                walk(unanchored[u], 1)
        }

        // Safety net: any session still unvisited (e.g. only reachable through
        // a cycle we broke) is surfaced at depth 0 so nothing silently vanishes.
        for (var o = 0; o < nodes.length; o++)
            if (nodes[o].kind === "session" && !visited[nodes[o].id])
                walk(nodes[o].id, 0)

        return out
    }
}
