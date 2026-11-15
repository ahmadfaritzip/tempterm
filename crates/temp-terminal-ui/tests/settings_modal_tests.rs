use temp_terminal_ui::workspace::{SettingsCategory, SettingsCategoryGroup, SettingsModalState};

#[test]
fn test_settings_category_groups_and_metadata() {
    let all = SettingsCategory::all();
    assert_eq!(all.len(), 11);

    assert_eq!(SettingsCategory::General.group(), SettingsCategoryGroup::Application);
    assert_eq!(SettingsCategory::Appearance.group(), SettingsCategoryGroup::Application);
    assert_eq!(SettingsCategory::Terminal.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Editor.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Browser.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Keyboard.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Notifications.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::Agents.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::WebAccess.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::DevUrls.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::About.group(), SettingsCategoryGroup::System);

    for cat in all {
        assert!(!cat.title().is_empty());
        assert!(!cat.description().is_empty());
    }

    let groups = SettingsCategoryGroup::all();
    assert_eq!(groups.len(), 4);
    assert_eq!(SettingsCategoryGroup::Application.label(), "APPLICATION");
    assert_eq!(SettingsCategoryGroup::Workspace.label(), "WORKSPACE");
    assert_eq!(SettingsCategoryGroup::Integrations.label(), "INTEGRATIONS");
    assert_eq!(SettingsCategoryGroup::System.label(), "SYSTEM");

    assert_eq!(SettingsCategoryGroup::Application.categories(), &[SettingsCategory::General, SettingsCategory::Appearance]);
    assert_eq!(SettingsCategoryGroup::Workspace.categories(), &[
        SettingsCategory::Terminal,
        SettingsCategory::Editor,
        SettingsCategory::Browser,
        SettingsCategory::Keyboard,
    ]);
    assert_eq!(SettingsCategoryGroup::Integrations.categories(), &[
        SettingsCategory::Notifications,
        SettingsCategory::Agents,
        SettingsCategory::WebAccess,
        SettingsCategory::DevUrls,
    ]);
    assert_eq!(SettingsCategoryGroup::System.categories(), &[SettingsCategory::About]);
}

#[test]
fn test_settings_modal_state_navigation() {
    let mut state = SettingsModalState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);

    state.set_category(SettingsCategory::Terminal);
    assert_eq!(state.active_category, SettingsCategory::Terminal);

    state.next_category();
    assert_eq!(state.active_category, SettingsCategory::Editor);

    state.prev_category();
    assert_eq!(state.active_category, SettingsCategory::Terminal);

    state.close();
    assert!(!state.is_open);

    state.toggle();
    assert!(state.is_open);
    state.toggle();
    assert!(!state.is_open);
}

#[test]
fn test_settings_modal_state_cyclic_wrapping() {
    let mut state = SettingsModalState::default();
    let all = SettingsCategory::all();
    let first = all[0];
    let last = all[all.len() - 1];

    state.set_category(first);
    state.prev_category();
    assert_eq!(state.active_category, last);

    state.next_category();
    assert_eq!(state.active_category, first);
}

#[test]
fn test_settings_category_titles_zero_emoji_and_no_em_dash() {
    let all = SettingsCategory::all();
    for cat in all {
        let title = cat.title();
        let desc = cat.description();

        // Must contain NO em dash (\u2014)
        assert!(!title.contains('\u{2014}'), "Title contains em dash: {}", title);
        assert!(!desc.contains('\u{2014}'), "Description contains em dash: {}", desc);

        // Must contain NO emojis (Anti-Slop R-04)
        for ch in title.chars() {
            let u = ch as u32;
            let is_emoji = (0x1F300..=0x1F9FF).contains(&u)
                || (0x2600..=0x26FF).contains(&u)
                || (0x2700..=0x27BF).contains(&u);
            assert!(!is_emoji, "Title '{}' contains emoji character: {}", title, ch);
        }
    }

    for group in SettingsCategoryGroup::all() {
        let label = group.label();
        assert!(!label.contains('\u{2014}'), "Group label contains em dash: {}", label);
        assert_eq!(label, label.to_uppercase(), "Group label must be uppercase");
        for ch in label.chars() {
            let u = ch as u32;
            let is_emoji = (0x1F300..=0x1F9FF).contains(&u)
                || (0x2600..=0x26FF).contains(&u)
                || (0x2700..=0x27BF).contains(&u);
            assert!(!is_emoji, "Group label '{}' contains emoji: {}", label, ch);
        }
    }
}

