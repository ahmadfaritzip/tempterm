use temp_terminal_ui::workspace::{filter_shortcuts, get_all_shortcuts, ShortcutItem, ShortcutsOverlayState};

#[test]
fn test_shortcuts_overlay_state_navigation() {
    let mut state = ShortcutsOverlayState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);
    assert_eq!(state.selected_index, 0);
    assert!(state.filter.is_empty());

    // Navigation down and up
    state.move_down(5);
    assert_eq!(state.selected_index, 1);
    state.move_down(5);
    assert_eq!(state.selected_index, 2);
    state.move_down(5);
    state.move_down(5);
    state.move_down(5);
    assert_eq!(state.selected_index, 4); // clamped at max (5 - 1)

    state.move_up();
    assert_eq!(state.selected_index, 3);
    state.move_up();
    state.move_up();
    state.move_up();
    state.move_up();
    assert_eq!(state.selected_index, 0); // clamped at 0

    state.close();
    assert!(!state.is_open);

    // Toggle open and close
    state.toggle();
    assert!(state.is_open);
    state.toggle();
    assert!(!state.is_open);
}

#[test]
fn test_filter_shortcuts_query_and_category() {
    let items = [
        ShortcutItem {
            name: "New Terminal Session",
            key: "Ctrl+T",
            category: "Terminal",
            action_id: "session.new",
        },
        ShortcutItem {
            name: "Save Active Editor",
            key: "Ctrl+S",
            category: "Editor",
            action_id: "file.save",
        },
        ShortcutItem {
            name: "Toggle Sidebar",
            key: "Ctrl+B",
            category: "View",
            action_id: "view.toggleSidebar",
        },
    ];

    // Filter by query name
    let res = filter_shortcuts(&items, "Terminal", None);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "New Terminal Session");

    // Filter by key combo
    let res = filter_shortcuts(&items, "Ctrl+S", None);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].action_id, "file.save");

    // Filter by category
    let res = filter_shortcuts(&items, "", Some("Editor"));
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "Save Active Editor");

    // Filter by non-matching query
    let res = filter_shortcuts(&items, "NonExistentCommand", None);
    assert!(res.is_empty());
}

#[test]
fn test_get_all_shortcuts_and_category_filter() {
    let all = get_all_shortcuts();
    assert_eq!(all.len(), 27);

    let mut state = ShortcutsOverlayState::default();
    state.selected_index = 5;
    state.set_category_filter(Some("Terminal".to_string()));
    assert_eq!(state.category_filter.as_deref(), Some("Terminal"));
    assert_eq!(state.selected_index, 0);
}

#[test]
fn test_all_shortcuts_key_combinations_parse_valid_keycaps() {
    let all = get_all_shortcuts();
    for item in all {
        assert!(!item.name.is_empty());
        assert!(!item.category.is_empty());
        assert!(!item.key.is_empty());

        let combos: Vec<&str> = item.key.split(" / ").collect();
        assert!(!combos.is_empty());
        for combo in combos {
            let keys: Vec<&str> = combo.split('+').collect();
            assert!(!keys.is_empty());
            for key in keys {
                assert!(!key.trim().is_empty(), "Empty keycap in {}", item.key);
            }
        }
    }
}

#[test]
fn test_shortcuts_empty_list_arrow_navigation() {
    let mut state = ShortcutsOverlayState::default();
    state.open();
    assert_eq!(state.selected_index, 0);

    // Pressing ArrowDown on empty list should stay at 0 safely without panic
    state.move_down(0);
    assert_eq!(state.selected_index, 0);

    // Pressing ArrowUp on empty list should stay at 0
    state.move_up();
    assert_eq!(state.selected_index, 0);

    // If selected_index was somehow non-zero and clamped on empty list
    state.selected_index = 7;
    state.clamp_selection(0);
    assert_eq!(state.selected_index, 0);

    state.move_down(0);
    assert_eq!(state.selected_index, 0);
}

#[test]
fn test_shortcuts_search_and_index_adjustment_workflow() {
    let all = get_all_shortcuts();
    assert_eq!(all.len(), 27);

    let mut state = ShortcutsOverlayState::default();
    state.open();

    // 1. User navigates down 15 times in the full list
    for _ in 0..15 {
        state.move_down(all.len());
    }
    assert_eq!(state.selected_index, 15);

    // 2. User types a search query narrowing the list to a small count (< 15 items)
    state.filter = "terminal".to_string();
    let filtered = filter_shortcuts(&all, &state.filter, None);
    assert!(!filtered.is_empty());
    assert!(filtered.len() < 15, "Expected filtered items < 15, got {}", filtered.len());

    // Safe index clamping trigger when filtered count is smaller than selected_index
    if state.selected_index >= filtered.len() && !filtered.is_empty() {
        state.selected_index = 0;
    }
    assert_eq!(state.selected_index, 0);

    // 3. User navigates within filtered list
    state.move_down(filtered.len());
    assert_eq!(state.selected_index, 1);

    // 4. User types a query matching zero items
    state.filter = "xyz999_nonexistent".to_string();
    let filtered_empty = filter_shortcuts(&all, &state.filter, None);
    assert!(filtered_empty.is_empty());

    // Navigation on zero matches should remain at 0 without panic
    state.move_down(filtered_empty.len());
    assert_eq!(state.selected_index, 0);
    state.move_up();
    assert_eq!(state.selected_index, 0);

    // 5. User clears search filter (resets to full list)
    state.filter.clear();
    let reset_filtered = filter_shortcuts(&all, &state.filter, None);
    assert_eq!(reset_filtered.len(), all.len());
    assert_eq!(state.selected_index, 0);

    // User can navigate down to the last item
    for _ in 0..50 {
        state.move_down(reset_filtered.len());
    }
    assert_eq!(state.selected_index, reset_filtered.len() - 1);
}

#[test]
fn test_shortcuts_enter_execution_and_escape_closing() {
    let mut state = ShortcutsOverlayState::default();
    state.open();
    assert!(state.is_open);

    // Escape closes the modal
    state.close();
    assert!(!state.is_open);

    // Reopen and search for zoom shortcut
    state.open();
    let all = get_all_shortcuts();
    state.filter = "zoom".to_string();
    let filtered = filter_shortcuts(&all, &state.filter, None);
    assert!(!filtered.is_empty());

    // Enter selection picks the item action_id and closes
    let chosen = filtered.get(state.selected_index).map(|item| item.action_id);
    assert_eq!(chosen, Some("pane.zoom"));
    state.close();
    assert!(!state.is_open);
}

#[test]
fn test_shortcuts_overlay_clamp_selection_boundaries() {
    let mut state = ShortcutsOverlayState::default();

    // Clamping when selected_index >= total_items
    state.selected_index = 10;
    state.clamp_selection(5);
    assert_eq!(state.selected_index, 0);

    // Within bounds should remain untouched
    state.selected_index = 3;
    state.clamp_selection(5);
    assert_eq!(state.selected_index, 3);

    // Boundary edge: selected_index == total_items
    state.selected_index = 5;
    state.clamp_selection(5);
    assert_eq!(state.selected_index, 0);

    // Zero items always resets to 0
    state.selected_index = 0;
    state.clamp_selection(0);
    assert_eq!(state.selected_index, 0);
}


