#include "backend.h"
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusVariant>
#include <QDateTime>
#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QRegularExpression>
#include <QUrlQuery>
#include <QVariantMap>
#include <QVersionNumber>

static const QString helper = QStringLiteral("/usr/bin/turkdpi-service");

Backend::Backend(QObject *parent) : QObject(parent), m_currentVersion(QCoreApplication::applicationVersion()) {
    connect(&m_process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        const QString output = QString::fromUtf8(m_process.readAllStandardOutput()).trimmed();
        const QString error = QString::fromUtf8(m_process.readAllStandardError()).trimmed();
        if (!output.isEmpty()) appendLog(output);
        if (status != QProcess::NormalExit || code != 0) {
            const QString detail = error.isEmpty() ? tr("Yetkili işlem başarısız oldu") : error;
            appendLog(detail); emit operationFailed(detail);
        }
        m_busy = false; emit busyChanged(); refresh();
    });
    m_timer.setInterval(2500);
    connect(&m_timer, &QTimer::timeout, this, &Backend::refresh);
    m_timer.start();
    m_updateTimer.setInterval(6 * 60 * 60 * 1000);
    connect(&m_updateTimer, &QTimer::timeout, this, [this]() { beginUpdateCheck(false); });
    m_updateTimer.start();
    QTimer::singleShot(5000, this, [this]() { beginUpdateCheck(false); });
    refresh();
}

void Backend::appendLog(const QString &line) {
    m_logs += QDateTime::currentDateTime().toString(QStringLiteral("HH:mm:ss ")) + line + QLatin1Char('\n');
    if (m_logs.size() > 12000) m_logs = m_logs.right(12000);
    emit logsChanged();
}

void Backend::privileged(const QStringList &arguments) {
    if (m_busy) return;
    m_busy = true; emit busyChanged();
    appendLog(helper + QLatin1Char(' ') + arguments.join(QLatin1Char(' ')));
    m_process.start(QStringLiteral("/usr/bin/pkexec"), QStringList{helper} + arguments);
}

void Backend::start(const QString &profile) {
    static const QStringList allowed{QStringLiteral("safe"), QStringLiteral("balanced"), QStringLiteral("discord"), QStringLiteral("roblox"), QStringLiteral("general"), QStringLiteral("aggressive")};
    if (!allowed.contains(profile)) { emit operationFailed(tr("Geçersiz profil")); return; }
    privileged({QStringLiteral("start"), profile});
}
void Backend::stop() { privileged({QStringLiteral("stop")}); }
void Backend::cleanup() { privileged({QStringLiteral("cleanup")}); }
void Backend::autoSelect() { privileged({QStringLiteral("auto")}); }

void Backend::test() {
    if (m_busy) return;
    m_busy = true; emit busyChanged();
    m_process.start(helper, {QStringLiteral("test")});
}

void Backend::setAutostart(bool enabled) {
    if (m_busy) return;
    m_busy = true; emit busyChanged();
    const QString verb = enabled ? QStringLiteral("enable") : QStringLiteral("disable");
    m_process.start(QStringLiteral("/usr/bin/pkexec"), {QStringLiteral("/usr/bin/systemctl"), verb, QStringLiteral("turkdpi.service")});
}

void Backend::setDns(bool cloudflare) {
    privileged({QStringLiteral("set-dns"), cloudflare ? QStringLiteral("cloudflare") : QStringLiteral("automatic")});
}

void Backend::checkForUpdates() { beginUpdateCheck(true); }

