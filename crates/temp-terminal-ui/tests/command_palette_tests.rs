use temp_terminal_ui::workspace::{
    filter_palette_commands, get_all_palette_commands, CommandPaletteState, PaletteCommand,
};

#[test]
fn test_command_palette_state_navigation() {
    let mut state = CommandPaletteState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);
    assert_eq!(state.selected_index, 0);
    assert!(state.query.is_empty());

    // Navigation down with wrap around
    state.move_down(3);
    assert_eq!(state.selected_index, 1);
    state.move_down(3);
    assert_eq!(state.selected_index, 2);
    state.move_down(3);
    assert_eq!(state.selected_index, 0); // wrapped around

    // Navigation up with wrap around
    state.move_up(3);
    assert_eq!(state.selected_index, 2); // wrapped to bottom
    state.move_up(3);
    assert_eq!(state.selected_index, 1);
    state.move_up(3);
    assert_eq!(state.selected_index, 0);

    // Clamping boundaries
    state.selected_index = 10;
    state.clamp_selection(5);
    assert_eq!(state.selected_index, 0);

    state.close();
    assert!(!state.is_open);

    // Toggle
    state.toggle();
    assert!(state.is_open);
    state.toggle();
    assert!(!state.is_open);
}

#[test]
fn test_command_palette_state_edge_cases() {
    let mut state = CommandPaletteState::default();

    // Zero items navigation should not panic or divide by zero
    state.move_up(0);
    assert_eq!(state.selected_index, 0);
    state.move_down(0);
    assert_eq!(state.selected_index, 0);
    state.clamp_selection(0);
    assert_eq!(state.selected_index, 0);

    // Clamping valid index should keep it intact
    state.selected_index = 2;
    state.clamp_selection(4);
    assert_eq!(state.selected_index, 2);

    // Open resets query and selected_index
    state.query = "some search".to_string();
    state.selected_index = 3;
    state.open();
    assert!(state.is_open);
    assert!(state.query.is_empty());
    assert_eq!(state.selected_index, 0);
}

#[test]
fn test_filter_palette_commands() {
    let cmds = [
        PaletteCommand { id: "session.new", title: "New Terminal Session", category: "Session", shortcut: Some("Ctrl+T") },
        PaletteCommand { id: "file.save", title: "Save File", category: "File", shortcut: Some("Ctrl+S") },
        PaletteCommand { id: "view.toggleSidebar", title: "Toggle Left Sidebar", category: "View", shortcut: Some("Ctrl+B") },
    ];

    // Empty or whitespace query returns all
    let res = filter_palette_commands(&cmds, "");
    assert_eq!(res.len(), 3);
    let res = filter_palette_commands(&cmds, "   ");
    assert_eq!(res.len(), 3);

    // Match title
    let res = filter_palette_commands(&cmds, "terminal");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "session.new");

    // Match category
    let res = filter_palette_commands(&cmds, "File");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "file.save");

    // Match shortcut
    let res = filter_palette_commands(&cmds, "Ctrl+B");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "view.toggleSidebar");

    // No match
    let res = filter_palette_commands(&cmds, "UnknownAction");
    assert!(res.is_empty());
}

#[test]
fn test_get_all_palette_commands_validity() {
    let all = get_all_palette_commands();
    assert_eq!(all.len(), 18);

    for cmd in &all {
        assert!(!cmd.id.is_empty(), "Palette command id must not be empty");
        assert!(!cmd.title.is_empty(), "Palette command title must not be empty");
        assert!(!cmd.category.is_empty(), "Palette command category must not be empty");
        assert!(
            cmd.id.contains('.'),
            "Palette command id should be namespaced (e.g., category.action): {}",
            cmd.id
        );
    }
}

#[test]
fn test_palette_commands_key_combinations_parse_valid_keycaps() {
    let all = get_all_palette_commands();
    let mut shortcut_count = 0;

    for cmd in &all {
        if let Some(sc) = cmd.shortcut {
            shortcut_count += 1;
            assert!(!sc.is_empty(), "Shortcut must not be empty if Some");

            // Split on alternative combinations (e.g. "F1 / Ctrl+Shift+/")
            let parts: Vec<&str> = sc.split(" / ").collect();
            assert!(!parts.is_empty(), "Should yield at least one combination");

            for part in parts {
                let keys: Vec<&str> = part.split('+').collect();
                assert!(!keys.is_empty(), "Should yield at least one keycap");
                for key in keys {
                    assert!(
                        !key.trim().is_empty(),
                        "Keycap must not be whitespace: {} in {}",
                        key,
                        sc
                    );
                }
            }
        }
    }

    assert!(shortcut_count > 10, "Expected majority of palette commands to have shortcuts");
}

#[test]
fn test_palette_commands_neutral_categories() {
    let all = get_all_palette_commands();
    let allowed_categories = [
        "App",
        "File",
        "Help",
        "Session",
        "Process",
        "Terminal",
        "View",
    ];

    for cmd in &all {
        assert!(
            allowed_categories.contains(&cmd.category),
            "Category '{}' in command '{}' is not in the neutral categories set (Anti-Slop R-29)",
            cmd.category,
            cmd.title
        );
    }
}

#[test]
fn test_filter_palette_commands_case_and_whitespace_invariance() {
    let all = get_all_palette_commands();

    // Mixed case title query
    let res_upper = filter_palette_commands(&all, "TERMINAL");
    let res_lower = filter_palette_commands(&all, "terminal");
    assert_eq!(res_upper.len(), res_lower.len());
    assert!(!res_upper.is_empty());

    // Leading/trailing whitespace
    let res_padded = filter_palette_commands(&all, "   split   ");
    let res_clean = filter_palette_commands(&all, "split");
    assert_eq!(res_padded.len(), res_clean.len());
    assert_eq!(res_padded.len(), 2); // Split Terminal Down, Split Terminal Right

    // Shortcut search
    let res_sc = filter_palette_commands(&all, "ctrl+shift+p");
    assert_eq!(res_sc.len(), 1);
    assert_eq!(res_sc[0].id, "app.commandPalette");
}

