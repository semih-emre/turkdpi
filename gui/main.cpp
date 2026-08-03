#include "backend.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QUrl>

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("TurkDPI"));
    app.setOrganizationName(QStringLiteral("TurkDPI"));
    app.setApplicationVersion(QStringLiteral(TURKDPI_VERSION));
    Backend backend;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.load(QUrl(QStringLiteral("qrc:/qt/qml/TurkDPI/Main.qml")));
    if (engine.rootObjects().isEmpty()) return 1;
    return app.exec();
}
