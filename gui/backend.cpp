#include "backend.h"
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusVariant>
#include <QDateTime>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>

static const QString helper = QStringLiteral("/usr/bin/turkdpi-service");

Backend::Backend(QObject *parent) : QObject(parent) {
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
    static const QStringList allowed{QStringLiteral("safe"), QStringLiteral("balanced"), QStringLiteral("discord"), QStringLiteral("aggressive")};
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
        emit statusChanged();
    }
    refreshNetwork();
}
