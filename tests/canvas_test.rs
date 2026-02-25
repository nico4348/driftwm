use driftwm::canvas::{CanvasPos, MomentumState, ScreenPos, camera_to_center_window, canvas_to_screen, find_nearest, is_origin_visible, screen_to_canvas};
use driftwm::config::Direction;
use smithay::utils::{Logical, Point, Size};
use std::f64::consts::FRAC_1_SQRT_2;

// --- Coordinate transform round-trip tests ---

#[test]
fn screen_to_canvas_and_back_with_zero_camera() {
    let screen = ScreenPos(Point::from((100.0, 200.0)));
    let camera = Point::from((0.0, 0.0));
    let canvas = screen_to_canvas(screen, camera);
    let back = canvas_to_screen(canvas, camera);
    assert_eq!(back.0.x, screen.0.x);
    assert_eq!(back.0.y, screen.0.y);
}

#[test]
fn screen_to_canvas_and_back_with_positive_camera() {
    let screen = ScreenPos(Point::from((50.0, 75.0)));
    let camera = Point::from((300.0, 400.0));
    let canvas = screen_to_canvas(screen, camera);
    let back = canvas_to_screen(canvas, camera);
    assert!((back.0.x - screen.0.x).abs() < 1e-10);
    assert!((back.0.y - screen.0.y).abs() < 1e-10);
}

#[test]
fn screen_to_canvas_and_back_with_negative_camera() {
    let screen = ScreenPos(Point::from((10.0, 20.0)));
    let camera = Point::from((-150.0, -250.0));
    let canvas = screen_to_canvas(screen, camera);
    let back = canvas_to_screen(canvas, camera);
    assert!((back.0.x - screen.0.x).abs() < 1e-10);
    assert!((back.0.y - screen.0.y).abs() < 1e-10);
}

#[test]
fn canvas_to_screen_and_back_with_positive_camera() {
    let canvas = CanvasPos(Point::from((500.0, 600.0)));
    let camera = Point::from((100.0, 200.0));
    let screen = canvas_to_screen(canvas, camera);
    let back = screen_to_canvas(screen, camera);
    assert!((back.0.x - canvas.0.x).abs() < 1e-10);
    assert!((back.0.y - canvas.0.y).abs() < 1e-10);
}

#[test]
fn screen_to_canvas_adds_camera_offset() {
    let screen = ScreenPos(Point::from((10.0, 20.0)));
    let camera = Point::from((100.0, 200.0));
    let canvas = screen_to_canvas(screen, camera);
    assert_eq!(canvas.0.x, 110.0);
    assert_eq!(canvas.0.y, 220.0);
}

#[test]
fn canvas_to_screen_subtracts_camera_offset() {
    let canvas = CanvasPos(Point::from((110.0, 220.0)));
    let camera = Point::from((100.0, 200.0));
    let screen = canvas_to_screen(canvas, camera);
    assert_eq!(screen.0.x, 10.0);
    assert_eq!(screen.0.y, 20.0);
}

// --- MomentumState tests ---

#[test]
fn momentum_tick_decays_by_friction() {
    let mut m = MomentumState::new(0.96);
    m.velocity = Point::from((10.0, 0.0));
    m.last_scroll_frame = 0;
    let delta = m.tick(1).expect("expected Some delta");
    assert!((delta.x - 10.0).abs() < 1e-10, "tick returns pre-decay velocity");
    assert!((m.velocity.x - 10.0 * 0.96).abs() < 1e-10, "velocity decays by friction");
}

#[test]
fn momentum_tick_stops_below_threshold() {
    let mut m = MomentumState::new(0.96);
    // speed = sqrt(0.1^2 + 0.1^2) ≈ 0.141, speed_sq = 0.02 < threshold_sq 0.25
    m.velocity = Point::from((0.1, 0.1));
    m.last_scroll_frame = 0;
    let result = m.tick(1);
    assert!(result.is_none(), "tick should return None below threshold");
    assert_eq!(m.velocity.x, 0.0);
    assert_eq!(m.velocity.y, 0.0);
}

#[test]
fn momentum_tick_returns_none_when_velocity_zeroed() {
    let mut m = MomentumState::new(0.96);
    m.velocity = Point::from((0.0, 0.0));
    m.last_scroll_frame = 0;
    let result = m.tick(1);
    assert!(result.is_none());
}

