#pragma once

#include <QObject>
#include <QProcess>
#include <QString>
#include <QTimer>

class Backend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool active READ active NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QString profile READ profile NOTIFY statusChanged)
    Q_PROPERTY(QString message READ message NOTIFY statusChanged)
    Q_PROPERTY(QString method READ method NOTIFY statusChanged)
    Q_PROPERTY(QString network READ network NOTIFY networkChanged)
    Q_PROPERTY(QString logs READ logs NOTIFY logsChanged)
    Q_PROPERTY(bool dnsCloudflare READ dnsCloudflare NOTIFY statusChanged)
public:
    explicit Backend(QObject *parent = nullptr);
    bool active() const { return m_active; }
    bool busy() const { return m_busy; }
    QString profile() const { return m_profile; }
    QString message() const { return m_message; }
    QString method() const { return m_method; }
    QString network() const { return m_network; }
    QString logs() const { return m_logs; }
    bool dnsCloudflare() const { return m_dnsCloudflare; }

    Q_INVOKABLE void start(const QString &profile);
    Q_INVOKABLE void stop();
    Q_INVOKABLE void cleanup();
    Q_INVOKABLE void test();
    Q_INVOKABLE void autoSelect();
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void setAutostart(bool enabled);
    Q_INVOKABLE void setDns(bool cloudflare);

signals:
    void statusChanged();
    void busyChanged();
    void networkChanged();
    void logsChanged();
    void operationFailed(const QString &message);

private:
    void privileged(const QStringList &arguments);
    void refreshNetwork();
    void appendLog(const QString &line);
    bool m_active = false;
    bool m_busy = false;
    bool m_dnsCloudflare = false;
    QString m_profile = QStringLiteral("none");
    QString m_message = QStringLiteral("Durum bekleniyor");
    QString m_method = QStringLiteral("none");
    QString m_network = QStringLiteral("Bağlantı yok");
    QString m_logs;
    QProcess m_process;
    QTimer m_timer;
};
