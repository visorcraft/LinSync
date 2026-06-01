//! In-process cxx-qt session transport (`LinSyncSessionBridge`).
//!
//! **Host parity (Phase 3).** This QObject is an intentionally-partial,
//! future-optimization transport. It is currently **not** registered with the
//! QML engine: `run_cxxqt_host` starts the HTTP bridge and `Main.qml`'s
//! `sessionBridge` stays `null`, so *both* the external `qml6` host and the
//! in-process cxx-qt host drive the UI over the same HTTP bridge. Host parity
//! is therefore satisfied by construction — the two hosts run identical code
//! over one transport — and operations not exposed here (filters, plugins,
//! folder ops, the per-mode `/compare/*` routes, …) are intentionally served
//! by the HTTP bridge through the QML's `hasSessionBridge()` fallback rather
//! than duplicated as qinvokables. The qinvokables below mirror the HTTP
//! handlers (several delegate straight into them) so that, if this transport is
//! ever wired in, its compare / profile / settings / merge surface already
//! matches HTTP. The shared `build_tab_for_paths_with_mode` keeps `compare_paths`
//! and HTTP `/compare` in lock-step (see `http_route_and_shared_builder_agree_on_compare`).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qbytearray.h");
        type QByteArray = cxx_qt_lib::QByteArray;
        include!("cxx-qt-lib/qhash_i32_QByteArray.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, active_tab_id)]
        #[qproperty(bool, can_redo)]
        #[qproperty(bool, can_undo)]
        #[qproperty(QString, context_json)]
        #[qproperty(i32, difference_count)]
        #[qproperty(QString, last_error)]
        #[qproperty(bool, left_dirty)]
        #[qproperty(QString, left_path)]
        #[qproperty(QString, compare_mode)]
        #[qproperty(bool, right_dirty)]
        #[qproperty(QString, right_path)]
        #[qproperty(QString, status)]
        #[qproperty(i32, tab_count)]
        #[qproperty(bool, validation_compatible)]
        #[qproperty(QString, validation_message)]
        #[qproperty(QString, validation_path_kind)]
        type LinSyncSessionBridge = super::LinSyncSessionBridgeRust;

        #[qinvokable]
        fn session_json(self: &LinSyncSessionBridge) -> QString;

        #[qinvokable]
        fn tab_id_at(self: &LinSyncSessionBridge, index: i32) -> i32;

        #[qinvokable]
        fn tab_title_at(self: &LinSyncSessionBridge, index: i32) -> QString;

        #[qinvokable]
        fn tab_dirty_at(self: &LinSyncSessionBridge, index: i32) -> bool;

        #[qinvokable]
        fn tab_can_undo_at(self: &LinSyncSessionBridge, index: i32) -> bool;

        #[qinvokable]
        fn tab_can_redo_at(self: &LinSyncSessionBridge, index: i32) -> bool;

        #[qinvokable]
        fn recent_path_count(self: &LinSyncSessionBridge) -> i32;

        #[qinvokable]
        fn recent_path_at(self: &LinSyncSessionBridge, index: i32) -> QString;

        #[qinvokable]
        fn summary_count(self: &LinSyncSessionBridge) -> i32;

        #[qinvokable]
        fn summary_label_at(self: &LinSyncSessionBridge, index: i32) -> QString;

        #[qinvokable]
        fn summary_value_at(self: &LinSyncSessionBridge, index: i32) -> QString;

        #[qinvokable]
        fn load_context_file(self: Pin<&mut LinSyncSessionBridge>, path: &QString) -> QString;

        #[qinvokable]
        fn compare_paths(
            self: Pin<&mut LinSyncSessionBridge>,
            left: &QString,
            right: &QString,
            mode: &QString,
            new_tab: bool,
        ) -> QString;

        #[qinvokable]
        fn profile_list(self: &LinSyncSessionBridge) -> QString;

        #[qinvokable]
        fn profile_active_get(self: &LinSyncSessionBridge) -> QString;

        #[qinvokable]
        fn profile_active_set(self: &LinSyncSessionBridge, id: &QString) -> QString;

        #[qinvokable]
        fn activate_tab(self: Pin<&mut LinSyncSessionBridge>, tab_id: i32) -> QString;

        #[qinvokable]
        fn close_tab(self: Pin<&mut LinSyncSessionBridge>, tab_id: i32) -> QString;

        #[qinvokable]
        fn copy_current_row(
            self: Pin<&mut LinSyncSessionBridge>,
            row: i32,
            direction: &QString,
        ) -> QString;

        #[qinvokable]
        fn copy_all(self: Pin<&mut LinSyncSessionBridge>, direction: &QString) -> QString;

        #[qinvokable]
        fn undo(self: Pin<&mut LinSyncSessionBridge>) -> QString;

        #[qinvokable]
        fn redo(self: Pin<&mut LinSyncSessionBridge>) -> QString;

        #[qinvokable]
        fn save_side(self: Pin<&mut LinSyncSessionBridge>, side: &QString) -> QString;

        #[qinvokable]
        fn load_settings(self: Pin<&mut LinSyncSessionBridge>) -> QString;

        #[qinvokable]
        fn save_setting(
            self: Pin<&mut LinSyncSessionBridge>,
            key: &QString,
            value: &QString,
        ) -> QString;

        #[qinvokable]
        fn reset_settings(self: Pin<&mut LinSyncSessionBridge>) -> QString;

        #[qinvokable]
        fn start_three_way_merge(
            self: Pin<&mut LinSyncSessionBridge>,
            base: &QString,
            left: &QString,
            right: &QString,
        ) -> QString;

        #[qinvokable]
        fn resolve_three_way_conflict(
            self: Pin<&mut LinSyncSessionBridge>,
            id: i32,
            choice: &QString,
            text: &QString,
        ) -> QString;

        #[qinvokable]
        fn save_three_way_merge(self: Pin<&mut LinSyncSessionBridge>, path: &QString) -> QString;
    }

    unsafe extern "RustQt" {
        #[rust_name = "row_count"]
        #[qinvokable]
        #[cxx_override]
        fn rowCount(self: &LinSyncSessionBridge, parent: &QModelIndex) -> i32;

        #[rust_name = "model_data"]
        #[qinvokable]
        #[cxx_override]
        fn data(self: &LinSyncSessionBridge, index: &QModelIndex, role: i32) -> QVariant;

        #[rust_name = "role_names"]
        #[qinvokable]
        #[cxx_override]
        fn roleNames(self: &LinSyncSessionBridge) -> QHash_i32_QByteArray;
    }

    // Inherited QAbstractItemModel hooks used to notify bound QML views when the
    // active tab's rows are replaced wholesale (compare / undo / redo / copy /
    // save / tab switch). Without these, the view keeps rendering stale rows.
    extern "RustQt" {
        /// # Safety
        ///
        /// Inherited `beginResetModel` from the base class. If you call
        /// `begin_reset_model`, it is your responsibility to ensure
        /// `end_reset_model` is called.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut LinSyncSessionBridge>);

        /// # Safety
        ///
        /// Inherited `endResetModel` from the base class. If you call
        /// `begin_reset_model`, it is your responsibility to ensure
        /// `end_reset_model` is called.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut LinSyncSessionBridge>);
    }
}