#[test]
fn momentum_tick_skips_same_frame_as_last_scroll() {
    let mut m = MomentumState::new(0.96);
    m.accumulate(Point::from((5.0, 5.0)), 5);
    let result = m.tick(5);
    assert!(result.is_none(), "tick on same frame as scroll should return None");
}

#[test]
fn momentum_tick_returns_some_on_next_frame_after_scroll() {
    let mut m = MomentumState::new(0.96);
    m.accumulate(Point::from((5.0, 5.0)), 5);
    m.tick(5); // skip frame 5
    let result = m.tick(6);
    assert!(result.is_some(), "tick on next frame should return Some");
}

#[test]
fn momentum_friction_zero_stops_after_first_tick() {
    let mut m = MomentumState::new(0.0);
    m.velocity = Point::from((10.0, 0.0));
    m.last_scroll_frame = 0;
    let first = m.tick(1);
    assert!(first.is_some(), "first tick should return the velocity");
    // After friction=0.0 is applied, velocity becomes 0.0 * 0.0 = 0.0
    let second = m.tick(2);
    assert!(second.is_none(), "second tick with friction=0 should return None");
}

#[test]
fn momentum_friction_one_never_stops() {
    let mut m = MomentumState::new(1.0);
    m.velocity = Point::from((1.0, 0.0));
    m.last_scroll_frame = 0;
    for frame in 1..=50 {
        let result = m.tick(frame);
        assert!(result.is_some(), "friction=1.0 should never stop, failed at frame {frame}");
        // velocity must stay at exactly 1.0
        assert!((m.velocity.x - 1.0).abs() < 1e-10);
    }
}

#[test]
fn momentum_friction_096_decays_monotonically_and_stops() {
    let mut m = MomentumState::new(0.96);
    m.velocity = Point::from((20.0, 0.0));
    m.last_scroll_frame = 0;
    let mut prev_speed_sq = m.velocity.x.powi(2) + m.velocity.y.powi(2);
    let mut ticked = false;
    for frame in 1..=200 {
        match m.tick(frame) {
            Some(_) => {
                ticked = true;
                let speed_sq = m.velocity.x.powi(2) + m.velocity.y.powi(2);
                assert!(speed_sq < prev_speed_sq, "speed should decrease monotonically at frame {frame}");
                prev_speed_sq = speed_sq;
            }
            None => {
                assert!(ticked, "momentum must tick at least once before stopping");
                break;
            }
        }
    }
}

#[test]
fn momentum_accumulate_ema_weighting() {
    let mut m = MomentumState::new(0.96);
    // Start with zero velocity, apply a delta: result = 0.0 * 0.3 + delta * 0.7
    let delta = Point::from((10.0, 20.0));
    m.accumulate(delta, 1);
    assert!((m.velocity.x - 7.0).abs() < 1e-10, "first accumulate: 0*0.3 + 10*0.7 = 7.0");
    assert!((m.velocity.y - 14.0).abs() < 1e-10, "first accumulate: 0*0.3 + 20*0.7 = 14.0");
}

#[test]
fn momentum_accumulate_second_ema_step() {
    let mut m = MomentumState::new(0.96);
    let delta = Point::from((10.0, 0.0));
    m.accumulate(delta, 1);
    // velocity after first = 7.0
    m.accumulate(delta, 2);
    // velocity = 7.0 * 0.3 + 10.0 * 0.7 = 2.1 + 7.0 = 9.1
    assert!((m.velocity.x - 9.1).abs() < 1e-10, "second accumulate EMA step");
}

#[test]
fn momentum_accumulate_records_frame() {
    let mut m = MomentumState::new(0.96);
    m.accumulate(Point::from((1.0, 0.0)), 42);
    assert_eq!(m.last_scroll_frame, 42);
}

#[test]
fn momentum_stop_zeroes_velocity() {
    let mut m = MomentumState::new(0.96);
    m.velocity = Point::from((10.0, 10.0));
    m.stop();
    assert_eq!(m.velocity.x, 0.0);
    assert_eq!(m.velocity.y, 0.0);
}

#[test]
fn momentum_stop_causes_tick_to_return_none() {
    let mut m = MomentumState::new(0.96);
    m.velocity = Point::from((10.0, 10.0));
    m.last_scroll_frame = 0;
    m.stop();
    let result = m.tick(1);
    assert!(result.is_none());
}

