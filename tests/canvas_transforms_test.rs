use driftwm::canvas::{
    CanvasPos, MomentumState, ScreenPos,
    canvas_to_screen, screen_to_canvas,
};
use smithay::utils::Point;

// --- Coordinate transform round-trip tests (zoom=1.0) ---

#[test]
fn screen_to_canvas_and_back_with_zero_camera() {
    let screen = ScreenPos(Point::from((100.0, 200.0)));
    let camera = Point::from((0.0, 0.0));
    let canvas = screen_to_canvas(screen, camera, 1.0);
    let back = canvas_to_screen(canvas, camera, 1.0);
    assert_eq!(back.0.x, screen.0.x);
    assert_eq!(back.0.y, screen.0.y);
}

#[test]
fn screen_to_canvas_and_back_with_positive_camera() {
    let screen = ScreenPos(Point::from((50.0, 75.0)));
    let camera = Point::from((300.0, 400.0));
    let canvas = screen_to_canvas(screen, camera, 1.0);
    let back = canvas_to_screen(canvas, camera, 1.0);
    assert!((back.0.x - screen.0.x).abs() < 1e-10);
    assert!((back.0.y - screen.0.y).abs() < 1e-10);
}

#[test]
fn screen_to_canvas_and_back_with_negative_camera() {
    let screen = ScreenPos(Point::from((10.0, 20.0)));
    let camera = Point::from((-150.0, -250.0));
    let canvas = screen_to_canvas(screen, camera, 1.0);
    let back = canvas_to_screen(canvas, camera, 1.0);
    assert!((back.0.x - screen.0.x).abs() < 1e-10);
    assert!((back.0.y - screen.0.y).abs() < 1e-10);
}

#[test]
fn canvas_to_screen_and_back_with_positive_camera() {
    let canvas = CanvasPos(Point::from((500.0, 600.0)));
    let camera = Point::from((100.0, 200.0));
    let screen = canvas_to_screen(canvas, camera, 1.0);
    let back = screen_to_canvas(screen, camera, 1.0);
    assert!((back.0.x - canvas.0.x).abs() < 1e-10);
    assert!((back.0.y - canvas.0.y).abs() < 1e-10);
}

#[test]
fn screen_to_canvas_adds_camera_offset() {
    let screen = ScreenPos(Point::from((10.0, 20.0)));
    let camera = Point::from((100.0, 200.0));
    let canvas = screen_to_canvas(screen, camera, 1.0);
    assert_eq!(canvas.0.x, 110.0);
    assert_eq!(canvas.0.y, 220.0);
}

#[test]
fn canvas_to_screen_subtracts_camera_offset() {
    let canvas = CanvasPos(Point::from((110.0, 220.0)));
    let camera = Point::from((100.0, 200.0));
    let screen = canvas_to_screen(canvas, camera, 1.0);
    assert_eq!(screen.0.x, 10.0);
    assert_eq!(screen.0.y, 20.0);
}

// --- Zoom coordinate transform tests ---

#[test]
fn screen_to_canvas_with_zoom_half() {
    // screen=100, camera=0, zoom=0.5 → canvas = 100/0.5 + 0 = 200
    let screen = ScreenPos(Point::from((100.0, 50.0)));
    let camera = Point::from((0.0, 0.0));
    let canvas = screen_to_canvas(screen, camera, 0.5);
    assert!((canvas.0.x - 200.0).abs() < 1e-10);
    assert!((canvas.0.y - 100.0).abs() < 1e-10);
}

#[test]
fn canvas_to_screen_with_zoom_half() {
    // canvas=200, camera=0, zoom=0.5 → screen = (200-0)*0.5 = 100
    let canvas = CanvasPos(Point::from((200.0, 100.0)));
    let camera = Point::from((0.0, 0.0));
    let screen = canvas_to_screen(canvas, camera, 0.5);
    assert!((screen.0.x - 100.0).abs() < 1e-10);
    assert!((screen.0.y - 50.0).abs() < 1e-10);
}

#[test]
fn zoom_round_trip_with_camera_and_zoom() {
    let screen = ScreenPos(Point::from((300.0, 200.0)));
    let camera = Point::from((100.0, 50.0));
    let zoom = 0.7;
    let canvas = screen_to_canvas(screen, camera, zoom);
    let back = canvas_to_screen(canvas, camera, zoom);
    assert!((back.0.x - screen.0.x).abs() < 1e-10);
    assert!((back.0.y - screen.0.y).abs() < 1e-10);
}

#[test]
fn screen_to_canvas_zoom_one_equals_no_zoom() {
    let screen = ScreenPos(Point::from((50.0, 75.0)));
    let camera = Point::from((300.0, 400.0));
    let with_zoom = screen_to_canvas(screen, camera, 1.0);
    // At zoom=1: canvas = screen/1 + camera = screen + camera
    assert!((with_zoom.0.x - 350.0).abs() < 1e-10);
    assert!((with_zoom.0.y - 475.0).abs() < 1e-10);
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