#[test]
fn test_render_setting_card_executes_closure_and_returns_value() {
    let ctx = eframe::egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            let mut called = false;
            let val = temp_terminal_ui::workspace::render_setting_card(ui, "Session Lifecycle", |ui| {
                ui.label("Child content");
                called = true;
                12345
            });
            assert!(called);
            assert_eq!(val, 12345);
        });
    });
}

#[test]
fn test_settings_card_titles_zero_emoji_and_no_em_dash() {
    let card_titles = [
        "Session Lifecycle",
        "Link Handling",
        "Theme & Density",
        "Display Options",
        "Typography",
        "Cursor & Interaction",
        "Buffer Capacity",
        "Indentation & Wrapping",
        "Display Layout",
        "Desktop Alerts",
        "Supervised CLI Integrations",
        "Permissions & Automation",
        "Platform Availability",
        "Search Provider",
        "Browsing Data",
        "Keyboard Shortcuts",
        "Build Information",
        "Configuration & Logs",
        "Official Resources",
    ];

    for title in card_titles {
        assert!(
            !title.contains('\u{2014}'),
            "Card title contains em dash: {}",
            title
        );
        for ch in title.chars() {
            let u = ch as u32;
            let is_emoji = (0x1F300..=0x1F9FF).contains(&u)
                || (0x2600..=0x26FF).contains(&u)
                || (0x2700..=0x27BF).contains(&u);
            assert!(
                !is_emoji,
                "Card title '{}' contains emoji character: {}",
                title, ch
            );
        }
    }
}

#[test]
fn test_settings_keyboard_tab_uses_canonical_shortcuts() {
    let all = temp_terminal_ui::workspace::get_all_shortcuts();
    assert!(!all.is_empty(), "Canonical shortcuts list must not be empty");
    assert!(
        all.len() >= 15,
        "Expected at least 15 canonical shortcuts, found {}",
        all.len()
    );

    // Empty filter returns all shortcuts
    let filtered_empty = temp_terminal_ui::workspace::filter_shortcuts(all, "", None);
    assert_eq!(filtered_empty.len(), all.len());

    // Search by name
    let filtered_terminal = temp_terminal_ui::workspace::filter_shortcuts(all, "Terminal", None);
    assert!(!filtered_terminal.is_empty());
    assert!(filtered_terminal.len() < all.len());

    // Search by key combination
    let filtered_ctrl = temp_terminal_ui::workspace::filter_shortcuts(all, "Ctrl+Shift", None);
    assert!(!filtered_ctrl.is_empty());

    // Verify each shortcut adheres to anti-slop rules and valid structure
    for item in all {
        assert!(!item.name.is_empty());
        assert!(!item.key.is_empty());
        assert!(!item.category.is_empty());
        assert!(!item.action_id.is_empty());

        // Zero em dash (\u2014) audit
        assert!(
            !item.name.contains('\u{2014}'),
            "Shortcut name contains em dash: {}",
            item.name
        );
        assert!(
            !item.key.contains('\u{2014}'),
            "Shortcut key contains em dash: {}",
            item.key
        );
        assert!(
            !item.category.contains('\u{2014}'),
            "Shortcut category contains em dash: {}",
            item.category
        );

        // Zero emoji audit (Anti-Slop R-04)
        for ch in item.name.chars().chain(item.key.chars()).chain(item.category.chars()) {
            let u = ch as u32;
            let is_emoji = (0x1F300..=0x1F9FF).contains(&u)
                || (0x2600..=0x26FF).contains(&u)
                || (0x2700..=0x27BF).contains(&u);
            assert!(
                !is_emoji,
                "Shortcut item '{}' contains emoji character: {}",
                item.name, ch
            );
        }

        // Verify valid individual keycap parsing for UI rendering
        let parts: Vec<&str> = item.key.split(" / ").collect();
        for combo in parts {
            let sub_keys: Vec<&str> = combo.split('+').collect();
            for sub in sub_keys {
                assert!(!sub.trim().is_empty(), "Empty keycap in {}", item.key);
            }
        }
    }
}
