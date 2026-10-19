//! Textausgabe auf dem Framebuffer, den Limine für uns eingerichtet hat.
//!
//! Jedes Zeichen wird aus der 8x8-Pixel-Schrift `font8x8` gezeichnet und
//! zur besseren Lesbarkeit doppelt so groß dargestellt.

use core::fmt;
use font8x8::UnicodeFonts;
use limine::framebuffer::Framebuffer;

const SCALE: usize = 2;
const GLYPH: usize = 8 * SCALE;

/// Zitronengelb auf dunklem Hintergrund.
const FOREGROUND: u32 = 0x00FF_E14D;
const BACKGROUND: u32 = 0x0010_1418;

pub struct Console {
    buffer: *mut u32,
    width: usize,
    height: usize,
    /// Pixel pro Zeile (kann größer als `width` sein).
    stride: usize,
    column: usize,
    row: usize,
}

// Der Framebuffer gehört allein dem Kernel; der Zugriff läuft über einen Mutex.
unsafe impl Send for Console {}

impl Console {
    /// Gibt `None` zurück, wenn der Framebuffer kein 32-Bit-Format hat.
    pub fn new(fb: &Framebuffer) -> Option<Self> {
        if fb.bpp != 32 {
            return None;
        }
        let mut console = Self {
            buffer: fb.address() as *mut u32,
            width: fb.width as usize,
            height: fb.height as usize,
            stride: fb.pitch as usize / 4,
            column: 0,
            row: 0,
        };
        console.clear();
        Some(console)
    }

    fn columns(&self) -> usize {
        self.width / GLYPH
    }

    fn rows(&self) -> usize {
        self.height / GLYPH
    }

    fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        unsafe { self.buffer.add(y * self.stride + x).write_volatile(color) };
    }

    pub fn clear(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, BACKGROUND);
            }
        }
        self.column = 0;
        self.row = 0;
    }

    fn draw_glyph(&mut self, c: char) {
        let glyph = font8x8::BASIC_FONTS
            .get(c)
            .or_else(|| font8x8::BASIC_FONTS.get('?'))
            .unwrap();
        let (x0, y0) = (self.column * GLYPH, self.row * GLYPH);
        for (gy, bits) in glyph.iter().enumerate() {
            for gx in 0..8 {
                let color = if bits & (1 << gx) != 0 {
                    FOREGROUND
                } else {
                    BACKGROUND
                };
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        self.put_pixel(x0 + gx * SCALE + dx, y0 + gy * SCALE + dy, color);
                    }
                }
            }
        }
    }

    fn new_line(&mut self) {
        self.column = 0;
        self.row += 1;
        if self.row >= self.rows() {
            // Noch kein Scrollen: wir fangen oben wieder an.
            self.clear();
        }
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.new_line(),
            '\r' => self.column = 0,
            c => {
                if self.column >= self.columns() {
                    self.new_line();
                }
                self.draw_glyph(c);
                self.column += 1;
            }
        }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}
