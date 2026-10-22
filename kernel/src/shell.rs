//! lemsh, die Zitronen-Shell.
//!
//! Liest eine Zeile von der Tastatur, zerlegt sie in Befehl und Argumente und
//! führt den Befehl aus. Mehr ist eine Shell im Kern nicht.

use alloc::string::String;

use crate::{clear_screen, interrupts, kprint, kprintln};

const PROMPT: &str = "lemsh> ";

/// Längere Eingaben werden abgeschnitten.
const MAX_LINE: usize = 256;

/// Ein Befehl der Shell: Name, kurze Beschreibung und die Funktion, die ihn
/// mit den Argumenten (dem Rest der Zeile) ausführt.
struct Command {
    name: &'static str,
    help: &'static str,
    run: fn(&str),
}

const COMMANDS: &[Command] = &[
    Command {
        name: "help",
        help: "zeigt diese Hilfe",
        run: help,
    },
    Command {
        name: "clear",
        help: "leert den Bildschirm",
        run: clear,
    },
    Command {
        name: "echo",
        help: "gibt den Text dahinter aus",
        run: echo,
    },
    Command {
        name: "reboot",
        help: "startet den Rechner neu",
        run: reboot,
    },
];

/// Startet die Shell. Sie läuft, bis der Rechner ausgeht.
pub fn run() -> ! {
    kprintln!("\nlemsh bereit. Tippe 'help' fuer alle Befehle.");
    let mut line = String::new();
    loop {
        kprint!("{PROMPT}");
        read_line(&mut line);
        execute(line.trim());
    }
}

/// Liest Zeichen in `line`, bis Enter gedrückt wird.
fn read_line(line: &mut String) {
    line.clear();
    loop {
        match interrupts::read_key() {
            '\n' => {
                kprintln!();
                return;
            }
            '\u{8}' => {
                if line.pop().is_some() {
                    kprint!("\u{8}");
                }
            }
            c if c.is_control() => {}
            c => {
                if line.len() + c.len_utf8() <= MAX_LINE {
                    line.push(c);
                    kprint!("{c}");
                }
            }
        }
    }
}

fn execute(line: &str) {
    if line.is_empty() {
        return;
    }
    let (name, args) = line.split_once(' ').unwrap_or((line, ""));
    match COMMANDS.iter().find(|command| command.name == name) {
        Some(command) => (command.run)(args.trim_start()),
        None => kprintln!("Unbekannter Befehl: {name}. Tippe 'help' fuer alle Befehle."),
    }
}

// --- Befehle --------------------------------------------------------------------

fn help(_args: &str) {
    kprintln!("Befehle:");
    for command in COMMANDS {
        kprintln!("  {:<8}{}", command.name, command.help);
    }
}

fn clear(_args: &str) {
    clear_screen();
}

fn echo(args: &str) {
    kprintln!("{args}");
}

fn reboot(_args: &str) {
    kprintln!("Starte neu ...");
    unsafe {
        // Der Tastatur-Controller kann die CPU zurücksetzen: erst warten, bis
        // er bereit ist, dann den Befehl 0xFE ("Reset-Leitung pulsen") senden.
        let mut status = x86_64::instructions::port::Port::<u8>::new(0x64);
        while status.read() & 0b10 != 0 {}
        status.write(0xFE);

        // Falls das nicht klappt: eine leere IDT laden und einen Interrupt
        // auslösen. Die CPU findet keinen Handler, gibt auf (Triple Fault)
        // und startet neu.
        x86_64::instructions::tables::lidt(&x86_64::structures::DescriptorTablePointer {
            limit: 0,
            base: x86_64::VirtAddr::zero(),
        });
        x86_64::instructions::interrupts::int3();
    }
    unreachable!();
}
