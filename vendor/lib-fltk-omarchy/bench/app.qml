// Bench workload "app": header, bound input/label, buttons, progress,
// status and a 100 000-row virtualized ListView with two columns.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: win
    title: Qt.application.arguments[Qt.application.arguments.length - 1]
    width: 520; height: 400; visible: true
    property int clicks: 0
    header: ToolBar { Label { text: "Bench"; font.bold: true; anchors.verticalCenter: parent.verticalCenter; x: 10 } }
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 8
        TextField { id: name; text: "Omarchy"; Layout.fillWidth: true }
        Label { text: "Hello " + name.text + " (" + win.clicks + ")"; font.bold: true }
        RowLayout {
            Button { text: "Add"; highlighted: true; onClicked: win.clicks++ }
            Button { text: "Reset"; onClicked: win.clicks = 0 }
        }
        ProgressBar { value: 0.4; Layout.fillWidth: true }
        ListView {
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true
            model: 100000
            delegate: Item {
                width: ListView.view.width; height: 28
                Label { text: "Item " + index; x: 12; anchors.verticalCenter: parent.verticalCenter }
                Label { text: (index * 3) + " KB"; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter }
            }
            ScrollBar.vertical: ScrollBar {}
        }
    }
}
