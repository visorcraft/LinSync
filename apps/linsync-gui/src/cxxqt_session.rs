//! cxx-qt bridge for the bundled C++ UI-translation helper.
//!
//! Historically this module also hosted an in-process `LinSyncSessionBridge`
//! QObject (a future-optimization alternative to the HTTP bridge). It was never
//! registered with the QML engine — both the external `qml6` host and the
//! in-process cxx-qt host drive the UI over the same HTTP bridge — and has been
//! removed as dead code. Only the translator extern remains.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        // Install a QTranslator for the active locale (cxx-qt-lib does not
        // expose QTranslator). Implemented inline in the bundled C++ shim.
        include!("linsync_translator.h");
        #[doc(hidden)]
        fn linsync_install_translator(dir: &QString) -> bool;
    }
}
