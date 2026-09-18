#pragma once

#include <QNetworkAccessManager>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QTimer>

/// QML ile `turkdpi-service` arasındaki köprü.
///
/// Servis kısa ömürlü komutlar hâlinde çalıştırılıyor ve sonuç `status.json`
/// üzerinden okunuyor. Bu tasarım, arayüz kapansa bile motorun çalışmaya devam
/// etmesini sağlıyor — kullanıcı pencereyi kapattığında Discord'un kesilmemesi
/// gerekiyor.
class Backend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool active READ active NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool elevated READ elevated NOTIFY statusChanged)
    Q_PROPERTY(bool engineReady READ engineReady NOTIFY statusChanged)
    Q_PROPERTY(QString strategy READ strategy NOTIFY statusChanged)
    Q_PROPERTY(QString message READ message NOTIFY statusChanged)
    Q_PROPERTY(QString method READ method NOTIFY statusChanged)
    Q_PROPERTY(QString engineName READ engineName NOTIFY statusChanged)
    Q_PROPERTY(QString diagnosis READ diagnosis NOTIFY statusChanged)
    Q_PROPERTY(QString proxyAddress READ proxyAddress NOTIFY statusChanged)
    Q_PROPERTY(QString network READ network NOTIFY networkChanged)
    Q_PROPERTY(QString progress READ progress NOTIFY progressChanged)
    Q_PROPERTY(QString logs READ logs NOTIFY logsChanged)
    Q_PROPERTY(bool checkingUpdates READ checkingUpdates NOTIFY updateChanged)
    Q_PROPERTY(bool updateAvailable READ updateAvailable NOTIFY updateChanged)
    Q_PROPERTY(QString currentVersion READ currentVersion CONSTANT)
    Q_PROPERTY(QString latestVersion READ latestVersion NOTIFY updateChanged)
    Q_PROPERTY(QString updateStatus READ updateStatus NOTIFY updateChanged)

public:
    explicit Backend(QObject *parent = nullptr);

    bool active() const { return m_active; }
    bool busy() const { return m_busy; }
    bool elevated() const { return m_elevated; }
    bool engineReady() const { return m_engineReady; }
    QString strategy() const { return m_strategy; }
    QString message() const { return m_message; }
    QString method() const { return m_method; }
    QString engineName() const { return m_engineName; }
    QString diagnosis() const { return m_diagnosis; }
    QString proxyAddress() const { return m_proxyAddress; }
    QString network() const { return m_network; }
    QString progress() const { return m_progress; }
    QString logs() const { return m_logs; }
    bool checkingUpdates() const { return m_checkingUpdates; }
    bool updateAvailable() const { return m_updateAvailable; }
    QString currentVersion() const { return m_currentVersion; }
    QString latestVersion() const { return m_latestVersion; }
    QString updateStatus() const { return m_updateStatus; }

    /// Ağı teşhis eder, çalışan stratejiyi arar ve uygular.
    Q_INVOKABLE void autoFix();
    Q_INVOKABLE void stop();
    Q_INVOKABLE void test();
    Q_INVOKABLE void diagnose();
    Q_INVOKABLE void installEngine();
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void copyProxyAddress();
    /// Günlük dizinini sistemin dosya yöneticisinde açar. Kullanıcının hata
    /// bildirirken göndereceği dosya orada.
    Q_INVOKABLE void openLogFolder();
    Q_INVOKABLE void checkForUpdates();
    Q_INVOKABLE void installUpdate();

signals:
    void statusChanged();
    void busyChanged();
    void networkChanged();
    void progressChanged();
    void logsChanged();
    void operationFailed(const QString &message);
    void updateChanged();

private:
    /// Servisi çalıştırır. `needsPrivilege` yalnızca Linux'ta Polkit'i devreye
    /// sokuyor; Windows ve macOS'ta yükseltme uygulamanın kendi yetkisinden
    /// geliyor, çünkü proxy modu zaten yetkisiz çalışıyor.
    void runService(const QStringList &arguments, bool needsPrivilege);
    void readStatusFile();
    void detectEnvironment();
    void refreshNetwork();
    void appendLog(const QString &line);
    void setBusy(bool busy);
    void beginUpdateCheck(bool manual);

    bool m_active = false;
    bool m_busy = false;
    bool m_elevated = false;
    bool m_engineReady = false;
    bool m_checkingUpdates = false;
    bool m_updateAvailable = false;
    QString m_strategy = QStringLiteral("—");
    QString m_message = QStringLiteral("Durum bekleniyor");
    QString m_method = QStringLiteral("—");
    QString m_engineName;
    QString m_diagnosis;
    QString m_proxyAddress;
    QString m_network = QStringLiteral("Bağlantı yok");
    QString m_progress;
    QString m_logs;
    QString m_currentVersion;
    QString m_latestVersion;
    QString m_updateStatus = QStringLiteral("Güncelleme henüz denetlenmedi");
    QString m_notifiedVersion;
    QProcess m_process;
    QTimer m_timer;
    QTimer m_updateTimer;
    QNetworkAccessManager m_updateNetwork;
};
