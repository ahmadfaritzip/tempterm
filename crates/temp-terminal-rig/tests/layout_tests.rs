use std::str::FromStr;
use temp_terminal_rig::layout::{
    LayoutAllocator, LayoutConfig, RectBounds, SlotRole, TabSlotLocation,
};

#[test]
fn test_allocate_manager_slot_and_bounds() {
    let config = LayoutConfig::default();
    let location = LayoutAllocator::allocate_slot(&SlotRole::Manager, &config);

    assert_eq!(
        location,
        TabSlotLocation {
            tab_name: "main".to_string(),
            slot_index: 0,
            row: 0,
            col: 0,
            total_rows: 1,
            total_cols: 2,
        }
    );

    let bounds = LayoutAllocator::compute_rect_bounds(&location);
    assert_eq!(
        bounds,
        RectBounds {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        }
    );
}

#[test]
fn test_allocate_board_slot_and_bounds() {
    let config = LayoutConfig::default();
    let location = LayoutAllocator::allocate_slot(&SlotRole::Board, &config);

    assert_eq!(
        location,
        TabSlotLocation {
            tab_name: "main".to_string(),
            slot_index: 1,
            row: 0,
            col: 1,
            total_rows: 2,
            total_cols: 2,
        }
    );

    let bounds = LayoutAllocator::compute_rect_bounds(&location);
    assert_eq!(
        bounds,
        RectBounds {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        }
    );
}

#[test]
fn test_allocate_single_worker_when_pane_max_is_one() {
    let config = LayoutConfig {
        pane_max: 1,
        tab_max: 8,
    };
    let location = LayoutAllocator::allocate_slot(&SlotRole::Worker(0), &config);

    assert_eq!(
        location,
        TabSlotLocation {
            tab_name: "main".to_string(),
            slot_index: 2,
            row: 0,
            col: 1,
            total_rows: 1,
            total_cols: 2,
        }
    );

    let bounds = LayoutAllocator::compute_rect_bounds(&location);
    assert_eq!(
        bounds,
        RectBounds {
            x: 0.5,
            y: 0.5,
            w: 0.5,
            h: 0.5,
        }
    );
}

#[test]
fn test_allocate_workers_on_main_stack_vertically() {
    let config = LayoutConfig::default(); // pane_max = 2

    let loc_w0 = LayoutAllocator::allocate_slot(&SlotRole::Worker(0), &config);
    let loc_w1 = LayoutAllocator::allocate_slot(&SlotRole::Worker(1), &config);

    assert_eq!(
        loc_w0,
        TabSlotLocation {
            tab_name: "main".to_string(),
            slot_index: 2,
            row: 0,
            col: 1,
            total_rows: 2,
            total_cols: 2,
        }
    );
    assert_eq!(
        loc_w1,
        TabSlotLocation {
            tab_name: "main".to_string(),
            slot_index: 3,
            row: 1,
            col: 1,
            total_rows: 2,
            total_cols: 2,
        }
    );

    let b_w0 = LayoutAllocator::compute_rect_bounds(&loc_w0);
    let b_w1 = LayoutAllocator::compute_rect_bounds(&loc_w1);

    assert_eq!(
        b_w0,
        RectBounds {
            x: 0.5,
            y: 0.5,
            w: 0.5,
            h: 0.25,
        }
    );
    assert_eq!(
        b_w1,
        RectBounds {
            x: 0.5,
            y: 0.75,
            w: 0.5,
            h: 0.25,
        }
    );
}

#[test]
fn test_allocate_overflow_to_over_1() {
    let config = LayoutConfig::default(); // pane_max = 2, tab_max = 8

    let loc_w2 = LayoutAllocator::allocate_slot(&SlotRole::Worker(2), &config);
    assert_eq!(
        loc_w2,
        TabSlotLocation {
            tab_name: "over-1".to_string(),
            slot_index: 0,
            row: 0,
            col: 0,
            total_rows: 4,
            total_cols: 2,
        }
    );

    let b_w2 = LayoutAllocator::compute_rect_bounds(&loc_w2);
    assert_eq!(
        b_w2,
        RectBounds {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.25,
        }
    );
}

