// tests/selection/songbook-borrow/lender/widgets/slotA.qml — the borrowed body.
// Never loaded by a suite (no shell runs here); it exists so the lender's
// `_widgets/` record has a file to point at, which is what the songMeta
// cross-checks assert.
import QtQuick

Item {
  implicitWidth: 1
  implicitHeight: 1
}
