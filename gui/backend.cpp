#include "backend.h"

#include <QCoreApplication>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QClipboard>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QStandardPaths>
#include <QUdpSocket>

namespace {

constexpr auto kVersionUrl =
    "https://raw.githubusercontent.com/semih-emre/turkdpi/main/version.json";

/// Servis ikilisinin yolu.
///
/// Linux'ta paket tarafından `/usr/bin` altına kuruluyor. Windows ve macOS'ta
/// uygulama kendi dizininden taşınabilir şekilde çalışabildiği için önce
/// arayüzün yanına bakılıyor.
QString servicePath()
{
#ifdef Q_OS_WIN
    const QString name = QStringLiteral("turkdpi-service.exe");
#else
    const QString name = QStringLiteral("turkdpi-service");
#endif
    const QString beside = QDir(QCoreApplication::applicationDirPath()).filePath(name);
    if (QFileInfo::exists(beside)) {
        return beside;
    }
#ifdef Q_OS_LINUX
    return QStringLiteral("/usr/bin/turkdpi-service");
#else
    return beside;
#endif
}

/// `status.json` konumu. `src/paths.rs` ile aynı kuralları izliyor; ikisi
/// birlikte değiştirilmeli.
QString statusPath()
{
#if defined(Q_OS_WIN)
    QString base = qEnvironmentVariable("ProgramData", QStringLiteral("C:/ProgramData"));
    return QDir(base).filePath(QStringLiteral("TurkDPI/run/status.json"));
#elif defined(Q_OS_MACOS)
    return QStringLiteral("/var/run/turkdpi/status.json");
#else
    return QStringLiteral("/run/turkdpi/status.json");
#endif
}

/// Günlük dizini. `src/paths.rs` ile aynı kuralları izliyor.
QString logDirectory()
{
#if defined(Q_OS_WIN)
    QString base = qEnvironmentVariable("ProgramData", QStringLiteral("C:/ProgramData"));
    return QDir(base).filePath(QStringLiteral("TurkDPI/logs"));
#elif defined(Q_OS_MACOS)
    return QStringLiteral("/Library/Application Support/TurkDPI/logs");
#else
    return QStringLiteral("/var/lib/turkdpi/logs");
#endif
}

/// Sürüm dizgilerini sayısal olarak karşılaştırır.
///
/// Metin karşılaştırması "0.10.0" sürümünü "0.9.0"dan küçük sayardı; bu da
/// kullanıcıları güncellemeden mahrum bırakırdı.
bool isNewer(const QString &candidate, const QString &current)
{
    const QStringList left = candidate.split('.');
    const QStringList right = current.split('.');
    for (int index = 0; index < qMax(left.size(), right.size()); ++index) {
        const int a = index < left.size() ? left.at(index).toInt() : 0;
        const int b = index < right.size() ? right.at(index).toInt() : 0;
        if (a != b) {
            return a > b;
        }
    }
    return false;
}

} // namespace

Backend::Backend(QObject *parent)
    : QObject(parent)
{
    m_currentVersion = QStringLiteral(TURKDPI_VERSION);

    connect(&m_process, &QProcess::readyReadStandardOutput, this, [this] {
        const QString chunk = QString::fromUtf8(m_process.readAllStandardOutput());
        for (const QString &line : chunk.split('\n', Qt::SkipEmptyParts)) {
            appendLog(line.trimmed());
        }
        m_progress = chunk.trimmed().section('\n', -1);
        emit progressChanged();
    });
    connect(&m_process, &QProcess::readyReadStandardError, this, [this] {
        const QString chunk = QString::fromUtf8(m_process.readAllStandardError());
        if (!chunk.trimmed().isEmpty()) {
            appendLog(chunk.trimmed());
        }
    });
    connect(&m_process, &QProcess::finished, this,
            [this](int exitCode, QProcess::ExitStatus) {
                setBusy(false);
                m_progress.clear();
                emit progressChanged();
                readStatusFile();
                detectEnvironment();
                if (exitCode != 0) {
                    emit operationFailed(m_logs.section('\n', -3));
                }
            });

    // Durum dosyası servis tarafından yazılıyor; arayüz onu düzenli okuyor.
    // Böylece komut satırından yapılan değişiklikler de arayüze yansıyor.
    connect(&m_timer, &QTimer::timeout, this, [this] {
        readStatusFile();
        refreshNetwork();
    });
    m_timer.start(2000);

    connect(&m_updateTimer, &QTimer::timeout, this, [this] { beginUpdateCheck(false); });
    m_updateTimer.start(6 * 60 * 60 * 1000);

    readStatusFile();
    refreshNetwork();
    detectEnvironment();
    beginUpdateCheck(false);
}

