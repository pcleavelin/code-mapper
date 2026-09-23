//! The window and its event loop: winit events become one frame's `Input`, the app builds and
//! draws a frame when input arrived or it asked to be redrawn, and a test script from
//! `CODEMAP_SCRIPT` can stand in for the mouse and keyboard.

use crate::gfx::{Color, Gfx};
use crate::ui::{Input, Key, Mods};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

/// What a frame asks of the loop.
pub struct Frame {
    pub redraw_after: Duration,
    pub quit: bool,
    pub clear: Color,
    pub cursor: CursorIcon,
}

pub trait App {
    fn frame(&mut self, gfx: &mut Gfx, input: &mut Input) -> Frame;
    /// A line of a test script the loop did not understand: the app's own commands. False
    /// means the line is not done yet and is run again next frame.
    fn script(&mut self, line: &str) -> bool;
    /// The centre of the element a script names, from last frame's rectangles.
    fn locate(&mut self, name: &str) -> Option<(i32, i32)>;
}

/// A test script from `CODEMAP_SCRIPT=<file>`: one command per line, fed to the app as if the
/// mouse and keyboard had done it. `wait <n>` lets n frames pass; `mouse <x> <y>`, `down`, `up`,
/// `click <x> <y>`, `dblclick <x> <y>`, `drag <x0> <y0> <x1> <y1>`, `wheel <dy> [ctrl|shift]`,
/// `down [ctrl|shift|alt] [twice]` (`twice` makes the press a double-click),
/// `key <name> [ctrl] [alt]`, `text <chars>`, `quit`; anything else goes to the app. Every
/// input command is its own frame, so the app sees it exactly as a real event.
struct Script {
    lines: Vec<String>,
    pc: usize,
    wait: u32,
}

impl Script {
    fn load() -> Option<Script> {
        let path = std::env::var_os("CODEMAP_SCRIPT")?;
        let text = std::fs::read_to_string(&path).ok()?;
        Some(Script { lines: text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect(), pc: 0, wait: 0 })
    }
}

struct Runner<A: App> {
    app: A,
    title: String,
    gfx: Option<Gfx>,
    input: Input,
    mods: Mods,
    last_click: Option<(Instant, u8, (i32, i32))>,
    pending: bool, // input arrived since the last frame
    next_redraw: Instant,
    start: Instant,
    cursor: CursorIcon,
    script: Option<Script>,
    // frames built since the last `dump`, for the script harness's frame-time line
    frames: u32,
    frame_max: Duration,
    frame_over: u32,
}

impl<A: App> Runner<A> {
    /// Run the script up to and including the next input command or wait. Returns whether to
    /// quit.
    fn step_script(&mut self) -> bool {
        let Some(sc) = self.script.as_mut() else { return false };
        if sc.wait > 0 {
            sc.wait -= 1;
            return false;
        }
        self.input.mods = self.mods;
        loop {
            let Some(line) = sc.lines.get(sc.pc).cloned() else { return false };
            sc.pc += 1;
            let w: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| w.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            let mods = |from: usize| Mods { ctrl: w[from.min(w.len())..].contains(&"ctrl"), shift: w[from.min(w.len())..].contains(&"shift"), alt: w[from.min(w.len())..].contains(&"alt") };
            match w[0] {
                "wait" => {
                    sc.wait = (num(1).max(1) - 1) as u32;
                    return false;
                }
                "mouse" => self.input.mouse = (num(1), num(2)),
                "down" => {
                    self.input.down[0] = true;
                    self.input.pressed[0] = true;
                    self.input.clicks[0] = if w.contains(&"twice") { 2 } else { 1 };
                    self.input.mods = mods(1);
                    return false;
                }
                "up" => {
                    self.input.down[0] = false;
                    return false;
                }
                "click" | "dblclick" => {
                    // expands into its own frames; the press carries the click count
                    let mut rest: String = w[3.min(w.len())..].join(" ");
                    if w[0] == "dblclick" {
                        rest.push_str(" twice");
                    }
                    let at = sc.pc;
                    sc.lines.splice(at..at, [format!("mouse {} {}", w[1], w[2]), format!("down {rest}"), "wait 1".into(), "up".into(), "wait 1".into()]);
                }
                "click-id" | "hover-id" | "dblclick-id" => {
                    // the element by name, then the plain form of the same gesture
                    let Some(name) = w.get(1) else { continue };
                    match self.app.locate(name) {
                        Some((x, y)) => {
                            let verb = match &w[0][..w[0].len() - 3] {
                                "hover" => "mouse",
                                v => v,
                            };
                            let line = format!("{verb} {x} {y} {}", w[2.min(w.len())..].join(" "));
                            let at = sc.pc;
                            sc.lines.insert(at, line);
                        }
                        None => eprintln!("script: no element '{name}' last frame"),
                    }
                }
                "drag" => {
                    let (x0, y0, x1, y1) = (num(1), num(2), num(3), num(4));
                    let n = 8;
                    let mut ins = vec![format!("mouse {x0} {y0}"), "down".into(), "wait 1".into()];
                    for i in 1..=n {
                        ins.push(format!("mouse {} {}", x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n));
                        ins.push("wait 1".into());
                    }
                    ins.push("up".into());
                    ins.push("wait 1".into());
                    let at = sc.pc;
                    sc.lines.splice(at..at, ins);
                }
                "wheel" => {
                    self.input.wheel.1 += num(1) as f32;
                    self.input.mods = mods(2);
                    return false;
                }
                "key" => {
                    let k = match w.get(1).copied().unwrap_or("") {
                        "enter" => Key::Enter,
                        "escape" => Key::Escape,
                        "backspace" => Key::Backspace,
                        "left" => Key::Left,
                        "right" => Key::Right,
                        "up" => Key::Up,
                        "down" => Key::Down,
                        other => Key::Char(other.chars().next().unwrap_or(' ')),
                    };
                    self.input.keys.push((k, mods(2)));
                    return false;
                }
                "text" => {
                    self.input.text.push_str(&w[1..].join(" "));
                    return false;
                }
                "quit" => return true,
                _ => {
                    if line == "dump" {
                        eprintln!("DUMP frames n={} max={:.1} over16={} t={:.1}", self.frames, self.frame_max.as_secs_f64() * 1000.0, self.frame_over, self.start.elapsed().as_secs_f64() * 1000.0);
                        (self.frames, self.frame_max, self.frame_over) = (0, Duration::ZERO, 0);
                    }
                    if !self.app.script(&line) {
                        sc.pc -= 1;
                        return false;
                    }
                    if line.starts_with("shot") {
                        return false; // the screenshot is of the frame that follows, before the next command
                    }
                }
            }
        }
    }
}

