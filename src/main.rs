use std::collections::HashMap;
use std::hash::Hash;
use std::io::{Read, Write};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc;
use std::{fs, thread};

use fontdue::{Font, FontSettings, Metrics};
use softbuffer::Surface;
use winit::application::ApplicationHandler;
use winit::event::{self, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};
use portable_pty::{CommandBuilder, PtyPair, PtySize, PtySystem, native_pty_system};
use anyhow::Error;

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
}

impl App {
    fn init(&mut self) {
        self.fontmap = HashMap::new();
        self.grid = Vec::new();

        let font = include_bytes!("/Users/manosriram/dev/tilde/src/assets/monospace-argon.otf") as &[u8];
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        for x in font.chars() {
            let f = font.rasterize(*x.0, 18.0);
            self.fontmap.insert(*x.0, f);
        }
        println!("{:?}", self.fontmap.get(&'a'));
        

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        // let cmd = CommandBuilder::new("bash");
        // pair.slave.spawn_command(cmd).unwrap();

        self.writer = Some(pair.master.take_writer().unwrap());
        self.pty = Some(pair);
        
        // self.window.as_ref().unwrap().
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

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        } else if event == WindowEvent::RedrawRequested {

            let mut reader = self.pty.as_ref().unwrap().master.try_clone_reader().unwrap();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(1) => break,
                    Ok(n) => print!("zz = {}", String::from_utf8_lossy(&buf[..n])),
                    Err(e) => {print!("{}", e)}
                }
            }

            println!("requesting after run");
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

            for y in 0..height {
                for x in 0..width {
                    buffer[(y * width + x) as usize] = 97;
                }
            }
            buffer.present().unwrap();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut def = App::default();
    def.init();
    def.run("ls -l");
    event_loop.run_app(def).unwrap();
}
