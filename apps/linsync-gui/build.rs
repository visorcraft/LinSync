#[cfg(feature = "cxxqt")]
fn main() {
    use cxx_qt_build::{CxxQtBuilder, QmlModule};
    use qt_build_utils::{QResource, QResourceFile, QResources};

    let mut module = QmlModule::new("com.visorcraft.LinSync");
    let mut rust_sources = Vec::new();

    if cfg!(feature = "cxxqt-app") {
        module = module.qml_files([
            "qml/Main.qml",
            "qml/DesignTokens.qml",
            "qml/AboutPage.qml",
            "qml/AppCheckBox.qml",
            "qml/AppComboBox.qml",
            "qml/AppSpinBox.qml",
            "qml/AppTextField.qml",
            "qml/Card.qml",
            "qml/CreditsPage.qml",
            "qml/DocumentComparePage.qml",
            "qml/FiltersPage.qml",
            "qml/GplLicenseText.qml",
            "qml/LicensesPage.qml",
            "qml/LinSyncNavItem.qml",
            "qml/PluginsPage.qml",
            "qml/SessionsPage.qml",
            "qml/SettingsPage.qml",
        ]);
        rust_sources.push("src/cxxqt_session.rs");
    }

    if cfg!(feature = "cxxqt-smoke") {
        module = module.qml_file("qml/CxxQtSmoke.qml");
        rust_sources.push("src/cxxqt_smoke.rs");
    }

    CxxQtBuilder::new_qml_module(module)
        .qrc_resources(
            QResources::new().resource(
                QResource::new()
                    .prefix("/qt/qml/io/visorcraft/LinSync")
                    .file(
                        QResourceFile::new("qml/assets/com.visorcraft.LinSync.png")
                            .alias("assets/com.visorcraft.LinSync.png"),
                    )
                    .file(
                        QResourceFile::new("qml/assets/com.visorcraft.LinSync.png")
                            .alias("qml/assets/com.visorcraft.LinSync.png"),
                    ),
            ),
        )
        .files(rust_sources)
        .build();
}

#[cfg(not(feature = "cxxqt"))]
fn main() {}
