use std::collections::HashMap;
use std::hash::Hash;
use std::io::{Read, Write};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender};
use std::{fs, thread};

use fontdue::{Font, FontSettings, Metrics};
use portable_pty::{CommandBuilder, PtyPair, PtySize, PtySystem, native_pty_system};
use softbuffer::Surface;
use std::sync::mpsc::channel;
use winit::application::ApplicationHandler;
use winit::event::ElementState::Pressed;
use winit::event::WindowEvent::KeyboardInput;
use winit::event::{self, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::Key::Named;
use winit::keyboard::PhysicalKey::{self, Code};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: usize,
    y: usize,
}

#[derive(Default)]
struct App {
    window: Option<Rc<dyn Window>>,
    pty: Option<PtyPair>,
    writer: Option<Box<dyn Write + Send>>,
    context: Option<softbuffer::Context<Rc<dyn Window>>>,
    surface: Option<softbuffer::Surface<Rc<dyn Window>, Rc<dyn Window>>>,
    fontmap: HashMap<char, (Metrics, Vec<u8>)>,
    grid: Vec<char>,
    channel_sender: Option<mpsc::Sender<Vec<u8>>>,
    channel_recv: Option<mpsc::Receiver<Vec<u8>>>,
    cursor_position: Option<Point>,
}

impl App {
    fn init(&mut self) {
        self.fontmap = HashMap::new();
        self.cursor_position = Some(Point { x: 0, y: 0 });

        self.grid = vec![' '; 6400];

        let (sender, receiver) = channel();
        self.channel_sender = Some(sender);
        self.channel_recv = Some(receiver);

        let font =
            include_bytes!("/Users/manosriram/dev/tilde/src/assets/monospace-argon.otf") as &[u8];
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        for x in font.chars() {
            let f = font.rasterize(*x.0, 13.0);
            self.fontmap.insert(*x.0, f);
        }
        println!("{:?}", self.fontmap.get(&'a'));

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 80,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let cmd = CommandBuilder::new("bash");
        pair.slave.spawn_command(cmd).unwrap();

        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut buf = [0u8; 4096];
        let tx = self.channel_sender.clone().unwrap();
        let r = thread::spawn(move || {
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    // Ok(1) => break,
                    Ok(n) => {
                        // print!("zz = {}", String::from_utf8_lossy(&buf[..n]));
                        tx.send(buf[..n].to_vec());
                    }
                    Err(e) => {
                        print!("{}", e)
                    }
                }
            }
        });

        self.writer = Some(pair.master.take_writer().unwrap());
        self.pty = Some(pair);
    }

    fn run(&mut self, command: &str) {
        if let Some(writer) = self.writer.as_mut() {
            println!("running run");
            writeln!(writer, "{command}\r\n").unwrap();
        }
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let window: Rc<dyn Window> = event_loop
            .create_window(WindowAttributes::default().with_title("tilde"))
            .unwrap()
            .into();

        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        // println!("grid : {:?}", self.grid);
        let cm = self.channel_recv.as_ref().unwrap().try_recv();
        match cm {
            Ok(v) => {
                let r = String::from_utf8(v);
                for ch in r.unwrap().chars() {
                    if ch == '\n' {
                        self.cursor_position.as_mut().unwrap().y += 1;
                    }
                    if ch == '\r' {
                        self.cursor_position.as_mut().unwrap().x = 0;
                    }

                    if self.cursor_position.as_mut().unwrap().x > 80 {
                        self.cursor_position.as_mut().unwrap().y += 1;
                        self.cursor_position.as_mut().unwrap().x = 0;
                    }

                    self.grid
                        [self.cursor_position.unwrap().y * 80 + self.cursor_position.unwrap().x] =
                        ch;
                    self.cursor_position.as_mut().unwrap().x += 1;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            Err(e) => {}
        };

        // println!("{:?}", cm);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                println!("requesting after run");
                println!("grid : {:?}", self.grid);

                let size = self.window.as_ref().unwrap().surface_size();
                let (width, height) = (size.width, size.height);
                let Some(surface) = self.surface.as_mut() else {
                    return;
                };
                if width == 0 || height == 0 {
                    return;
                }
                surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();

                for y in 0..80 {
                    for x in 0..80 {
                        let fmr = self.fontmap.get(&self.grid[(y * 80 + x) as usize]);
                        match fmr {
                            Some(fmi) => {
                                let px = x * 13;
                                let py = y * 13;

                                let (metrics, bitmap) = fmi;
                                for gy in 0..metrics.height {
                                    for gx in 0..metrics.width {
                                        let cvg = bitmap[gy * metrics.width + gx];

                                        let screen_x = px + metrics.xmin as usize + gx;
                                        let screen_y =
                                            py + (13usize.saturating_sub(metrics.height)) + gy;

                                        if screen_x < width as usize && screen_y < height as usize {
                                            let c = cvg as u32;
                                            let color = (c << 16) | (c << 8) | c;
                                            buffer[screen_y * width as usize + screen_x] = 0;
                                            buffer[screen_y * width as usize + screen_x] = color;
                                        }
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
                buffer.present().unwrap();
            }
            WindowEvent::KeyboardInput { event, .. } => match event.state {
                Pressed => match event.logical_key {
                    Key::Named(k) => match k {
                        NamedKey::Enter => {
                            write!(self.writer.as_mut().unwrap(), "\n").unwrap();
                        }
                        _ => {}
                    },
                    Key::Character(ch) => {
                        println!("{}", ch);
                        write!(self.writer.as_mut().unwrap(), "{ch}").unwrap();
                    }
                    Key::Unidentified(_) => {}
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut def = App::default();
    def.init();
    def.run("ls -l");
    def.run("pwd");
    event_loop.run_app(def).unwrap();
}