impl<A: App> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title(&self.title).with_inner_size(winit::dpi::LogicalSize::new(1600.0, 1000.0)).with_maximized(true);
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gfx = Gfx::new(window);
        self.input.size = gfx.size;
        self.gfx = Some(gfx);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                gfx.resize(s.width, s.height);
                self.input.size = gfx.size;
                self.pending = true;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                gfx.scale = scale_factor as f32;
                self.pending = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.mods = Mods { ctrl: s.control_key(), shift: s.shift_key(), alt: s.alt_key() };
                self.input.mods = self.mods;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse = (position.x as i32, position.y as i32);
                self.pending = true;
            }
            WindowEvent::CursorLeft { .. } => {
                self.input.mouse = (-1, -1);
                self.pending = true;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Back => {
                        if state == ElementState::Pressed {
                            self.input.back = true;
                        }
                        self.pending = true;
                        return;
                    }
                    MouseButton::Forward => {
                        if state == ElementState::Pressed {
                            self.input.forward = true;
                        }
                        self.pending = true;
                        return;
                    }
                    _ => return,
                };
                match state {
                    ElementState::Pressed => {
                        self.input.down[b] = true;
                        self.input.pressed[b] = true;
                        let now = Instant::now();
                        let near = |a: (i32, i32), c: (i32, i32)| (a.0 - c.0).abs() < 4 && (a.1 - c.1).abs() < 4;
                        let double = matches!(self.last_click, Some((t, lb, p)) if lb == b as u8 && now.duration_since(t) < Duration::from_millis(350) && near(p, self.input.mouse));
                        self.input.clicks[b] = if double { 2 } else { 1 };
                        self.last_click = if double { None } else { Some((now, b as u8, self.input.mouse)) };
                    }
                    ElementState::Released => self.input.down[b] = false,
                }
                self.pending = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                self.input.wheel.0 += x;
                self.input.wheel.1 += y;
                self.pending = true;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    let key = match &event.logical_key {
                        WKey::Named(NamedKey::Enter) => Some(Key::Enter),
                        WKey::Named(NamedKey::Escape) => Some(Key::Escape),
                        WKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
                        WKey::Named(NamedKey::Delete) => Some(Key::Delete),
                        WKey::Named(NamedKey::ArrowLeft) => Some(Key::Left),
                        WKey::Named(NamedKey::ArrowRight) => Some(Key::Right),
                        WKey::Named(NamedKey::ArrowUp) => Some(Key::Up),
                        WKey::Named(NamedKey::ArrowDown) => Some(Key::Down),
                        WKey::Named(NamedKey::Home) => Some(Key::Home),
                        WKey::Named(NamedKey::End) => Some(Key::End),
                        WKey::Character(s) => s.chars().next().map(Key::Char),
                        _ => None,
                    };
                    if let Some(k) = key {
                        self.input.keys.push((k, self.mods));
                    }
                    if let Some(t) = &event.text {
                        if !self.mods.ctrl && !self.mods.alt {
                            self.input.text.extend(t.chars().filter(|c| !c.is_control()));
                        }
                    }
                }
                self.pending = true;
            }
            WindowEvent::RedrawRequested => {
                self.input.time = self.start.elapsed().as_secs_f64();
                let scripted_quit = self.step_script();
                let gfx = self.gfx.as_mut().unwrap();
                let t0 = Instant::now();
                gfx.begin();
                let out = self.app.frame(gfx, &mut self.input);
                gfx.render(out.clear);
                if out.cursor != self.cursor {
                    self.cursor = out.cursor;
                    gfx.window.set_cursor(out.cursor);
                }
                let d = t0.elapsed();
                self.frames += 1;
                self.frame_max = self.frame_max.max(d);
                self.frame_over += (d > Duration::from_millis(16)) as u32;
                self.input.end_frame();
                self.pending = false;
                self.next_redraw = Instant::now() + if self.script.is_some() { Duration::from_millis(8) } else { out.redraw_after };
                if out.quit || scripted_quit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gfx) = self.gfx.as_ref() else { return };
        let due = Instant::now() >= self.next_redraw;
        if self.pending || due {
            gfx.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw));
    }
}

pub fn run(title: &str, app: impl App + 'static) {
    let event_loop = EventLoop::new().expect("event loop");
    let mut runner = Runner { app, title: title.to_owned(), gfx: None, input: Input::default(), mods: Mods::default(), last_click: None, pending: true, next_redraw: Instant::now(), start: Instant::now(), cursor: CursorIcon::Default, script: Script::load(), frames: 0, frame_max: Duration::ZERO, frame_over: 0 };
    event_loop.run_app(&mut runner).expect("run");
}