// --- camera_to_center_window tests ---

#[test]
fn camera_to_center_window_standard_window() {
    // Window at (100, 100) size 200x200, viewport 1920x1080
    // window center = (200, 200), viewport center = (960, 540)
    // expected camera = (200 - 960, 200 - 540) = (-760, -340)
    let loc = Point::<i32, Logical>::from((100, 100));
    let win_size = Size::<i32, Logical>::from((200, 200));
    let vp_size = Size::<i32, Logical>::from((1920, 1080));
    let camera = camera_to_center_window(loc, win_size, vp_size);
    assert!((camera.x - (-760.0)).abs() < 1e-10, "camera.x should be -760, got {}", camera.x);
    assert!((camera.y - (-340.0)).abs() < 1e-10, "camera.y should be -340, got {}", camera.y);
}

#[test]
fn camera_to_center_window_small_viewport() {
    // Window at (0, 0) size 100x100, viewport 800x600
    // window center = (50, 50), viewport center = (400, 300)
    // expected camera = (50 - 400, 50 - 300) = (-350, -250)
    let loc = Point::<i32, Logical>::from((0, 0));
    let win_size = Size::<i32, Logical>::from((100, 100));
    let vp_size = Size::<i32, Logical>::from((800, 600));
    let camera = camera_to_center_window(loc, win_size, vp_size);
    assert!((camera.x - (-350.0)).abs() < 1e-10, "camera.x should be -350, got {}", camera.x);
    assert!((camera.y - (-250.0)).abs() < 1e-10, "camera.y should be -250, got {}", camera.y);
}

#[test]
fn camera_to_center_window_far_offset_window() {
    // Window at (1000, 2000) size 400x300, viewport 1920x1080
    // window center = (1200, 2150), viewport center = (960, 540)
    // expected camera = (1200 - 960, 2150 - 540) = (240, 1610)
    let loc = Point::<i32, Logical>::from((1000, 2000));
    let win_size = Size::<i32, Logical>::from((400, 300));
    let vp_size = Size::<i32, Logical>::from((1920, 1080));
    let camera = camera_to_center_window(loc, win_size, vp_size);
    assert!((camera.x - 240.0).abs() < 1e-10, "camera.x should be 240, got {}", camera.x);
    assert!((camera.y - 1610.0).abs() < 1e-10, "camera.y should be 1610, got {}", camera.y);
}

#[test]
fn camera_to_center_window_already_centered_returns_zero() {
    // Window at (860, 440) size 200x200, viewport 1920x1080
    // window center = (960, 540) = viewport center
    // expected camera = (0, 0)
    let loc = Point::<i32, Logical>::from((860, 440));
    let win_size = Size::<i32, Logical>::from((200, 200));
    let vp_size = Size::<i32, Logical>::from((1920, 1080));
    let camera = camera_to_center_window(loc, win_size, vp_size);
    assert!((camera.x).abs() < 1e-10, "camera.x should be 0 for already-centered window, got {}", camera.x);
    assert!((camera.y).abs() < 1e-10, "camera.y should be 0 for already-centered window, got {}", camera.y);
}

// --- Direction::to_unit_vec tests ---

#[test]
fn direction_up_unit_vec() {
    let (x, y) = Direction::Up.to_unit_vec();
    assert_eq!(x, 0.0, "Up x component should be 0");
    assert_eq!(y, -1.0, "Up y component should be -1");
}

#[test]
fn direction_down_unit_vec() {
    let (x, y) = Direction::Down.to_unit_vec();
    assert_eq!(x, 0.0, "Down x component should be 0");
    assert_eq!(y, 1.0, "Down y component should be 1");
}

#[test]
fn direction_left_unit_vec() {
    let (x, y) = Direction::Left.to_unit_vec();
    assert_eq!(x, -1.0, "Left x component should be -1");
    assert_eq!(y, 0.0, "Left y component should be 0");
}

#[test]
fn direction_right_unit_vec() {
    let (x, y) = Direction::Right.to_unit_vec();
    assert_eq!(x, 1.0, "Right x component should be 1");
    assert_eq!(y, 0.0, "Right y component should be 0");
}