void Backend::setBusy(bool busy)
{
    if (m_busy != busy) {
        m_busy = busy;
        emit busyChanged();
    }
}

void Backend::appendLog(const QString &line)
{
    if (line.isEmpty()) {
        return;
    }
    m_logs += line + '\n';
    // Günlük sınırsız büyürse bellek ve arayüz performansı bozuluyor.
    if (m_logs.size() > 20000) {
        m_logs = m_logs.right(15000);
    }
    emit logsChanged();
}

void Backend::runService(const QStringList &arguments, bool needsPrivilege)
{
    if (m_process.state() != QProcess::NotRunning) {
        emit operationFailed(tr("Önceki işlem hâlâ sürüyor."));
        return;
    }
    const QString service = servicePath();
    if (!QFileInfo::exists(service)) {
        emit operationFailed(tr("Servis bulunamadı: %1").arg(service));
        return;
    }

    QString program = service;
    QStringList finalArguments = arguments;

#ifdef Q_OS_LINUX
    // Linux'ta yetki Polkit üzerinden alınıyor; kurulum bir .policy dosyası
    // getiriyor ve kullanıcıya standart sistem penceresi gösteriliyor.
    if (needsPrivilege && !m_elevated) {
        program = QStringLiteral("/usr/bin/pkexec");
        finalArguments.prepend(service);
    }
#else
    // Windows ve macOS'ta yükseltme uygulamanın kendi yetkisinden geliyor.
    // Yetki yoksa servis zaten proxy moduna düşüyor, bu yüzden burada
    // kullanıcıyı engellemiyoruz.
    Q_UNUSED(needsPrivilege);
#endif

    appendLog(QStringLiteral("$ %1 %2").arg(QFileInfo(program).fileName(),
                                            finalArguments.join(' ')));
    setBusy(true);
    m_process.start(program, finalArguments);
}

void Backend::readStatusFile()
{
    QFile file(statusPath());
    if (!file.open(QIODevice::ReadOnly)) {
        return;
    }
    const QJsonObject status = QJsonDocument::fromJson(file.readAll()).object();
    if (status.isEmpty()) {
        return;
    }

    m_active = status.value(QStringLiteral("active")).toBool();
    m_strategy = status.value(QStringLiteral("profile")).toString(QStringLiteral("—"));
    m_message = status.value(QStringLiteral("message")).toString();
    m_method = status.value(QStringLiteral("method")).toString(QStringLiteral("—"));
    m_engineName = status.value(QStringLiteral("backend")).toString();
    m_diagnosis = status.value(QStringLiteral("diagnosis")).toString();
    m_proxyAddress = status.value(QStringLiteral("proxy")).toString();
    emit statusChanged();
}

void Backend::detectEnvironment()
{
    // Ayrı, kısa ömürlü bir süreç: ana süreç uzun süren bir arama
    // yürütüyor olabilir ve onu bloklamak istemiyoruz.
    auto *probe = new QProcess(this);
    connect(probe, &QProcess::finished, this,
            [this, probe](int, QProcess::ExitStatus) {
                const QJsonObject info =
                    QJsonDocument::fromJson(probe->readAllStandardOutput()).object();
                m_elevated = info.value(QStringLiteral("elevated")).toBool();
                m_engineReady = !info.value(QStringLiteral("selected")).isNull()
                    && info.value(QStringLiteral("selected")).isString();
                emit statusChanged();
                probe->deleteLater();
            });
    probe->start(servicePath(), {QStringLiteral("info")});
}

