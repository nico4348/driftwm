use smithay::utils::{Logical, Point, Size};

use crate::config::Direction;

/// A position in screen-local coordinates (0,0 = top-left of the output).
#[derive(Debug, Clone, Copy)]
pub struct ScreenPos(pub Point<f64, Logical>);

/// A position in infinite canvas coordinates (absolute world position).
#[derive(Debug, Clone, Copy)]
pub struct CanvasPos(pub Point<f64, Logical>);

#[inline]
pub fn screen_to_canvas(screen: ScreenPos, camera: Point<f64, Logical>) -> CanvasPos {
    CanvasPos(screen.0 + camera)
}

#[inline]
pub fn canvas_to_screen(canvas: CanvasPos, camera: Point<f64, Logical>) -> ScreenPos {
    ScreenPos(canvas.0 - camera)
}

/// Compute the camera position that centers a window in the viewport.
pub fn camera_to_center_window(
    window_loc: Point<i32, Logical>,
    window_size: Size<i32, Logical>,
    viewport_size: Size<i32, Logical>,
) -> Point<f64, Logical> {
    let window_center_x = window_loc.x as f64 + window_size.w as f64 / 2.0;
    let window_center_y = window_loc.y as f64 + window_size.h as f64 / 2.0;
    let viewport_center_x = viewport_size.w as f64 / 2.0;
    let viewport_center_y = viewport_size.h as f64 / 2.0;
    Point::from((
        window_center_x - viewport_center_x,
        window_center_y - viewport_center_y,
    ))
}

/// Check whether the canvas origin (0, 0) is visible in the current viewport.
pub fn is_origin_visible(camera: Point<f64, Logical>, viewport_size: Size<i32, Logical>) -> bool {
    camera.x <= 0.0
        && 0.0 <= camera.x + viewport_size.w as f64
        && camera.y <= 0.0
        && 0.0 <= camera.y + viewport_size.h as f64
}

/// Find the nearest item in a 90° cone from `origin` in the given direction.
///
/// Uses dot/cross product against the direction unit vector: a candidate is
/// in the cone when `dot > 0 && |cross| <= dot` (i.e. within ±45° of the
/// direction). Among candidates, picks the nearest by Euclidean distance.
///
/// Generic over the item type so it works with `Window` in production and
/// simple types (e.g. `&str`) in tests.
pub fn find_nearest<W: PartialEq>(
    origin: Point<f64, Logical>,
    dir: &Direction,
    items: impl Iterator<Item = (W, Point<f64, Logical>)>,
    skip: Option<&W>,
) -> Option<W> {
    let (ux, uy) = dir.to_unit_vec();
    let mut best: Option<(W, f64)> = None;

    for (item, center) in items {
        if skip.is_some_and(|s| s == &item) {
            continue;
        }
        let dx = center.x - origin.x;
        let dy = center.y - origin.y;
        let dot = dx * ux + dy * uy;
        let cross = (dx * uy - dy * ux).abs();
        if dot > 0.0 && cross <= dot {
            let dist_sq = dx * dx + dy * dy;
            if best.as_ref().is_none_or(|(_, d)| dist_sq < *d) {
                best = Some((item, dist_sq));
            }
        }
    }

    best.map(|(w, _)| w)
}

/// Scroll momentum physics: velocity decays by friction each frame.
/// Uses EMA (exponential moving average) for accumulation to smooth
/// out jittery trackpad deltas.
pub struct MomentumState {
    pub velocity: Point<f64, Logical>,
    pub friction: f64,
    /// Stop when |velocity|^2 < threshold_sq (default 0.25 = 0.5 px/frame)
    pub threshold_sq: f64,
    /// Frame number of the last scroll event. Prevents double-counting
    /// camera movement on frames where a scroll event fired.
    pub last_scroll_frame: u64,
}

impl MomentumState {
    pub fn new(friction: f64) -> Self {
        Self {
            velocity: Point::from((0.0, 0.0)),
            friction,
            threshold_sq: 0.25,
            last_scroll_frame: 0,
        }
    }

    /// EMA accumulate: velocity = velocity * 0.3 + delta * 0.7
    pub fn accumulate(&mut self, delta: Point<f64, Logical>, frame: u64) {
        self.velocity = Point::from((
            self.velocity.x * 0.3 + delta.x * 0.7,
            self.velocity.y * 0.3 + delta.y * 0.7,
        ));
        self.last_scroll_frame = frame;
    }

    /// Returns Some(delta) to apply, or None if skipped/finished.
    pub fn tick(&mut self, current_frame: u64) -> Option<Point<f64, Logical>> {
        // Skip on frames where a scroll event already moved the camera
        if self.last_scroll_frame == current_frame {
            return None;
        }
        if self.velocity.x.powi(2) + self.velocity.y.powi(2) < self.threshold_sq {
            self.velocity = Point::from((0.0, 0.0));
            return None;
        }
        let delta = self.velocity;
        self.velocity = Point::from((delta.x * self.friction, delta.y * self.friction));
        Some(delta)
    }

    pub fn stop(&mut self) {
        self.velocity = Point::from((0.0, 0.0));
    }
}
