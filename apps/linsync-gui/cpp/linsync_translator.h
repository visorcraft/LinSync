// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Tiny C++ shim that installs a QTranslator for the active locale. cxx-qt-lib
// does not expose QTranslator, so the GUI host calls this free function (via a
// cxx-qt bridge declaration) right after constructing QGuiApplication and
// before loading any QML, so qsTr() strings resolve against the catalog.

#pragma once

#include <QtCore/QCoreApplication>
#include <QtCore/QLocale>
#include <QtCore/QString>
#include <QtCore/QTranslator>

// Load `linsync_<locale>.qm` (e.g. linsync_de.qm) from `dir` for the active
// QLocale and install it on the application. Returns true when a catalog was
// loaded and installed. The translator is parented to the application so it
// lives for the process and is cleaned up on exit.
inline bool linsync_install_translator(const QString &dir) {
    auto *translator = new QTranslator(QCoreApplication::instance());
    if (translator->load(QLocale(), QStringLiteral("linsync"), QStringLiteral("_"),
                          dir)) {
        QCoreApplication::installTranslator(translator);
        return true;
    }
    delete translator;
    return false;
}
