#![feature(mpmc_channel)]

mod chip8;

extern crate sdl3;

use std::sync::mpmc::{Receiver, channel};
use std::time::Duration;
use std::{env, fs, process, thread};

use chip8::System;

use sdl3::event::Event;
use sdl3::keyboard::Keycode;

use log::{error, info};
use sdl3::pixels::PixelFormat;
use sdl3::sys::pixels::SDL_PixelFormat;
use sdl3::sys::render::SDL_SetTextureScaleMode;
use sdl3::sys::surface::SDL_SCALEMODE_NEAREST;

fn main() {
    colog::init();

    let (tx_draw, rx_draw) = channel::<[bool; 64 * 32]>();
    let (tx_quit, rx_quit) = channel::<bool>();

    // read args
    let args: Vec<String> = env::args().collect();

    // invalid usage
    if args.len() != 2 {
        error!("Please specify the path to a rom");
        process::exit(1);
    }

    let rom_path = args[1].clone();

    info!("Reading rom: {rom_path}");

    let mut system = System::new();

    // load rom
    if let Ok(rom) = fs::read(rom_path) {
        system.load_rom(&rom);
    } else {
        panic!("Failed to read rom file");
    }

    let ui_rx_quit = rx_quit.clone();
    let ui_rx_draw = rx_draw.clone();
    let ui_handle = thread::spawn(move || {
        init_ui(&ui_rx_quit, &ui_rx_draw).unwrap_or_else(|e| {
            panic!("Failed to initialize window: {e:?}");
        });
    });

    let system_rx_quit = rx_quit.clone();
    let system_tx_draw = tx_draw.clone();
    let system_handle = thread::spawn(move || {
        system.run(&system_tx_draw, &system_rx_quit);
    });

    // wait for ui thread to finish
    match ui_handle.join() {
        Ok(_) => {
            info!("Goodbye!");
        }
        Err(e) => {
            panic!("Failed to join UI thread: {e:?}");
        }
    }

    tx_quit.send(true).unwrap();
}

fn init_ui(rx_quit: &Receiver<bool>, rx_draw: &Receiver<[bool; 64 * 32]>) -> Result<(), String> {
    let sdl_context = sdl3::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
        .window("Nic's CHIP-8 Emulator", 640, 320)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas();

    let texture_creator = canvas.texture_creator();

    // create texture
    let mut texture = texture_creator
        .create_texture_streaming(
            unsafe { PixelFormat::from_ll(SDL_PixelFormat::RGB24) },
            64,
            32,
        )
        .map_err(|e| e.to_string())?;
    unsafe {
        // fix scaling
        SDL_SetTextureScaleMode(texture.raw(), SDL_SCALEMODE_NEAREST);
    }

    canvas.present();

    thread::sleep(Duration::from_secs(1));

    'mainloop: loop {
        // handle events coming in through the message channel
        if let Ok(gfx) = rx_draw.try_recv() {
            // update texture
            texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                bool_to_rgb24_inplace(&gfx, buffer);
            })?;
            canvas.copy(&texture, None, None)?;
            canvas.present();
        }

        // handle quit events
        if let Ok(_) = rx_quit.recv_timeout(Duration::from_millis(50)) {
            break 'mainloop;
        }

        // handle sdl window events
        for event in sdl_context.event_pump()?.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Option::Some(Keycode::Escape),
                    ..
                } => {
                    break 'mainloop;
                }
                _ => {}
            }
        }
    }

    drop(texture);
    drop(texture_creator);
    drop(canvas);
    drop(video_subsystem);
    drop(sdl_context);

    Ok(())
}

fn bool_to_rgb24_inplace(bools: &[bool], out: &mut [u8]) {
    // The output buffer must have space for 3 bytes per bool (RGB).
    assert_eq!(
        out.len(),
        bools.len() * 3,
        "Output slice is the wrong length!"
    );

    for (i, &b) in bools.iter().enumerate() {
        let base = i * 3;
        if b {
            // Write white: (255, 255, 255)
            out[base] = 255;
            out[base + 1] = 255;
            out[base + 2] = 255;
        } else {
            // Write black: (0, 0, 0)
            out[base] = 0;
            out[base + 1] = 0;
            out[base + 2] = 0;
        }
    }
}