void Backend::refreshNetwork()
{
    // Dışarı çıkan arayüzün adresi; bağlantı değişikliğini böyle fark ediyoruz.
    QUdpSocket socket;
    socket.connectToHost(QStringLiteral("1.1.1.1"), 53);
    const QString address =
        socket.localAddress().isNull() ? QString() : socket.localAddress().toString();
    const QString label = address.isEmpty() ? tr("Bağlantı yok") : address;
    if (label != m_network) {
        m_network = label;
        emit networkChanged();
    }
}

void Backend::autoFix()
{
    m_logs.clear();
    emit logsChanged();
    runService({QStringLiteral("auto")}, true);
}

void Backend::stop() { runService({QStringLiteral("stop")}, true); }
void Backend::test() { runService({QStringLiteral("test")}, false); }
void Backend::diagnose() { runService({QStringLiteral("diagnose")}, false); }
void Backend::installEngine() { runService({QStringLiteral("install-engine")}, false); }

void Backend::refresh()
{
    readStatusFile();
    refreshNetwork();
    detectEnvironment();
}

void Backend::copyProxyAddress()
{
    if (!m_proxyAddress.isEmpty()) {
        QGuiApplication::clipboard()->setText(m_proxyAddress);
    }
}

void Backend::openLogFolder()
{
    const QString directory = logDirectory();
    // Dizin henüz oluşmamış olabilir (hiç komut çalıştırılmadıysa); açmadan
    // önce oluşturuyoruz ki kullanıcı boş bir hata penceresiyle karşılaşmasın.
    QDir().mkpath(directory);
    if (!QDesktopServices::openUrl(QUrl::fromLocalFile(directory))) {
        emit operationFailed(tr("Günlük dizini açılamadı. Konum:\n%1").arg(directory));
    }
}

void Backend::checkForUpdates() { beginUpdateCheck(true); }

void Backend::beginUpdateCheck(bool manual)
{
    if (m_checkingUpdates) {
        return;
    }
    m_checkingUpdates = true;
    if (manual) {
        m_updateStatus = tr("Güncellemeler denetleniyor…");
    }
    emit updateChanged();

    QNetworkRequest request{QUrl(QString::fromLatin1(kVersionUrl))};
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    auto *reply = m_updateNetwork.get(request);
    connect(reply, &QNetworkReply::finished, this, [this, reply, manual] {
        m_checkingUpdates = false;
        if (reply->error() != QNetworkReply::NoError) {
            if (manual) {
                m_updateStatus = tr("Güncelleme denetimi başarısız: %1").arg(reply->errorString());
            }
            emit updateChanged();
            reply->deleteLater();
            return;
        }
        const QJsonObject manifest = QJsonDocument::fromJson(reply->readAll()).object();
        m_latestVersion = manifest.value(QStringLiteral("version")).toString();
        const QString notes = manifest.value(QStringLiteral("notes")).toString();
        m_updateAvailable = isNewer(m_latestVersion, m_currentVersion);
        m_updateStatus = m_updateAvailable
            ? tr("Yeni sürüm %1 hazır. %2").arg(m_latestVersion, notes)
            : tr("En güncel sürümü kullanıyorsunuz (%1).").arg(m_currentVersion);
        emit updateChanged();
        reply->deleteLater();
    });
}

void Backend::installUpdate()
{
    if (!m_updateAvailable) {
        return;
    }
#ifdef Q_OS_LINUX
    // Linux'ta güncelleme kaynaktan derlenip paket yöneticisiyle kuruluyor.
    auto *updater = new QProcess(this);
    updater->setProgram(QStringLiteral("/usr/bin/turkdpi-update"));
    updater->setArguments({m_latestVersion});
    connect(updater, &QProcess::finished, this, [this, updater](int code, QProcess::ExitStatus) {
        if (code != 0) {
            emit operationFailed(tr("Güncelleme başarısız oldu."));
        } else {
            m_updateStatus = tr("Güncelleme tamamlandı. Uygulamayı yeniden başlatın.");
            emit updateChanged();
        }
        updater->deleteLater();
    });
    updater->start();
#else
    // Windows ve macOS'ta kurulum paketi sürüm sayfasından indiriliyor.
    emit operationFailed(
        tr("Yeni sürümü indirin:\n"
           "https://github.com/semih-emre/turkdpi/releases/tag/v%1")
            .arg(m_latestVersion));
#endif
}
