#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, difference_count)]
        #[qproperty(QString, label)]
        type CxxQtSmoke = super::CxxQtSmokeRust;

        #[qinvokable]
        fn bump(self: Pin<&mut CxxQtSmoke>);

        #[qinvokable]
        fn bump_on_qt_thread(self: Pin<&mut CxxQtSmoke>);
    }

    impl cxx_qt::Threading for CxxQtSmoke {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct CxxQtSmokeRust {
    difference_count: i32,
    label: QString,
}

impl ffi::CxxQtSmoke {
    pub fn bump(mut self: Pin<&mut Self>) {
        let next = *self.difference_count() + 1;
        self.as_mut().set_difference_count(next);
        self.as_mut().set_label(QString::from("CXX-Qt smoke"));
    }

    pub fn bump_on_qt_thread(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            qt_thread
                .queue(|mut smoke| {
                    smoke.as_mut().bump();
                })
                .expect("Qt thread queue should accept smoke update");
        });
    }
}