use core::pin::Pin;
use std::fs;
use std::path::{Path, PathBuf};

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use linsync_core::AppPaths;

use std::sync::{Arc, Mutex};

use crate::{
    GuiBridgeState, GuiCompareOptions, GuiLaunchContext, GuiLineRow, build_tab_for_paths_with_mode,
    context_to_json, load_gui_settings_json, record_recent_context, reset_gui_settings_json,
    resolve_compare_options_for_request,
    resolve_three_way_conflict as resolve_three_way_conflict_impl, save_gui_setting_json,
    save_three_way_merge_output, start_three_way_merge_session,
};

type RoleNames = QHash<QHashPair_i32_QByteArray>;

const DISPLAY_ROLE: i32 = 0;
const LEFT_ROW_ID_ROLE: i32 = 256;
const LEFT_NUMBER_ROLE: i32 = 257;
const LEFT_TEXT_ROLE: i32 = 258;
const LEFT_STATE_ROLE: i32 = 259;
const RIGHT_ROW_ID_ROLE: i32 = 260;
const RIGHT_NUMBER_ROLE: i32 = 261;
const RIGHT_TEXT_ROLE: i32 = 262;
const RIGHT_STATE_ROLE: i32 = 263;

pub struct LinSyncSessionBridgeRust {
    active_tab_id: i32,
    can_redo: bool,
    can_undo: bool,
    context_json: QString,
    difference_count: i32,
    last_error: QString,
    left_dirty: bool,
    left_path: QString,
    compare_mode: QString,
    right_dirty: bool,
    right_path: QString,
    status: QString,
    tab_count: i32,
    validation_compatible: bool,
    validation_message: QString,
    validation_path_kind: QString,
    paths: AppPaths,
    state: GuiBridgeState,
}

