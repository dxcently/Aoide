// tests/quickshell-seam/fixture/shell.qml — a host-brought Quickshell config.
//
// The quickshell dendrite starts whatever directory `aoide.quickshell.config`
// names; this is the smallest thing that answers as one (quickshell loads
// `<dir>/shell.qml`). It exists so the `test-quickshell-only` host can prove
// the lyra-less shape end to end: a shell runs, and nothing in the evaluation
// belongs to lyra.
import QtQuick

ShellRoot {
}
