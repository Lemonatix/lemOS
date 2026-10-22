//! Die Zitronenscheibe, die beim Start ganz oben erscheint.

use crate::console::FOREGROUND;
use crate::{kprint, kprintln, set_color};

/// Auf dem Bildschirm sind die Zeichen quadratisch, deshalb ist die Scheibe
/// gleich breit wie hoch (anders als im Terminal, wo sie gestaucht wirkt).
const LEMON: &[&str] = &[
    "    #####",
    "  ##ooooo##",
    " ##o..|..o##",
    " #o\\..|../o#",
    "#o..\\.|./..o#",
    "#o...\\|/...o#",
    "#o----*----o#",
    "#o.../|\\...o#",
    "#o../.|.\\..o#",
    " #o/..|..\\o#",
    " ##o..|..o##",
    "  ##ooooo##",
    "    #####",
];

/// Farbe je Zeichen: gelbe Schale, weiße Haut, Fruchtfleisch und
/// Trennwände der Segmente.
fn color(c: char) -> u32 {
    match c {
        '#' => 0x00FF_D21F,
        'o' => 0x00FF_F6C8,
        '.' => 0x00C8_A82A,
        _ => 0x00FF_FFFF,
    }
}

pub fn show() {
    for line in LEMON {
        kprint!(" ");
        for c in line.chars() {
            set_color(color(c));
            kprint!("{c}");
        }
        kprintln!();
    }
    set_color(FOREGROUND);
    kprintln!();
}
