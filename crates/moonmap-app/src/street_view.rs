//! Look Around, drawn here rather than by any platform's view, so it is the
//! same on every system: an equirectangular panorama reprojected to the box
//! on the CPU, whenever the user looks somewhere else. Dragging and scrolling
//! look around, a pinch zooms and a click walks that way.

use {
    crate::net,
    image::{Frame, RgbaImage},
    moonframe::{
        Data, HostView, ViewInit,
        gpui_kit::{
            App, Bounds, Context, Corners, InteractiveElement as _, IntoElement, MouseButton,
            MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
            Render, RenderImage, ScrollDelta, ScrollEvent, SharedString, Styled as _, Window,
            canvas, div, px, rgb, rgba,
        },
    },
    moonmap_core::{Coordinate, Shot, bearing_degrees, destination},
    std::{
        cell::{Cell, RefCell},
        f64::consts::{FRAC_PI_2, PI, TAU},
        rc::Rc,
        sync::Arc,
    },
};

/// How far a click walks, and how far from there another shot may be.
const STEP_METERS: f64 = 15.0;
const STEP_REACH_METERS: f64 = 25.0;
/// Pointer travel, in pixels, past which a press is a drag, not a click.
const CLICK_SLOP: f32 = 4.0;
const WIDEST: f64 = 110.0;
const NARROWEST: f64 = 30.0;
const HIGHEST: f64 = 85.0;

/// An image in memory, three bytes a pixel.
pub struct Panorama {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Where the viewer looks, in radians from the image's middle.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    yaw: f64,
    pitch: f64,
    /// The box's width covers this much.
    fov: f64,
}

impl Look {
    /// The way out of the pixel at `(x, y)` of a box `width` by `height`.
    fn ray(self, x: f64, y: f64, width: f64, height: f64) -> [f64; 3] {
        let focal = width / 2.0 / (self.fov / 2.0).tan();
        let (dx, dy) = (x - width / 2.0, height / 2.0 - y);
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (up, forward) = (
            dy * cos_pitch + focal * sin_pitch,
            focal * cos_pitch - dy * sin_pitch,
        );
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        [
            dx * cos_yaw + forward * sin_yaw,
            up,
            forward * cos_yaw - dx * sin_yaw,
        ]
    }

    /// The yaw of the pixel at `(x, y)`.
    fn yaw_at(self, x: f64, y: f64, width: f64, height: f64) -> f64 {
        let [east, _, north] = self.ray(x, y, width, height);
        east.atan2(north)
    }
}

/// The view from `look`, `width` by `height`, as BGRA with its pixels'
/// rows shared out among the machine's cores.
fn project(panorama: &Panorama, look: Look, width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![255; w * h * 4];
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    let rows = h.div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        for (band, chunk) in out.chunks_mut(rows * w * 4).enumerate() {
            scope.spawn(move || {
                let (pixels, _) = chunk.as_chunks_mut::<4>();
                for (index, pixel) in pixels.iter_mut().enumerate() {
                    let (x, y) = (index % w, band * rows + index / w);
                    let [east, up, north] = look.ray(
                        x as f64 + 0.5,
                        y as f64 + 0.5,
                        f64::from(width),
                        f64::from(height),
                    );
                    let color = sample(panorama, east.atan2(north), up.atan2(east.hypot(north)));
                    pixel[0] = color[2];
                    pixel[1] = color[1];
                    pixel[2] = color[0];
                }
            });
        }
    });
    out
}

/// The panorama's color at `yaw` and `pitch`, between its nearest pixels.
fn sample(panorama: &Panorama, yaw: f64, pitch: f64) -> [u8; 3] {
    let (width, height) = (f64::from(panorama.width), f64::from(panorama.height));
    let u = ((yaw / TAU + 0.5).rem_euclid(1.0) * width - 0.5).rem_euclid(width);
    let v = ((0.5 - pitch / PI) * height - 0.5).clamp(0.0, height - 1.0);
    let (left, top) = (u.floor(), v.floor());
    let (across, down) = (u - left, v - top);
    let right = (left as u32 + 1) % panorama.width;
    let bottom = (top as u32 + 1).min(panorama.height - 1);
    let at = |x: u32, y: u32| (y as usize * panorama.width as usize + x as usize) * 3;
    let corners = [
        at(left as u32, top as u32),
        at(right, top as u32),
        at(left as u32, bottom),
        at(right, bottom),
    ];
    let weights = [
        (1.0 - across) * (1.0 - down),
        across * (1.0 - down),
        (1.0 - across) * down,
        across * down,
    ];
    let mut color = [0; 3];
    for (channel, value) in color.iter_mut().enumerate() {
        let mixed: f64 = corners
            .iter()
            .zip(weights)
            .map(|(&corner, weight)| f64::from(panorama.pixels[corner + channel]) * weight)
            .sum();
        *value = mixed.round() as u8;
    }
    color
}