#[test]
fn direction_upleft_unit_vec() {
    let (x, y) = Direction::UpLeft.to_unit_vec();
    assert!((x - (-FRAC_1_SQRT_2)).abs() < 1e-15, "UpLeft x should be -FRAC_1_SQRT_2, got {x}");
    assert!((y - (-FRAC_1_SQRT_2)).abs() < 1e-15, "UpLeft y should be -FRAC_1_SQRT_2, got {y}");
}

#[test]
fn direction_upright_unit_vec() {
    let (x, y) = Direction::UpRight.to_unit_vec();
    assert!((x - FRAC_1_SQRT_2).abs() < 1e-15, "UpRight x should be FRAC_1_SQRT_2, got {x}");
    assert!((y - (-FRAC_1_SQRT_2)).abs() < 1e-15, "UpRight y should be -FRAC_1_SQRT_2, got {y}");
}

#[test]
fn direction_downleft_unit_vec() {
    let (x, y) = Direction::DownLeft.to_unit_vec();
    assert!((x - (-FRAC_1_SQRT_2)).abs() < 1e-15, "DownLeft x should be -FRAC_1_SQRT_2, got {x}");
    assert!((y - FRAC_1_SQRT_2).abs() < 1e-15, "DownLeft y should be FRAC_1_SQRT_2, got {y}");
}

#[test]
fn direction_downright_unit_vec() {
    let (x, y) = Direction::DownRight.to_unit_vec();
    assert!((x - FRAC_1_SQRT_2).abs() < 1e-15, "DownRight x should be FRAC_1_SQRT_2, got {x}");
    assert!((y - FRAC_1_SQRT_2).abs() < 1e-15, "DownRight y should be FRAC_1_SQRT_2, got {y}");
}

#[test]
fn cardinal_directions_have_one_zero_component() {
    for dir in [Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
        let (x, y) = dir.to_unit_vec();
        assert!(
            x == 0.0 || y == 0.0,
            "cardinal direction {dir:?} should have one zero component, got ({x}, {y})"
        );
    }
}

#[test]
fn diagonal_directions_have_equal_magnitude_components() {
    for dir in [Direction::UpLeft, Direction::UpRight, Direction::DownLeft, Direction::DownRight] {
        let (x, y) = dir.to_unit_vec();
        assert!(
            (x.abs() - y.abs()).abs() < 1e-15,
            "diagonal direction {dir:?} should have equal-magnitude components, got ({x}, {y})"
        );
    }
}

#[test]
fn all_directions_have_unit_magnitude() {
    let directions = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
        Direction::UpLeft,
        Direction::UpRight,
        Direction::DownLeft,
        Direction::DownRight,
    ];
    for dir in &directions {
        let (x, y) = dir.to_unit_vec();
        let magnitude = (x * x + y * y).sqrt();
        assert!(
            (magnitude - 1.0).abs() < 1e-15,
            "direction {dir:?} unit vec magnitude should be 1.0, got {magnitude}"
        );
    }
}

// --- find_nearest cone search tests ---