void Backend::beginUpdateCheck(bool manual) {
    if (m_checkingUpdates) return;
    m_checkingUpdates = true;
    m_updateStatus = tr("Güncellemeler denetleniyor…");
    emit updateChanged();

    QUrl url(QStringLiteral("https://raw.githubusercontent.com/semih-emre/turkdpi/main/version.json"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("timestamp"), QString::number(QDateTime::currentSecsSinceEpoch()));
    url.setQuery(query);
    QNetworkRequest request(url);
    request.setRawHeader("User-Agent", "TurkDPI-Update-Checker");
    request.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
    request.setTransferTimeout(10000);
    QNetworkReply *reply = m_updateNetwork.get(request);
    connect(reply, &QNetworkReply::finished, this, [this, reply, manual]() {
        m_checkingUpdates = false;
        if (reply->error() != QNetworkReply::NoError) {
            m_updateStatus = tr("Güncelleme denetlenemedi: %1").arg(reply->errorString());
            appendLog(m_updateStatus);
            emit updateChanged();
            reply->deleteLater();
            return;
        }

        QJsonParseError parseError;
        const QJsonDocument document = QJsonDocument::fromJson(reply->readAll(), &parseError);
        reply->deleteLater();
        const QJsonObject object = document.object();
        const QString version = object.value(QStringLiteral("version")).toString();
        const QString notes = object.value(QStringLiteral("notes")).toString();
        static const QRegularExpression versionPattern(QStringLiteral("^[0-9]+\\.[0-9]+\\.[0-9]+$"));
        if (parseError.error != QJsonParseError::NoError || !versionPattern.match(version).hasMatch()) {
            m_updateStatus = tr("Güncelleme bildirimi geçersiz");
            appendLog(m_updateStatus);
            emit updateChanged();
            return;
        }

        m_latestVersion = version;
        m_updateAvailable = QVersionNumber::compare(
            QVersionNumber::fromString(m_latestVersion),
            QVersionNumber::fromString(m_currentVersion)) > 0;
        if (m_updateAvailable) {
            m_updateStatus = tr("Yeni sürüm hazır: %1").arg(m_latestVersion);
            if (m_notifiedVersion != m_latestVersion) {
                notifyUpdate(m_latestVersion, notes);
                m_notifiedVersion = m_latestVersion;
            }
        } else {
            m_updateStatus = tr("TurkDPI güncel (%1)").arg(m_currentVersion);
        }
        if (manual) appendLog(m_updateStatus);
        emit updateChanged();
    });
}

void Backend::notifyUpdate(const QString &version, const QString &notes) {
    QDBusMessage message = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.Notifications"),
        QStringLiteral("/org/freedesktop/Notifications"),
        QStringLiteral("org.freedesktop.Notifications"),
        QStringLiteral("Notify"));
    message << QStringLiteral("TurkDPI") << quint32(0)
            << QStringLiteral("system-software-update")
            << tr("TurkDPI güncellemesi hazır")
            << tr("%1 sürümü yayımlandı. %2").arg(version, notes)
            << QStringList{} << QVariantMap{} << 10000;
    QDBusConnection::sessionBus().asyncCall(message);
}

void Backend::installUpdate() {
    static const QRegularExpression versionPattern(QStringLiteral("^[0-9]+\\.[0-9]+\\.[0-9]+$"));
    if (!m_updateAvailable || !versionPattern.match(m_latestVersion).hasMatch()) {
        emit operationFailed(tr("Kurulabilir bir güncelleme yok"));
        return;
    }
    const bool started = QProcess::startDetached(
        QStringLiteral("/usr/bin/konsole"),
        {QStringLiteral("--hold"), QStringLiteral("-e"),
         QStringLiteral("/usr/lib/turkdpi/turkdpi-update"), m_latestVersion});
    if (!started) emit operationFailed(tr("Güncelleme terminali başlatılamadı"));
}

void Backend::refreshNetwork() {
    QDBusInterface properties(QStringLiteral("org.freedesktop.NetworkManager"), QStringLiteral("/org/freedesktop/NetworkManager"), QStringLiteral("org.freedesktop.DBus.Properties"), QDBusConnection::systemBus());
    const QDBusMessage primaryReply = properties.call(QStringLiteral("Get"), QStringLiteral("org.freedesktop.NetworkManager"), QStringLiteral("PrimaryConnection"));
    if (primaryReply.type() != QDBusMessage::ReplyMessage || primaryReply.arguments().isEmpty()) return;
    const QDBusObjectPath path = qvariant_cast<QDBusVariant>(primaryReply.arguments().first()).variant().value<QDBusObjectPath>();
    QDBusInterface active(QStringLiteral("org.freedesktop.NetworkManager"), path.path(), QStringLiteral("org.freedesktop.DBus.Properties"), QDBusConnection::systemBus());
    const QDBusMessage idReply = active.call(QStringLiteral("Get"), QStringLiteral("org.freedesktop.NetworkManager.Connection.Active"), QStringLiteral("Id"));
    if (idReply.type() == QDBusMessage::ReplyMessage && !idReply.arguments().isEmpty()) {
        const QString next = qvariant_cast<QDBusVariant>(idReply.arguments().first()).variant().toString();
        if (next != m_network) { m_network = next; emit networkChanged(); }
    }
}

void Backend::refresh() {
    QFile file(QStringLiteral("/run/turkdpi/status.json"));
    if (file.open(QIODevice::ReadOnly)) {
        const auto object = QJsonDocument::fromJson(file.readAll()).object();
        m_active = object.value(QStringLiteral("active")).toBool();
        m_profile = object.value(QStringLiteral("profile")).toString(QStringLiteral("none"));
        m_method = object.value(QStringLiteral("method")).toString(QStringLiteral("none"));
        m_message = object.value(QStringLiteral("message")).toString();
        m_dnsCloudflare = object.value(QStringLiteral("dns_cloudflare")).toBool();
        emit statusChanged();
    }
    refreshNetwork();
}
