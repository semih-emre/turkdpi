#include "backend.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("TurkDPI"));
    app.setOrganizationName(QStringLiteral("TurkDPI"));
    Backend backend;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("backend"), &backend);
    engine.loadFromModule(QStringLiteral("TurkDPI"), QStringLiteral("Main"));
    if (engine.rootObjects().isEmpty()) return 1;
    return app.exec();
}
