import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    width: 760; height: 680; visible: true
    title: qsTr("Türkiye DPI Yöneticisi")
    color: palette.window
    property string selectedProfile: "discord"

    Connections { target: backend; function onOperationFailed(message) { errorDialog.text = message; errorDialog.open() } }
    Dialog { id: errorDialog; title: qsTr("İşlem başarısız"); standardButtons: Dialog.Ok; property alias text: errorLabel.text; Label { id: errorLabel; wrapMode: Text.Wrap } }

    ColumnLayout {
        anchors.fill: parent; anchors.margins: 24; spacing: 16
        Label { text: qsTr("Türkiye DPI Yöneticisi"); font.pixelSize: 28; font.bold: true }
        GridLayout {
            columns: 2; columnSpacing: 24; rowSpacing: 8; Layout.fillWidth: true
            Label { text: qsTr("Durum:"); font.bold: true }
            Label { text: backend.active ? qsTr("● Aktif") : qsTr("○ Pasif"); color: backend.active ? "#27ae60" : palette.text }
            Label { text: qsTr("Bağlantı:"); font.bold: true }
            Label { text: backend.network }
            Label { text: qsTr("Aktif profil:"); font.bold: true }
            Label { text: backend.profile }
            Label { text: qsTr("Çalışan yöntem:"); font.bold: true }
            Label { text: backend.method; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
        RowLayout {
            Label { text: qsTr("Profil") }
            ComboBox {
                id: profiles; model: ["discord", "safe", "balanced", "aggressive"]
                onCurrentTextChanged: window.selectedProfile = currentText
            }
            Button { text: qsTr("Başlat"); enabled: !backend.busy; onClicked: backend.start(window.selectedProfile) }
            Button { text: qsTr("Otomatik Test"); enabled: !backend.busy; onClicked: backend.autoSelect() }
            Button { text: qsTr("Durdur"); enabled: !backend.busy && backend.active; onClicked: backend.stop() }
        }
        RowLayout {
            Button { text: qsTr("Bağlantıyı Test Et"); enabled: !backend.busy; onClicked: backend.test() }
            Button { text: qsTr("Kuralları Temizle"); enabled: !backend.busy; onClicked: backend.cleanup() }
            CheckBox { text: qsTr("Sistem açılışında çalıştır"); onToggled: backend.setAutostart(checked) }
            BusyIndicator { running: backend.busy; visible: running }
        }
        GroupBox {
            title: qsTr("Son durum"); Layout.fillWidth: true
            Label { text: backend.message; wrapMode: Text.Wrap; width: parent.width }
        }
        GroupBox {
            title: qsTr("Günlükler (içerik veya trafik kaydedilmez)"); Layout.fillWidth: true; Layout.fillHeight: true
            ScrollView { anchors.fill: parent; TextArea { text: backend.logs; readOnly: true; wrapMode: TextEdit.Wrap } }
        }
    }
}