/// Helper: build items list from (name, x, y) tuples.
fn items<'a>(positions: &'a [(&'a str, f64, f64)]) -> Vec<(&'a str, Point<f64, Logical>)> {
    positions
        .iter()
        .map(|(name, x, y)| (*name, Point::from((*x, *y))))
        .collect()
}

fn origin(x: f64, y: f64) -> Point<f64, Logical> {
    Point::from((x, y))
}

#[test]
fn find_nearest_directly_right() {
    let w = items(&[("a", 200.0, 0.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, Some("a"));
}

#[test]
fn find_nearest_44_degrees_inside_cone() {
    // tan(44°) ≈ 0.9657 — just inside the ±45° cone
    let w = items(&[("a", 100.0, 96.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, Some("a"), "44° should be inside the 90° cone");
}

#[test]
fn find_nearest_46_degrees_outside_cone() {
    // tan(46°) ≈ 1.0355 — just outside the ±45° cone
    let w = items(&[("a", 100.0, 104.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, None, "46° should be outside the 90° cone");
}

#[test]
fn find_nearest_exactly_45_degrees_is_on_boundary() {
    // At exactly 45°, cross == dot, so cross <= dot → included
    let w = items(&[("a", 100.0, 100.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, Some("a"), "exactly 45° (boundary) should be included");
}

#[test]
fn find_nearest_no_window_in_direction() {
    // Window is behind the origin (to the left), searching right
    let w = items(&[("a", -200.0, 0.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, None);
}

#[test]
fn find_nearest_empty_list() {
    let w: Vec<(&str, Point<f64, Logical>)> = vec![];
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, None);
}

#[test]
fn find_nearest_closest_of_two_candidates_wins() {
    let w = items(&[("far", 500.0, 0.0), ("near", 100.0, 0.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, Some("near"));
}

#[test]
fn find_nearest_skipped_item_is_excluded() {
    let w = items(&[("skip_me", 100.0, 0.0), ("other", 200.0, 0.0)]);
    let result = find_nearest(
        origin(0.0, 0.0),
        &Direction::Right,
        w.into_iter(),
        Some(&"skip_me"),
    );
    assert_eq!(result, Some("other"));
}

#[test]
fn find_nearest_skip_only_candidate_returns_none() {
    let w = items(&[("only", 100.0, 0.0)]);
    let result = find_nearest(
        origin(0.0, 0.0),
        &Direction::Right,
        w.into_iter(),
        Some(&"only"),
    );
    assert_eq!(result, None);
}

#[test]
fn find_nearest_diagonal_direction() {
    // Searching DownRight from origin — window at (100, 100) is directly on that diagonal
    let w = items(&[("a", 100.0, 100.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::DownRight, w.into_iter(), None);
    assert_eq!(result, Some("a"));
}

#[test]
fn find_nearest_diagonal_rejects_perpendicular() {
    // Searching DownRight — window at (-100, 100) is in the DownLeft direction
    let w = items(&[("a", -100.0, 100.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::DownRight, w.into_iter(), None);
    assert_eq!(result, None);
}

#[test]
fn find_nearest_up_direction() {
    // y-axis is inverted (up = negative y)
    let w = items(&[("above", 0.0, -300.0), ("below", 0.0, 300.0)]);
    let result = find_nearest(origin(0.0, 0.0), &Direction::Up, w.into_iter(), None);
    assert_eq!(result, Some("above"));
}

#[test]
fn find_nearest_nonzero_origin() {
    let w = items(&[("a", 600.0, 400.0)]);
    let result = find_nearest(origin(500.0, 400.0), &Direction::Right, w.into_iter(), None);
    assert_eq!(result, Some("a"));
}

// --- is_origin_visible tests ---

fn vp(w: i32, h: i32) -> Size<i32, Logical> {
    Size::from((w, h))
}

fn cam(x: f64, y: f64) -> Point<f64, Logical> {
    Point::from((x, y))
}

#[test]
fn origin_visible_when_camera_centers_on_origin() {
    // camera = (-960, -540) → viewport spans [-960..960, -540..540] — origin is inside
    assert!(is_origin_visible(cam(-960.0, -540.0), vp(1920, 1080)));
}

#[test]
fn origin_visible_at_camera_zero() {
    // camera = (0, 0) → viewport spans [0..1920, 0..1080] — origin is at top-left corner
    assert!(is_origin_visible(cam(0.0, 0.0), vp(1920, 1080)));
}

#[test]
fn origin_not_visible_when_scrolled_far_right() {
    // camera = (500, 0) → viewport spans [500..2420, 0..1080] — origin is left of viewport
    assert!(!is_origin_visible(cam(500.0, 0.0), vp(1920, 1080)));
}

#[test]
fn origin_not_visible_when_scrolled_far_left() {
    // camera = (-2000, 0) → viewport spans [-2000..-80, 0..1080] — origin is right of viewport
    assert!(!is_origin_visible(cam(-2000.0, 0.0), vp(1920, 1080)));
}

#[test]
fn origin_not_visible_when_scrolled_far_down() {
    // camera = (0, 500) → viewport spans [0..1920, 500..1580] — origin is above viewport
    assert!(!is_origin_visible(cam(0.0, 500.0), vp(1920, 1080)));
}

#[test]
fn origin_visible_at_bottom_right_edge() {
    // camera = (-1920, -1080) → viewport spans [-1920..0, -1080..0] — origin at exact corner
    assert!(is_origin_visible(cam(-1920.0, -1080.0), vp(1920, 1080)));
}

#[test]
fn origin_not_visible_just_past_edge() {
    // camera = (-1920.1, -1080) → viewport x spans [-1920.1..-0.1] — origin at x=0 is outside
    assert!(!is_origin_visible(cam(-1920.1, -1080.0), vp(1920, 1080)));
}
