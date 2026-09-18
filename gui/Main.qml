import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

/// TurkDPI ana penceresi.
///
/// Denetimler bilerek `Controls.Basic` üzerine kendi stiliyle kuruluyor.
/// Material ya da Fusion kullanmak, arayüzün platforma göre farklı görünmesine
/// ve bazı dağıtımlarda ilgili QML modülü paketlenmediği için hiç açılmamasına
/// yol açıyordu.
ApplicationWindow {
    id: window
    width: 980
    height: 720
    minimumWidth: 720
    minimumHeight: 560
    visible: true
    title: qsTr("TurkDPI")
    color: theme.background

    // ---- Tasarım belirteçleri ------------------------------------------
    QtObject {
        id: theme
        readonly property color background: "#0e1116"
        readonly property color surface: "#161b22"
        readonly property color surfaceHover: "#1d232c"
        readonly property color border: "#262d38"
        readonly property color text: "#e6edf3"
        readonly property color textMuted: "#8b97a6"
        readonly property color accent: "#3b82f6"
        readonly property color accentHover: "#2f6fe0"
        readonly property color success: "#2ea043"
        readonly property color warning: "#d29922"
        readonly property color danger: "#f85149"
        readonly property int radius: 12
    }

    component Card: Rectangle {
        color: theme.surface
        border.color: theme.border
        border.width: 1
        radius: theme.radius
    }

    component Primary: Button {
        id: primaryButton
        implicitHeight: 44
        padding: 18
        contentItem: Text {
            text: primaryButton.text
            color: "#ffffff"
            font.pixelSize: 15
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 10
            color: !primaryButton.enabled ? theme.border
                 : primaryButton.down ? theme.accentHover
                 : primaryButton.hovered ? theme.accentHover : theme.accent
            Behavior on color { ColorAnimation { duration: 120 } }
        }
    }

    component Secondary: Button {
        id: secondaryButton
        implicitHeight: 38
        padding: 14
        contentItem: Text {
            text: secondaryButton.text
            color: secondaryButton.enabled ? theme.text : theme.textMuted
            font.pixelSize: 13
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 9
            color: secondaryButton.down || secondaryButton.hovered ? theme.surfaceHover
                                                                   : "transparent"
            border.color: theme.border
            border.width: 1
        }
    }

    /// Etiket + değer gösteren küçük bilgi hücresi.
    component Stat: ColumnLayout {
        property string label
        property string value
        property color valueColor: theme.text
        spacing: 4
        Text {
            text: parent.label
            color: theme.textMuted
            font.pixelSize: 11
            font.letterSpacing: 0.6
        }
        Text {
            text: parent.value
            color: parent.valueColor
            font.pixelSize: 15
            font.bold: true
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
    }

    Connections {
        target: backend
        function onOperationFailed(message) {
            errorLabel.text = message
            errorDialog.open()
        }
    }

    Dialog {
        id: errorDialog
        anchors.centerIn: parent
        width: Math.min(560, window.width - 80)
        modal: true
        title: qsTr("İşlem tamamlanamadı")
        standardButtons: Dialog.Ok
        background: Card {}
        contentItem: Text {
            id: errorLabel
            color: theme.text
            wrapMode: Text.Wrap
            font.pixelSize: 13
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 16

            // ---- Başlık ------------------------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 24
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                spacing: 12

                ColumnLayout {
                    spacing: 2
                    Layout.fillWidth: true
                    Text {
                        text: qsTr("TurkDPI")
                        color: theme.text
                        font.pixelSize: 26
                        font.bold: true
                    }
                    Text {
                        text: qsTr("Engelli servislere VPN'siz erişim")
                        color: theme.textMuted
                        font.pixelSize: 13
                    }
                }
                Text {
                    text: "v" + backend.currentVersion
                    color: theme.textMuted
                    font.pixelSize: 13
                }
            }

            // ---- Güncelleme şeridi -------------------------------------
            Card {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.preferredHeight: 60
                visible: backend.updateAvailable || backend.checkingUpdates
                border.color: backend.updateAvailable ? theme.warning : theme.border

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 12
                    BusyIndicator {
                        running: backend.checkingUpdates
                        visible: running
                        implicitWidth: 22
                        implicitHeight: 22
                    }
                    Text {
                        text: backend.updateStatus
                        color: backend.updateAvailable ? theme.warning : theme.textMuted
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Secondary {
                        text: qsTr("Güncelle")
                        visible: backend.updateAvailable
                        onClicked: backend.installUpdate()
                    }
                }
            }

            // ---- Ana durum kartı ---------------------------------------
            Card {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.preferredHeight: mainColumn.implicitHeight + 40

                ColumnLayout {
                    id: mainColumn
                    anchors.fill: parent
                    anchors.margins: 20
                    spacing: 18

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 14

                        Rectangle {
                            width: 12
                            height: 12
                            radius: 6
                            color: backend.active ? theme.success : theme.textMuted
                            SequentialAnimation on opacity {
                                running: backend.busy
                                loops: Animation.Infinite
                                NumberAnimation { to: 0.3; duration: 600 }
                                NumberAnimation { to: 1.0; duration: 600 }
                            }
                        }

                        ColumnLayout {
                            spacing: 3
                            Layout.fillWidth: true
                            Text {
                                text: backend.busy ? qsTr("Çalışıyor…")
                                    : backend.active ? qsTr("Koruma etkin")
                                                     : qsTr("Koruma kapalı")
                                color: theme.text
                                font.pixelSize: 19
                                font.bold: true
                            }
                            Text {
                                text: backend.progress.length > 0 ? backend.progress
                                                                  : backend.message
                                color: theme.textMuted
                                font.pixelSize: 13
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }
                        }

                        Primary {
                            text: backend.active ? qsTr("Durdur") : qsTr("Otomatik Düzelt")
                            enabled: !backend.busy
                            onClicked: backend.active ? backend.stop() : backend.autoFix()
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        height: 1
                        color: theme.border
                    }

                    GridLayout {
                        Layout.fillWidth: true
                        columns: Math.max(1, Math.floor(width / 210))
                        columnSpacing: 20
                        rowSpacing: 16

                        Stat {
                            Layout.fillWidth: true
                            label: qsTr("TEŞHİS")
                            value: backend.diagnosis.length > 0 ? backend.diagnosis
                                                                : qsTr("henüz ölçülmedi")
                            valueColor: backend.diagnosis.length > 0 ? theme.warning : theme.textMuted
                        }
                        Stat {
                            Layout.fillWidth: true
                            label: qsTr("MOTOR")
                            value: backend.method
                        }
                        Stat {
                            Layout.fillWidth: true
                            label: qsTr("STRATEJİ")
                            value: backend.strategy
                        }
                        Stat {
                            Layout.fillWidth: true
                            label: qsTr("AĞ")
                            value: backend.network
                        }
                    }
                }
            }

            // ---- Proxy bilgisi -----------------------------------------
            // Proxy modunda trafik kendiliğinden yönlenmiyor; kullanıcının
            // uygulamayı bu adrese yönlendirmesi gerekiyor. Bu kart yalnızca o
            // durumda görünüyor.
            Card {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.preferredHeight: proxyColumn.implicitHeight + 32
                visible: backend.proxyAddress.length > 0
                border.color: theme.accent

                ColumnLayout {
                    id: proxyColumn
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 8

                    Text {
                        text: qsTr("Proxy modu etkin")
                        color: theme.text
                        font.pixelSize: 15
                        font.bold: true
                    }
                    Text {
                        text: qsTr("Uygulamaları aşağıdaki SOCKS5 adresine yönlendirin. "
                                 + "Tüm trafiğin otomatik kapsanması için uygulamayı yönetici "
                                 + "yetkisiyle çalıştırın.")
                        color: theme.textMuted
                        font.pixelSize: 12
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        spacing: 10
                        Rectangle {
                            radius: 8
                            color: theme.background
                            border.color: theme.border
                            border.width: 1
                            Layout.preferredHeight: 36
                            Layout.preferredWidth: addressText.implicitWidth + 28
                            Text {
                                id: addressText
                                anchors.centerIn: parent
                                text: backend.proxyAddress
                                color: theme.accent
                                font.pixelSize: 14
                                font.family: "monospace"
                            }
                        }
                        Secondary {
                            text: qsTr("Kopyala")
                            onClicked: backend.copyProxyAddress()
                        }
                    }
                }
            }

            // ---- Eylemler ----------------------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                spacing: 10

                Secondary {
                    text: qsTr("Bağlantıyı Test Et")
                    enabled: !backend.busy
                    onClicked: backend.test()
                }
                Secondary {
                    text: qsTr("Ağı Teşhis Et")
                    enabled: !backend.busy
                    onClicked: backend.diagnose()
                }
                Secondary {
                    text: qsTr("Motoru İndir")
                    enabled: !backend.busy && !backend.engineReady
                    onClicked: backend.installEngine()
                }
                Item { Layout.fillWidth: true }
                Secondary {
                    text: qsTr("Günlükleri Aç")
                    onClicked: backend.openLogFolder()
                }
                Secondary {
                    text: qsTr("Güncellemeleri Denetle")
                    enabled: !backend.checkingUpdates
                    onClicked: backend.checkForUpdates()
                }
            }

            // ---- Motor uyarısı -----------------------------------------
            Card {
                Layout.fillWidth: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.preferredHeight: 56
                visible: !backend.engineReady
                border.color: theme.danger

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 14
                    Text {
                        text: qsTr("Motor kurulu değil. \"Motoru İndir\" ile kurabilirsiniz; "
                                 + "indirilen dosya SHA-256 ile doğrulanır.")
                        color: theme.danger
                        font.pixelSize: 12
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
            }

            // ---- Günlükler ---------------------------------------------
            Card {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.leftMargin: 24
                Layout.rightMargin: 24
                Layout.bottomMargin: 24
                Layout.minimumHeight: 200

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 10

                    RowLayout {
                        Layout.fillWidth: true
                        Text {
                            text: qsTr("GÜNLÜK — trafik veya içerik kaydedilmez")
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.letterSpacing: 0.6
                            Layout.fillWidth: true
                        }
                        Text {
                            text: qsTr("diske yazılır, son 3 gün saklanır")
                            color: theme.textMuted
                            font.pixelSize: 11
                        }
                    }
                    ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        TextArea {
                            text: backend.logs
                            readOnly: true
                            color: theme.textMuted
                            font.family: "monospace"
                            font.pixelSize: 12
                            wrapMode: TextEdit.Wrap
                            background: null
                            // Yeni satır geldikçe en alta kaydır.
                            onTextChanged: cursorPosition = length
                        }
                    }
                }
            }
        }
    }
}
