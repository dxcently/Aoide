// tests/consumer/songs/solo/widgets/bar.qml — the body the borrowed `bar` record
// names. Never loaded here (no shell runs in this fixture); it exists so the
// songbook's own cross-checks — a record pointing at a file that does not exist
// — are exercised on the consumer's songbook rather than waived.
import QtQuick

Item {
  implicitWidth: 1
  implicitHeight: 1
}