/// A shot from the `props` the shell gives.
fn shot(props: &Data) -> Option<Shot> {
    let text = |key| props.get(key).and_then(Data::as_str).map(str::to_owned);
    let number = |key| props.get(key).and_then(Data::as_f64);
    Some(Shot {
        id: text("id")?,
        lat: number("lat")?,
        lon: number("lon")?,
        heading: number("heading")?,
        url: text("url")?,
        hd: text("hd")?,
        credit: text("credit")?,
    })
}

/// Which frame is on screen, to know when it must be drawn again.
#[derive(PartialEq)]
struct Drawn {
    panorama: usize,
    look: [u64; 3],
    size: (u32, u32),
}

/// The last frame, and what it shows.
type Shown = Rc<RefCell<Option<(Drawn, Arc<RenderImage>)>>>;

struct Press {
    start: Point<Pixels>,
    last: Point<Pixels>,
    dragged: bool,
}

/// The `street-view` view.
pub struct StreetView {
    shot: Option<Shot>,
    token: String,
    panorama: Option<Arc<Panorama>>,
    look: Look,
    press: Option<Press>,
    walking: bool,
    /// Which load is the newest.
    serial: u64,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    frame: Shown,
}

impl StreetView {
    pub fn new(init: &ViewInit, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            shot: None,
            token: String::new(),
            panorama: None,
            look: Look {
                yaw: 0.0,
                pitch: 0.0,
                fov: 90f64.to_radians(),
            },
            press: None,
            walking: false,
            serial: 0,
            bounds: Rc::default(),
            frame: Rc::default(),
        };
        view.set_props(init.props(), cx);
        view
    }

    /// Moves to `shot`, still facing `bearing`, in degrees.
    fn show(&mut self, shot: Shot, bearing: f64, cx: &mut Context<Self>) {
        self.look.yaw = (bearing - shot.heading).to_radians();
        self.serial += 1;
        let serial = self.serial;
        let (small, large) = (shot.url.clone(), shot.hd.clone());
        self.shot = Some(shot);
        cx.spawn(async move |this, cx| {
            for url in [small, large] {
                let task = cx
                    .background_executor()
                    .spawn(async move { net::panorama(&url) });
                let Ok(panorama) = task.await else { continue };
                let shown = this.update(cx, |view, cx| {
                    if view.serial == serial {
                        view.panorama = Some(Arc::new(panorama));
                        cx.notify();
                    }
                    view.serial == serial
                });
                if !matches!(shown, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    /// Walks from the shot toward the pixel at `at`, if a shot is there.
    fn walk(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(from) = self.shot.clone() else {
            return;
        };
        if self.walking {
            return;
        }
        let bounds = self.bounds.get();
        let (width, height) = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        let (x, y) = (
            f64::from(at.x - bounds.origin.x),
            f64::from(at.y - bounds.origin.y),
        );
        let way = from.heading + self.look.yaw_at(x, y, width, height).to_degrees();
        let facing = from.heading + self.look.yaw.to_degrees();
        let ahead = destination(from.coordinate(), way, STEP_METERS);
        let token = self.token.clone();
        self.walking = true;
        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { net::shot(ahead, STEP_REACH_METERS, &token, Some(&from.id)) });
            let found = task.await;
            let _ = this.update(cx, |view, cx| {
                view.walking = false;
                if let Some(next) = found {
                    view.show(next, facing, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn turn(&mut self, dx: f64, dy: f64) {
        let per_pixel = self.look.fov / f64::from(self.bounds.get().size.width).max(1.0);
        self.look.yaw = (self.look.yaw - dx * per_pixel).rem_euclid(TAU);
        let most = HIGHEST.to_radians().min(FRAC_PI_2);
        self.look.pitch = (self.look.pitch + dy * per_pixel).clamp(-most, most);
    }

    fn zoom(&mut self, by: f64) {
        let fov = self.look.fov.to_degrees() * by;
        self.look.fov = fov.clamp(NARROWEST, WIDEST).to_radians();
    }

    fn pressed(&mut self, event: &MouseDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.press = Some(Press {
            start: event.position,
            last: event.position,
            dragged: false,
        });
    }

    fn moved(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = &mut self.press else {
            return;
        };
        let (dx, dy) = (
            event.position.x - press.last.x,
            event.position.y - press.last.y,
        );
        let from_start = event.position - press.start;
        press.last = event.position;
        press.dragged |= f32::from(from_start.x).hypot(f32::from(from_start.y)) > CLICK_SLOP;
        if press.dragged {
            self.turn(f64::from(dx), f64::from(dy));
            cx.notify();
        }
    }

    fn released(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(press) = self.press.take() {
            if !press.dragged {
                self.walk(event.position, cx);
            }
            cx.notify();
        }
    }

    fn scrolled(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.delta {
            ScrollDelta::Pixels(delta) => self.turn(f64::from(delta.x), f64::from(delta.y)),
            ScrollDelta::Lines(delta) => self.zoom(1.0 - f64::from(delta.y) * 0.08),
        }
        cx.notify();
    }
}

impl HostView for StreetView {
    fn set_props(&mut self, props: &Data, cx: &mut Context<Self>) {
        self.token = props
            .get("token")
            .and_then(Data::as_str)
            .unwrap_or_default()
            .to_owned();
        let Some(next) = shot(props) else {
            return;
        };
        if self.shot.as_ref().map(|shot| &shot.id) == Some(&next.id) {
            return;
        }
        // Facing what was asked for, as the map's pin is.
        let target = props
            .get("toward_lat")
            .and_then(Data::as_f64)
            .zip(props.get("toward_lon").and_then(Data::as_f64))
            .map(|(lat, lon)| Coordinate::new(lat, lon));
        let facing = target.map_or(next.heading, |to| bearing_degrees(next.coordinate(), to));
        self.panorama = None;
        self.show(next, facing, cx);
    }
}

fn paint(
    bounds: Bounds<Pixels>,
    panorama: &Panorama,
    look: Look,
    scale: f32,
    frame: &Shown,
    window: &mut Window,
    cx: &mut App,
) {
    let (width, height) = (
        (f32::from(bounds.size.width) * scale).round() as u32,
        (f32::from(bounds.size.height) * scale).round() as u32,
    );
    if width == 0 || height == 0 {
        return;
    }
    let drawn = Drawn {
        panorama: std::ptr::from_ref(panorama) as usize,
        look: [look.yaw.to_bits(), look.pitch.to_bits(), look.fov.to_bits()],
        size: (width, height),
    };
    let mut frame = frame.borrow_mut();
    if frame.as_ref().is_none_or(|(last, _)| *last != drawn) {
        let Some(image) =
            RgbaImage::from_raw(width, height, project(panorama, look, width, height))
        else {
            return;
        };
        let image = Arc::new(RenderImage::new(vec![Frame::new(image)]));
        if let Some((_, old)) = frame.replace((drawn, image)) {
            cx.drop_image(old, Some(window));
        }
    }
    if let Some((_, image)) = frame.as_ref() {
        let _ = window.paint_image(
            bounds,
            bounds,
            Corners::default(),
            Arc::clone(image),
            0,
            false,
        );
    }
}

impl Render for StreetView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (bounds, frame) = (Rc::clone(&self.bounds), Rc::clone(&self.frame));
        let (panorama, look, dragging) = (self.panorama.clone(), self.look, self.press.is_some());
        let credit: Option<SharedString> = self.shot.as_ref().map(|s| s.credit.clone().into());
        div()
            .id("street-view")
            .relative()
            .size_full()
            .overflow_hidden()
            // The map lies behind, and must not zoom as the view is scrolled.
            .occlude()
            .bg(rgb(0x1c1c1e))
            .cursor_grab()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pressed))
            .on_mouse_move(cx.listener(Self::moved))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::released))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::released))
            .on_scroll(cx.listener(Self::scrolled))
            .on_pinch(
                cx.listener(|view, event: &moonframe::gpui_kit::PinchEvent, _, cx| {
                    view.zoom(1.0 - f64::from(event.delta));
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    move |area, _, _| bounds.set(area),
                    move |area, (), window, cx| {
                        if let Some(panorama) = &panorama {
                            // Sharp once the view settles.
                            let scale = if dragging { 1.0 } else { window.scale_factor() };
                            paint(area, panorama, look, scale, &frame, window, cx);
                        }
                    },
                )
                .size_full(),
            )
            .children(credit.map(|credit| {
                div()
                    .absolute()
                    .bottom(px(6.))
                    .right(px(8.))
                    .px(px(6.))
                    .rounded(px(4.))
                    .bg(rgba(0x0000_0080))
                    .text_color(rgb(0xffffff))
                    .text_xs()
                    .child(credit)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red ahead of the camera's middle, blue behind.
    fn panorama() -> Panorama {
        let (width, height) = (64u32, 32u32);
        let mut pixels = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                let ahead = (x as i32 - 32).abs() < 16;
                pixels.extend(if ahead { [255, 0, 0] } else { [0, 0, 255] });
            }
        }
        Panorama {
            width,
            height,
            pixels,
        }
    }

    fn middle(look: Look) -> [u8; 4] {
        let out = project(&panorama(), look, 16, 16);
        let at = (8 * 16 + 8) * 4;
        [out[at], out[at + 1], out[at + 2], out[at + 3]]
    }

    #[test]
    fn looking_turns_the_panorama_and_writes_bgra() {
        let mut look = Look {
            yaw: 0.0,
            pitch: 0.0,
            fov: 1.5,
        };
        assert_eq!(middle(look), [0, 0, 255, 255]);
        look.yaw = PI;
        assert_eq!(middle(look), [255, 0, 0, 255]);
    }

    #[test]
    fn the_box_middle_faces_the_look_and_edges_spread_by_the_fov() {
        let look = Look {
            yaw: 0.4,
            pitch: 0.2,
            fov: 90f64.to_radians(),
        };
        assert!((look.yaw_at(400.0, 300.0, 800.0, 600.0) - 0.4).abs() < 1e-9);
        let level = Look { pitch: 0.0, ..look };
        let right = level.yaw_at(800.0, 300.0, 800.0, 600.0);
        assert!((right - (0.4 + 45f64.to_radians())).abs() < 1e-9);
    }
}
