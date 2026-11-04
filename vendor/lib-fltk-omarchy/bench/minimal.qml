// Bench workload "minimal": one label and one button in a 520x400 window.
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    title: Qt.application.arguments[Qt.application.arguments.length - 1]
    width: 520; height: 400; visible: true
    Column {
        anchors.fill: parent; anchors.margins: 16; spacing: 8
        Label { text: "Hello from QML" }
        Button { text: "Button"; width: parent.width }
    }
}
