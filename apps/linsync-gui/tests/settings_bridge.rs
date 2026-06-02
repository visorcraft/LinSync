// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync::test_support::{apply_gui_setting_test, save_and_load_setting, temp_app_paths};
use linsync_core::ThemePreference;

#[test]
fn every_documented_key_round_trips_through_bridge() {
    for (key, value) in [
        ("themePreference", "2"), // numeric theme value; 2 = Dark
        ("fontSize", "14"),
        ("fontFamily", "Iosevka"),
        ("tabWidth", "8"),
        ("showLineNumbers", "true"),
        ("showWhitespace", "true"),
        ("wordWrap", "true"),
        ("ignoreCase", "true"),
        ("ignoreWhitespace", "true"),
        ("ignoreBlankLines", "true"),
        ("ignoreEol", "true"),
        ("detectMoves", "true"),
        ("eolNormalization", "lf"),
        ("defaultCompareMode", "binary"),
        ("openLastSession", "true"),
        ("confirmOnClose", "true"),
        ("persistRecentPaths", "true"),
        ("reduceMotion", "true"),
        ("maxRecentPaths", "25"),
    ] {
        let result = apply_gui_setting_test(key, value);
        assert!(
            result.is_ok(),
            "key {key:?} value {value:?} failed: {result:?}"
        );
    }
}

#[test]
fn every_documented_key_persists_to_disk() {
    let paths = temp_app_paths("settings-bridge-persist");

    // Each tuple: (gui key, value string, verifier closure).
    // The verifier receives the reloaded CoreSettings and asserts the expected field value.

    let loaded = save_and_load_setting(&paths, "themePreference", "2")
        .expect("themePreference should persist");
    assert_eq!(
        loaded.theme_preference,
        ThemePreference::Dark,
        "themePreference=2 should reload as Dark"
    );

    let loaded = save_and_load_setting(&paths, "fontSize", "14").expect("fontSize should persist");
    assert_eq!(loaded.pane_font_size, 14, "fontSize=14 should reload as 14");

    let loaded =
        save_and_load_setting(&paths, "fontFamily", "Iosevka").expect("fontFamily should persist");
    assert_eq!(
        loaded.pane_font_family, "Iosevka",
        "fontFamily should reload as Iosevka"
    );

    let loaded = save_and_load_setting(&paths, "tabWidth", "8").expect("tabWidth should persist");
    assert_eq!(loaded.pane_tab_width, 8, "tabWidth=8 should reload as 8");

    let loaded = save_and_load_setting(&paths, "showLineNumbers", "false")
        .expect("showLineNumbers should persist");
    assert!(
        !loaded.show_line_numbers,
        "showLineNumbers=false should reload as false"
    );

    let loaded = save_and_load_setting(&paths, "showWhitespace", "true")
        .expect("showWhitespace should persist");
    assert!(
        loaded.show_whitespace,
        "showWhitespace=true should reload as true"
    );

    let loaded =
        save_and_load_setting(&paths, "wordWrap", "true").expect("wordWrap should persist");
    assert!(loaded.word_wrap, "wordWrap=true should reload as true");

    let loaded =
        save_and_load_setting(&paths, "ignoreCase", "true").expect("ignoreCase should persist");
    assert!(loaded.ignore_case, "ignoreCase=true should reload as true");

    let loaded = save_and_load_setting(&paths, "ignoreWhitespace", "true")
        .expect("ignoreWhitespace should persist");
    assert!(
        loaded.ignore_whitespace,
        "ignoreWhitespace=true should reload as true"
    );

    let loaded = save_and_load_setting(&paths, "ignoreBlankLines", "true")
        .expect("ignoreBlankLines should persist");
    assert!(
        loaded.ignore_blank_lines,
        "ignoreBlankLines=true should reload as true"
    );

    let loaded =
        save_and_load_setting(&paths, "ignoreEol", "false").expect("ignoreEol should persist");
    assert!(!loaded.ignore_eol, "ignoreEol=false should reload as false");

    let loaded =
        save_and_load_setting(&paths, "detectMoves", "true").expect("detectMoves should persist");
    assert!(
        loaded.detect_moves,
        "detectMoves=true should reload as true"
    );

    let loaded = save_and_load_setting(&paths, "eolNormalization", "lf")
        .expect("eolNormalization should persist");
    assert_eq!(
        loaded.eol_normalization, "lf",
        "eolNormalization=lf should reload as lf"
    );

    let loaded = save_and_load_setting(&paths, "defaultCompareMode", "binary")
        .expect("defaultCompareMode should persist");
    assert_eq!(
        loaded.default_compare_mode, "binary",
        "defaultCompareMode=binary should reload as binary"
    );

    let loaded = save_and_load_setting(&paths, "openLastSession", "false")
        .expect("openLastSession should persist");
    assert!(
        !loaded.open_last_session,
        "openLastSession=false should reload as false"
    );

    let loaded = save_and_load_setting(&paths, "confirmOnClose", "true")
        .expect("confirmOnClose should persist");
    assert!(
        loaded.confirm_on_close,
        "confirmOnClose=true should reload as true"
    );

    let loaded = save_and_load_setting(&paths, "persistRecentPaths", "false")
        .expect("persistRecentPaths should persist");
    assert!(
        !loaded.persist_recent_paths,
        "persistRecentPaths=false should reload as false"
    );

    let loaded =
        save_and_load_setting(&paths, "reduceMotion", "true").expect("reduceMotion should persist");
    assert!(
        loaded.reduce_motion,
        "reduceMotion=true should reload as true"
    );

    let loaded = save_and_load_setting(&paths, "maxRecentPaths", "25")
        .expect("maxRecentPaths should persist");
    assert_eq!(
        loaded.recent_limit, 25,
        "maxRecentPaths=25 should reload as 25"
    );
}
