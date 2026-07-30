import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render JUST the Power gadget for a screenshot.
//   qs -p modules/facets/quickshell/qml/PowerPreview.qml
// Floats a ~340x300 overlay surface carrying a stub gold-marble palette; the
// gadget reads real UPower (battery) + /proc/net/route (network) live. On a
// desktop with no battery it exercises the honest "AC — no battery" path.
//
// NOTE: instantiates PowerVitalsGadget — the fresh Corinthian power temple —
// because the legacy PowerGadget.qml filename is still a live surface.
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-power-preview"
        implicitWidth: 340
        implicitHeight: 300

        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        PowerVitalsGadget {
            anchors.fill: parent
            anchors.margins: 0

            // stub palette — the warm gold-marble light theme (hex only here)
            notes: QtObject {
                property string paletteBg:     "#f2ebde"   // pale marble ground
                property string paletteFg:     "#2f2a33"   // dark plum ink
                property string paletteAccent: "#a07414"   // Attic gold (numeric tallies)
                property string paletteHot:    "#4e8b45"   // laurel green (live-link standout)
                property string paletteUrgent: "#b0472f"   // terracotta (low / offline)
                property string wireCyan:      "#3f867e"   // teal (charge fill)
                property string holoBlue:      "#345f81"   // aegean (link + detail)
                property string violet:        "#6f4373"   // murex (Corinthian signature)
            }
        }
    }
}