#[test]
fn test_allocate_balanced_two_column_overflow_grid() {
    let config = LayoutConfig::default(); // pane_max = 2, tab_max = 8

    let expected_grid = [
        (2, 0, 0, 0, 0.0, 0.0),
        (3, 1, 0, 1, 0.5, 0.0),
        (4, 2, 1, 0, 0.0, 0.25),
        (5, 3, 1, 1, 0.5, 0.25),
        (6, 4, 2, 0, 0.0, 0.50),
        (7, 5, 2, 1, 0.5, 0.50),
        (8, 6, 3, 0, 0.0, 0.75),
        (9, 7, 3, 1, 0.5, 0.75),
    ];

    for (worker_idx, slot_idx, row, col, x, y) in expected_grid {
        let loc = LayoutAllocator::allocate_slot(&SlotRole::Worker(worker_idx), &config);
        assert_eq!(
            loc,
            TabSlotLocation {
                tab_name: "over-1".to_string(),
                slot_index: slot_idx,
                row,
                col,
                total_rows: 4,
                total_cols: 2,
            },
            "mismatch for worker {}",
            worker_idx
        );

        let bounds = LayoutAllocator::compute_rect_bounds(&loc);
        assert_eq!(
            bounds,
            RectBounds {
                x,
                y,
                w: 0.5,
                h: 0.25,
            },
            "bounds mismatch for worker {}",
            worker_idx
        );
    }
}

#[test]
fn test_allocate_overflow_to_over_2_and_beyond() {
    let config = LayoutConfig::default(); // pane_max = 2, tab_max = 8

    // Worker 10 is the 9th overflow worker -> goes to over-2, slot 0
    let loc_w10 = LayoutAllocator::allocate_slot(&SlotRole::Worker(10), &config);
    assert_eq!(
        loc_w10,
        TabSlotLocation {
            tab_name: "over-2".to_string(),
            slot_index: 0,
            row: 0,
            col: 0,
            total_rows: 4,
            total_cols: 2,
        }
    );

    let bounds_w10 = LayoutAllocator::compute_rect_bounds(&loc_w10);
    assert_eq!(
        bounds_w10,
        RectBounds {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.25,
        }
    );

    // Worker 18 -> 18 - 2 = 16 -> 16 / 8 = 2 -> tab over-3, slot 0
    let loc_w18 = LayoutAllocator::allocate_slot(&SlotRole::Worker(18), &config);
    assert_eq!(
        loc_w18,
        TabSlotLocation {
            tab_name: "over-3".to_string(),
            slot_index: 0,
            row: 0,
            col: 0,
            total_rows: 4,
            total_cols: 2,
        }
    );
}

#[test]
fn test_serde_roundtrip_all_types() {
    let role = SlotRole::Worker(42);
    let role_json = serde_json::to_string(&role).expect("serialize SlotRole");
    let role_back: SlotRole = serde_json::from_str(&role_json).expect("deserialize SlotRole");
    assert_eq!(role, role_back);

    let loc = TabSlotLocation {
        tab_name: "over-1".to_string(),
        slot_index: 3,
        row: 1,
        col: 1,
        total_rows: 4,
        total_cols: 2,
    };
    let loc_json = serde_json::to_string(&loc).expect("serialize TabSlotLocation");
    let loc_back: TabSlotLocation =
        serde_json::from_str(&loc_json).expect("deserialize TabSlotLocation");
    assert_eq!(loc, loc_back);

    let bounds = RectBounds {
        x: 0.25,
        y: 0.5,
        w: 0.5,
        h: 0.5,
    };
    let bounds_json = serde_json::to_string(&bounds).expect("serialize RectBounds");
    let bounds_back: RectBounds =
        serde_json::from_str(&bounds_json).expect("deserialize RectBounds");
    assert_eq!(bounds, bounds_back);

    let config = LayoutConfig {
        pane_max: 4,
        tab_max: 12,
    };
    let config_json = serde_json::to_string(&config).expect("serialize LayoutConfig");
    let config_back: LayoutConfig =
        serde_json::from_str(&config_json).expect("deserialize LayoutConfig");
    assert_eq!(config, config_back);
}

#[test]
fn test_slot_role_from_str() {
    assert_eq!(SlotRole::from_str("manager").unwrap(), SlotRole::Manager);
    assert_eq!(SlotRole::from_str("Manager").unwrap(), SlotRole::Manager);
    assert_eq!(SlotRole::from_str("board").unwrap(), SlotRole::Board);
    assert_eq!(SlotRole::from_str("BOARD").unwrap(), SlotRole::Board);
    assert_eq!(
        SlotRole::from_str("worker").unwrap(),
        SlotRole::Worker(0)
    );
    assert_eq!(
        SlotRole::from_str("worker:0").unwrap(),
        SlotRole::Worker(0)
    );
    assert_eq!(
        SlotRole::from_str("worker:5").unwrap(),
        SlotRole::Worker(5)
    );
    assert_eq!(
        SlotRole::from_str("worker-7").unwrap(),
        SlotRole::Worker(7)
    );
    assert_eq!(
        SlotRole::from_str("worker_3").unwrap(),
        SlotRole::Worker(3)
    );
    assert_eq!(
        SlotRole::from_str("worker/2").unwrap(),
        SlotRole::Worker(2)
    );
    assert!(SlotRole::from_str("invalid_role").is_err());
}
