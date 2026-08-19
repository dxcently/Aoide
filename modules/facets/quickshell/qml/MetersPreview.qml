import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render JUST the Meters gadget for a screenshot.
//   qs -p modules/facets/quickshell/qml/MetersPreview.qml
// Floats a ~340x300 overlay surface carrying a stub gold-marble palette; the
// gadget reads the machine's real /proc/stat + /proc/meminfo live.
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-meters-preview"
        implicitWidth: 340
        implicitHeight: 300

        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        MetersGadget {
            anchors.fill: parent
            anchors.margins: 0

            // stub palette — the warm gold-marble light theme (hex only here)
            livery: QtObject {
                property string paletteBg:     "#f2ebde"   // pale marble ground
                property string paletteFg:     "#2f2a33"   // dark plum ink
                property string paletteAccent: "#a07414"   // Attic gold
                property string paletteHot:    "#4e8b45"   // laurel green (the one standout)
                property string paletteUrgent: "#b0472f"   // terracotta
                property string wireCyan:      "#3f867e"   // teal (Doric signature)
                property string holoBlue:      "#345f81"   // aegean (RAM fill)
                property string violet:        "#6f4373"   // murex (CPU fill)
            }
        }
    }
}