#[test]
fn test_command_palette_wrap_around_navigation() {
    let mut state = CommandPaletteState::default();

    // 1. Empty list (total_items == 0)
    state.selected_index = 0;
    state.move_down(0);
    assert_eq!(state.selected_index, 0);
    state.move_up(0);
    assert_eq!(state.selected_index, 0);
    state.clamp_selection(0);
    assert_eq!(state.selected_index, 0);

    // 2. Single item (total_items == 1)
    state.move_down(1);
    assert_eq!(state.selected_index, 0);
    state.move_up(1);
    assert_eq!(state.selected_index, 0);
    state.clamp_selection(1);
    assert_eq!(state.selected_index, 0);

    // 3. Multiple items (total_items == 4)
    // Wrap around up from top (0 -> 3)
    state.selected_index = 0;
    state.move_up(4);
    assert_eq!(state.selected_index, 3);

    // Wrap around down from bottom (3 -> 0)
    state.move_down(4);
    assert_eq!(state.selected_index, 0);

    // Step-by-step downward navigation
    state.move_down(4);
    assert_eq!(state.selected_index, 1);
    state.move_down(4);
    assert_eq!(state.selected_index, 2);
    state.move_down(4);
    assert_eq!(state.selected_index, 3);
    state.move_down(4);
    assert_eq!(state.selected_index, 0);

    // Step-by-step upward navigation
    state.move_up(4);
    assert_eq!(state.selected_index, 3);
    state.move_up(4);
    assert_eq!(state.selected_index, 2);
    state.move_up(4);
    assert_eq!(state.selected_index, 1);
    state.move_up(4);
    assert_eq!(state.selected_index, 0);
}

#[test]
fn test_command_palette_dynamic_filtering_and_selection_lifecycle() {
    let all = get_all_palette_commands();
    let mut state = CommandPaletteState::default();

    state.open();
    assert!(state.is_open);
    assert_eq!(state.selected_index, 0);

    let filtered = filter_palette_commands(&all, &state.query);
    assert_eq!(filtered.len(), 18);

    // Navigate down to an arbitrary index
    for _ in 0..5 {
        state.move_down(filtered.len());
    }
    assert_eq!(state.selected_index, 5);

    // Simulating user typing "terminal": query changed resets selected_index to 0
    state.query = "terminal".to_string();
    state.selected_index = 0;

    let filtered_terminal = filter_palette_commands(&all, &state.query);
    assert!(!filtered_terminal.is_empty());
    state.clamp_selection(filtered_terminal.len());
    assert_eq!(state.selected_index, 0);

    // Navigate to bottom item in filtered results
    state.move_up(filtered_terminal.len());
    assert_eq!(state.selected_index, filtered_terminal.len() - 1);
    let chosen_cmd = filtered_terminal[state.selected_index];
    assert!(
        chosen_cmd.title.to_lowercase().contains("terminal")
            || chosen_cmd.category.to_lowercase().contains("terminal")
    );

    // Narrow query to non-matching term
    state.query = "nonexistent_query_xyz".to_string();
    state.selected_index = 0;
    let filtered_empty = filter_palette_commands(&all, &state.query);
    assert!(filtered_empty.is_empty());
    state.clamp_selection(filtered_empty.len());
    assert_eq!(state.selected_index, 0);

    // Safe navigation when empty
    state.move_down(filtered_empty.len());
    assert_eq!(state.selected_index, 0);
    state.move_up(filtered_empty.len());
    assert_eq!(state.selected_index, 0);

    // Clear query back to empty
    state.query.clear();
    state.selected_index = 0;
    let filtered_reset = filter_palette_commands(&all, &state.query);
    assert_eq!(filtered_reset.len(), 18);
    state.clamp_selection(filtered_reset.len());
    assert_eq!(state.selected_index, 0);
}

#[test]
fn test_command_palette_escape_dismissal_and_enter_execution() {
    let all = get_all_palette_commands();
    let mut state = CommandPaletteState::default();

    // Escape dismissal
    state.open();
    assert!(state.is_open);
    state.close();
    assert!(!state.is_open);

    // Enter execution with valid match
    state.open();
    state.query = "settings".to_string();
    state.selected_index = 0;
    let filtered = filter_palette_commands(&all, &state.query);
    assert_eq!(filtered.len(), 1);

    let mut executed_id = None;
    if !filtered.is_empty() && state.selected_index < filtered.len() {
        executed_id = Some(filtered[state.selected_index].id);
        state.close();
    }
    assert_eq!(executed_id, Some("app.settings"));
    assert!(!state.is_open);

    // Enter press with empty search results should not execute or close
    state.open();
    state.query = "no_match_possible".to_string();
    state.selected_index = 0;
    let empty_filtered = filter_palette_commands(&all, &state.query);
    assert!(empty_filtered.is_empty());

    let mut executed_id_empty = None;
    if !empty_filtered.is_empty() && state.selected_index < empty_filtered.len() {
        executed_id_empty = Some(empty_filtered[state.selected_index].id);
        state.close();
    }
    assert_eq!(executed_id_empty, None);
    assert!(state.is_open);

    // Mouse click row execution simulation
    let all_filtered = filter_palette_commands(&all, "");
    let target_idx = 2;
    let target_cmd = all_filtered[target_idx];
    state.selected_index = target_idx;
    let clicked_id = Some(target_cmd.id);
    state.close();

    assert_eq!(clicked_id, Some("file.save"));
    assert!(!state.is_open);
}