impl Default for LinSyncSessionBridgeRust {
    fn default() -> Self {
        Self {
            active_tab_id: 0,
            can_redo: false,
            can_undo: false,
            context_json: QString::default(),
            difference_count: 0,
            last_error: QString::default(),
            left_dirty: false,
            left_path: QString::default(),
            compare_mode: QString::from("Text"),
            right_dirty: false,
            right_path: QString::default(),
            status: QString::from("Ready"),
            tab_count: 0,
            validation_compatible: false,
            validation_message: QString::default(),
            validation_path_kind: QString::default(),
            paths: AppPaths::from_env(),
            state: GuiBridgeState::new(None),
        }
    }
}

impl ffi::LinSyncSessionBridge {
    pub fn session_json(&self) -> QString {
        match context_to_json(&self.rust().state.context()) {
            Ok(json) => QString::from(json),
            Err(err) => QString::from(error_context_json(&err)),
        }
    }

    pub fn tab_id_at(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }

        self.rust()
            .state
            .context()
            .session
            .tabs
            .get(index as usize)
            .map(|tab| to_i32(tab.id))
            .unwrap_or_default()
    }

    pub fn tab_title_at(&self, index: i32) -> QString {
        if index < 0 {
            return QString::default();
        }

        self.rust()
            .state
            .context()
            .session
            .tabs
            .get(index as usize)
            .map(|tab| QString::from(&tab.title))
            .unwrap_or_default()
    }

    pub fn tab_dirty_at(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }

        self.rust()
            .state
            .context()
            .session
            .tabs
            .get(index as usize)
            .map(|tab| tab.left_dirty || tab.right_dirty)
            .unwrap_or_default()
    }

    pub fn tab_can_undo_at(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }

        self.rust()
            .state
            .context()
            .session
            .tabs
            .get(index as usize)
            .map(|tab| tab.can_undo)
            .unwrap_or_default()
    }

    pub fn tab_can_redo_at(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }

        self.rust()
            .state
            .context()
            .session
            .tabs
            .get(index as usize)
            .map(|tab| tab.can_redo)
            .unwrap_or_default()
    }

    pub fn recent_path_count(&self) -> i32 {
        to_i32(self.rust().state.context().session.recent_paths.len())
    }

    pub fn recent_path_at(&self, index: i32) -> QString {
        if index < 0 {
            return QString::default();
        }

        self.rust()
            .state
            .context()
            .session
            .recent_paths
            .get(index as usize)
            .map(QString::from)
            .unwrap_or_default()
    }

    pub fn summary_count(&self) -> i32 {
        self.rust()
            .state
            .context()
            .active_tab()
            .map(|tab| to_i32(tab.summary.len()))
            .unwrap_or_default()
    }

    pub fn summary_label_at(&self, index: i32) -> QString {
        if index < 0 {
            return QString::default();
        }

        self.rust()
            .state
            .context()
            .active_tab()
            .and_then(|tab| tab.summary.get(index as usize))
            .map(|item| QString::from(&item.label))
            .unwrap_or_default()
    }

    pub fn summary_value_at(&self, index: i32) -> QString {
        if index < 0 {
            return QString::default();
        }

        self.rust()
            .state
            .context()
            .active_tab()
            .and_then(|tab| tab.summary.get(index as usize))
            .map(|item| QString::from(&item.value))
            .unwrap_or_default()
    }

    pub fn load_context_file(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let path = PathBuf::from(String::from(path));
        let context = match read_context_file(&path) {
            Ok(context) => context,
            Err(err) => return self.set_error(err),
        };

        {
            let mut rust = self.as_mut().rust_mut();
            rust.state = GuiBridgeState::new(Some(context));
        }
        self.as_mut().set_last_error(QString::default());
        self.refresh_context_json()
    }

    pub fn compare_paths(
        mut self: Pin<&mut Self>,
        left: &QString,
        right: &QString,
        mode: &QString,
        new_tab: bool,
    ) -> QString {
        let left = String::from(left);
        let right = String::from(right);
        let mode = String::from(mode);
        // Resolve all mode options from the active profile (mirroring the
        // HTTP bridge's `resolve_compare_options_for_request`) instead of
        // always using per-mode defaults. No QML-side per-request override
        // is threaded here yet, so the params list is empty.
        let paths = self.as_ref().get_ref().rust().paths.clone();
        let options = resolve_compare_options_for_request(&paths, &[])
            .unwrap_or_else(|_| GuiCompareOptions::default());
        let tab = build_tab_for_paths_with_mode(
            Path::new(&left),
            Path::new(&right),
            Some(&mode),
            &options,
        );

        let context = {
            let mut rust = self.as_mut().rust_mut();
            rust.state.apply_compare(tab, new_tab)
        };
        record_recent_context(&paths, &context);
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    /// In-process parity for the HTTP `GET /profiles/list` endpoint:
    /// `{active, profiles[{id,name,description,builtin}]}`.
    pub fn profile_list(&self) -> QString {
        let bytes = crate::profiles_list_bridge_response(&self.rust().paths);
        QString::from(String::from_utf8_lossy(&bytes).as_ref())
    }

    /// In-process parity for `GET /profiles/active/get`.
    pub fn profile_active_get(&self) -> QString {
        let bytes = crate::profiles_active_get_bridge_response(&self.rust().paths);
        QString::from(String::from_utf8_lossy(&bytes).as_ref())
    }

    /// In-process parity for `GET /profiles/active/set?id=X`. Rejects unknown
    /// ids exactly like the HTTP route (the response carries the 404 body).
    pub fn profile_active_set(&self, id: &QString) -> QString {
        let id = String::from(id);
        let query = format!("id={id}");
        let bytes = crate::profiles_active_set_bridge_response(&query, &self.rust().paths);
        QString::from(String::from_utf8_lossy(&bytes).as_ref())
    }

    pub fn activate_tab(mut self: Pin<&mut Self>, tab_id: i32) -> QString {
        if tab_id <= 0 {
            return self.set_error("missing tab id".to_owned());
        }

        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.activate_tab(tab_id as u64) {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn close_tab(mut self: Pin<&mut Self>, tab_id: i32) -> QString {
        if tab_id <= 0 {
            return self.set_error("missing tab id".to_owned());
        }

        let context = {
            let mut rust = self.as_mut().rust_mut();
            rust.state.close_tab(tab_id as u64)
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn copy_current_row(mut self: Pin<&mut Self>, row: i32, direction: &QString) -> QString {
        if row < 0 {
            return self.set_error("missing row".to_owned());
        }

        let direction = String::from(direction);
        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.copy_row(row as usize, &direction) {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn copy_all(mut self: Pin<&mut Self>, direction: &QString) -> QString {
        let direction = String::from(direction);
        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.copy_all(&direction) {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn undo(mut self: Pin<&mut Self>) -> QString {
        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.undo() {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn redo(mut self: Pin<&mut Self>) -> QString {
        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.redo() {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn save_side(mut self: Pin<&mut Self>, side: &QString) -> QString {
        let side = String::from(side);
        let context = {
            let mut rust = self.as_mut().rust_mut();
            match rust.state.save_side(&side) {
                Ok(context) => context,
                Err(err) => return self.set_error(err),
            }
        };
        self.as_mut().set_last_error(QString::default());
        self.set_context_json_from_context(&context)
    }

    pub fn load_settings(mut self: Pin<&mut Self>) -> QString {
        let paths = self.as_ref().get_ref().rust().paths.clone();
        match load_gui_settings_json(&paths) {
            Ok(json) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(json)
            }
            Err(err) => self.set_error(err),
        }
    }

    pub fn save_setting(mut self: Pin<&mut Self>, key: &QString, value: &QString) -> QString {
        let paths = self.as_ref().get_ref().rust().paths.clone();
        let key = String::from(key);
        let value = String::from(value);
        match save_gui_setting_json(&paths, &key, &value) {
            Ok(json) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(json)
            }
            Err(err) => self.set_error(err),
        }
    }

    pub fn reset_settings(mut self: Pin<&mut Self>) -> QString {
        let paths = self.as_ref().get_ref().rust().paths.clone();
        match reset_gui_settings_json(&paths) {
            Ok(json) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(json)
            }
            Err(err) => self.set_error(err),
        }
    }

    pub fn start_three_way_merge(
        mut self: Pin<&mut Self>,
        base: &QString,
        left: &QString,
        right: &QString,
    ) -> QString {
        let base = String::from(base);
        let left = String::from(left);
        let right = String::from(right);
        // Wrap the in-process state in a temporary Arc<Mutex<>> so we can reuse
        // the shared handler that the HTTP bridge also calls.
        let wrapped = {
            let mut rust = self.as_mut().rust_mut();
            // Temporarily move the state out, run the handler, then put it back.
            let placeholder = GuiBridgeState::new(None);
            let current = std::mem::replace(&mut rust.state, placeholder);
            Arc::new(Mutex::new(current))
        };
        let result = start_three_way_merge_session(&base, &left, &right, &wrapped);
        {
            let mut rust = self.as_mut().rust_mut();
            let recovered = Arc::try_unwrap(wrapped)
                .ok()
                .and_then(|m| m.into_inner().ok())
                .unwrap_or_else(|| GuiBridgeState::new(None));
            rust.state = recovered;
        }
        match result {
            Ok(json) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(json)
            }
            Err(err) => self.set_error(err),
        }
    }

    pub fn resolve_three_way_conflict(
        mut self: Pin<&mut Self>,
        id: i32,
        choice: &QString,
        text: &QString,
    ) -> QString {
        let choice = String::from(choice);
        let text = String::from(text);
        let wrapped = {
            let mut rust = self.as_mut().rust_mut();
            let placeholder = GuiBridgeState::new(None);
            let current = std::mem::replace(&mut rust.state, placeholder);
            Arc::new(Mutex::new(current))
        };
        let result = resolve_three_way_conflict_impl(id.max(0) as u32, &choice, &text, &wrapped);
        {
            let mut rust = self.as_mut().rust_mut();
            let recovered = Arc::try_unwrap(wrapped)
                .ok()
                .and_then(|m| m.into_inner().ok())
                .unwrap_or_else(|| GuiBridgeState::new(None));
            rust.state = recovered;
        }
        match result {
            Ok(json) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(json)
            }
            Err(err) => self.set_error(err),
        }
    }

    pub fn save_three_way_merge(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let path = String::from(path);
        let wrapped = {
            let mut rust = self.as_mut().rust_mut();
            let placeholder = GuiBridgeState::new(None);
            let current = std::mem::replace(&mut rust.state, placeholder);
            Arc::new(Mutex::new(current))
        };
        let result = save_three_way_merge_output(&path, &wrapped);
        {
            let mut rust = self.as_mut().rust_mut();
            let recovered = Arc::try_unwrap(wrapped)
                .ok()
                .and_then(|m| m.into_inner().ok())
                .unwrap_or_else(|| GuiBridgeState::new(None));
            rust.state = recovered;
        }
        match result {
            Ok(()) => {
                self.as_mut().set_last_error(QString::default());
                QString::from(serde_json::json!({ "ok": true }).to_string())
            }
            Err(err) => self.set_error(err),
        }
    }

    fn refresh_context_json(self: Pin<&mut Self>) -> QString {
        let context = self.as_ref().get_ref().rust().state.context();
        self.set_context_json_from_context(&context)
    }

    fn set_context_json_from_context(
        mut self: Pin<&mut Self>,
        context: &GuiLaunchContext,
    ) -> QString {
        match context_to_json(context) {
            Ok(json) => {
                let qjson = QString::from(json);
                // The active tab's row set (which `row_count`/`model_data`
                // read) has just been replaced wholesale by the calling
                // invokable, so notify bound QML views to re-query rather than
                // render stale rows. A model reset matches the wholesale
                // replacement (compare / undo / redo / copy / save / tab switch)
                // better than incremental insert/remove notifications.
                // SAFETY: `end_reset_model` is paired with this call below.
                unsafe {
                    self.as_mut().begin_reset_model();
                }
                self.as_mut()
                    .sync_active_tab_properties(ActiveTabSnapshot::from_context(context));
                self.as_mut().set_context_json(qjson.clone());
                // SAFETY: paired with the `begin_reset_model` call above.
                unsafe {
                    self.as_mut().end_reset_model();
                }
                qjson
            }
            Err(err) => self.set_error(err),
        }
    }

    fn sync_active_tab_properties(mut self: Pin<&mut Self>, snapshot: ActiveTabSnapshot) {
        self.as_mut().set_active_tab_id(snapshot.active_tab_id);
        self.as_mut().set_can_redo(snapshot.can_redo);
        self.as_mut().set_can_undo(snapshot.can_undo);
        self.as_mut().set_tab_count(snapshot.tab_count);
        self.as_mut()
            .set_left_path(QString::from(snapshot.left_path));
        self.as_mut()
            .set_right_path(QString::from(snapshot.right_path));
        self.as_mut()
            .set_compare_mode(QString::from(snapshot.compare_mode));
        self.as_mut().set_status(QString::from(snapshot.status));
        self.as_mut()
            .set_difference_count(snapshot.difference_count);
        self.as_mut()
            .set_validation_compatible(snapshot.validation_compatible);
        self.as_mut()
            .set_validation_message(QString::from(snapshot.validation_message));
        self.as_mut()
            .set_validation_path_kind(QString::from(snapshot.validation_path_kind));
        self.as_mut().set_left_dirty(snapshot.left_dirty);
        self.as_mut().set_right_dirty(snapshot.right_dirty);
    }

    fn set_error(mut self: Pin<&mut Self>, message: String) -> QString {
        self.as_mut().set_last_error(QString::from(&message));
        QString::default()
    }

    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            return 0;
        }

        self.rust()
            .state
            .context()
            .active_tab()
            .map(|tab| tab.left_rows.len().max(tab.right_rows.len()))
            .map(to_i32)
            .unwrap_or_default()
    }

    pub fn model_data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if !index.is_valid() {
            return QVariant::default();
        }

        let row_index = index.row();
        if row_index < 0 {
            return QVariant::default();
        }

        let context = self.rust().state.context();
        let Some(tab) = context.active_tab() else {
            return QVariant::default();
        };
        let row_index = row_index as usize;
        let left = tab.left_rows.get(row_index);
        let right = tab.right_rows.get(row_index);

        match role {
            DISPLAY_ROLE => row_text_variant(left),
            LEFT_ROW_ID_ROLE => row_id_variant(left),
            LEFT_NUMBER_ROLE => row_number_variant(left),
            LEFT_TEXT_ROLE => row_text_variant(left),
            LEFT_STATE_ROLE => row_state_variant(left),
            RIGHT_ROW_ID_ROLE => row_id_variant(right),
            RIGHT_NUMBER_ROLE => row_number_variant(right),
            RIGHT_TEXT_ROLE => row_text_variant(right),
            RIGHT_STATE_ROLE => row_state_variant(right),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> RoleNames {
        let mut roles = RoleNames::default();
        insert_role(&mut roles, DISPLAY_ROLE, "display");
        insert_role(&mut roles, LEFT_ROW_ID_ROLE, "leftRowId");
        insert_role(&mut roles, LEFT_NUMBER_ROLE, "leftNumber");
        insert_role(&mut roles, LEFT_TEXT_ROLE, "leftText");
        insert_role(&mut roles, LEFT_STATE_ROLE, "leftState");
        insert_role(&mut roles, RIGHT_ROW_ID_ROLE, "rightRowId");
        insert_role(&mut roles, RIGHT_NUMBER_ROLE, "rightNumber");
        insert_role(&mut roles, RIGHT_TEXT_ROLE, "rightText");
        insert_role(&mut roles, RIGHT_STATE_ROLE, "rightState");
        roles
    }
}

struct ActiveTabSnapshot {
    active_tab_id: i32,
    can_redo: bool,
    can_undo: bool,
    tab_count: i32,
    left_path: String,
    right_path: String,
    compare_mode: String,
    status: String,
    difference_count: i32,
    validation_compatible: bool,
    validation_message: String,
    validation_path_kind: String,
    left_dirty: bool,
    right_dirty: bool,
}

impl ActiveTabSnapshot {
    fn from_context(context: &GuiLaunchContext) -> Self {
        let Some(tab) = context.active_tab() else {
            return Self::default();
        };

        Self {
            active_tab_id: to_i32(tab.id),
            can_redo: tab.can_redo,
            can_undo: tab.can_undo,
            tab_count: to_i32(context.session.tabs.len()),
            left_path: tab.left_path.clone(),
            right_path: tab.right_path.clone(),
            compare_mode: tab.mode.clone(),
            status: tab.status.clone(),
            difference_count: to_i32(tab.difference_count),
            validation_compatible: tab.validation.compatible,
            validation_message: tab.validation.message.clone(),
            validation_path_kind: tab.validation.path_kind.clone(),
            left_dirty: tab.left_dirty,
            right_dirty: tab.right_dirty,
        }
    }
}

impl Default for ActiveTabSnapshot {
    fn default() -> Self {
        Self {
            active_tab_id: 0,
            can_redo: false,
            can_undo: false,
            tab_count: 0,
            left_path: String::new(),
            right_path: String::new(),
            compare_mode: "Text".to_owned(),
            status: "Ready".to_owned(),
            difference_count: 0,
            validation_compatible: false,
            validation_message: String::new(),
            validation_path_kind: String::new(),
            left_dirty: false,
            right_dirty: false,
        }
    }
}

fn to_i32(value: impl TryInto<i32>) -> i32 {
    value.try_into().unwrap_or(i32::MAX)
}

fn insert_role(roles: &mut RoleNames, role: i32, name: &str) {
    roles.insert(role, QByteArray::from(name));
}

fn row_id_variant(row: Option<&GuiLineRow>) -> QVariant {
    row.map(|row| qvariant_string(&row.row_id))
        .unwrap_or_default()
}

fn row_number_variant(row: Option<&GuiLineRow>) -> QVariant {
    row.and_then(|row| row.number)
        .map(to_i32)
        .map(|number| QVariant::from(&number))
        .unwrap_or_default()
}

fn row_state_variant(row: Option<&GuiLineRow>) -> QVariant {
    row.map(|row| qvariant_string(&row.state))
        .unwrap_or_default()
}

fn row_text_variant(row: Option<&GuiLineRow>) -> QVariant {
    row.map(|row| qvariant_string(&row.text))
        .unwrap_or_default()
}

fn qvariant_string(value: &str) -> QVariant {
    QVariant::from(&QString::from(value))
}

fn read_context_file(path: &Path) -> Result<GuiLaunchContext, String> {
    let data = fs::read_to_string(path)
        .map_err(|err| format!("failed to read GUI context '{}': {err}", path.display()))?;
    serde_json::from_str(&data)
        .map_err(|err| format!("failed to parse GUI context '{}': {err}", path.display()))
}

fn error_context_json(message: &str) -> String {
    serde_json::json!({
        "schema_version": crate::RESPONSE_SCHEMA_VERSION,
        "session": { "active_tab_id": 0, "tabs": [], "recent_paths": [] },
        "error": message,
    })
    .to_string()
}
