import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    width: 800; height: 760; visible: true
    title: qsTr("Türkiye DPI Yöneticisi")
    color: palette.window
    property string selectedProfile: "discord"
    function profileLabel(value) {
        const labels = {"discord": "Discord", "roblox": "Roblox", "general": "Genel Web", "safe": "Güvenli", "balanced": "Dengeli", "aggressive": "Agresif", "none": "Yok"}
        return labels[value] || value
    }

    Connections { target: backend; function onOperationFailed(message) { errorDialog.text = message; errorDialog.open() } }
    Dialog { id: errorDialog; title: qsTr("İşlem başarısız"); standardButtons: Dialog.Ok; property alias text: errorLabel.text; Label { id: errorLabel; wrapMode: Text.Wrap } }

    ColumnLayout {
        anchors.fill: parent; anchors.margins: 24; spacing: 16
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Türkiye DPI Yöneticisi"); font.pixelSize: 28; font.bold: true; Layout.fillWidth: true }
            Label { text: "v" + backend.currentVersion; color: palette.placeholderText }
        }
        Frame {
            Layout.fillWidth: true
            RowLayout {
                anchors.fill: parent
                Label {
                    text: backend.updateStatus
                    color: backend.updateAvailable ? "#f39c12" : palette.text
                    font.bold: backend.updateAvailable
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                }
                BusyIndicator { running: backend.checkingUpdates; visible: running; implicitWidth: 28; implicitHeight: 28 }
                Button { text: qsTr("Güncellemeleri Denetle"); enabled: !backend.checkingUpdates; onClicked: backend.checkForUpdates() }
                Button { text: qsTr("Güncelle"); visible: backend.updateAvailable; enabled: !backend.checkingUpdates; highlighted: true; onClicked: backend.installUpdate() }
            }
        }
        GridLayout {
            columns: 2; columnSpacing: 24; rowSpacing: 8; Layout.fillWidth: true
            Label { text: qsTr("Durum:"); font.bold: true }
            Label { text: backend.active ? qsTr("● Aktif") : qsTr("○ Pasif"); color: backend.active ? "#27ae60" : palette.text }
            Label { text: qsTr("Bağlantı:"); font.bold: true }
            Label { text: backend.network }
            Label { text: qsTr("Aktif profil:"); font.bold: true }
            Label { text: window.profileLabel(backend.profile) }
            Label { text: qsTr("Çalışan yöntem:"); font.bold: true }
            Label { text: backend.method; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
        RowLayout {
            Label { text: qsTr("Profil") }
            ComboBox {
                id: profiles
                textRole: "label"
                valueRole: "value"
                model: ListModel {
                    ListElement { label: "Discord"; value: "discord" }
                    ListElement { label: "Roblox"; value: "roblox" }
                    ListElement { label: "Genel Web"; value: "general" }
                    ListElement { label: "Güvenli"; value: "safe" }
                    ListElement { label: "Dengeli"; value: "balanced" }
                    ListElement { label: "Agresif"; value: "aggressive" }
                }
                onCurrentValueChanged: window.selectedProfile = currentValue
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
        RowLayout {
            CheckBox {
                text: qsTr("Cloudflare DNS (1.1.1.1 / 1.0.0.1)")
                checked: backend.dnsCloudflare
                enabled: !backend.busy
                onClicked: backend.setDns(checked)
            }
            Label { text: qsTr("Kapatıldığında önceki NetworkManager DNS ayarı geri yüklenir."); wrapMode: Text.Wrap }
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
