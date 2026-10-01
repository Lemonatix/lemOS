# 🍋 lemOS

**lemOS** is a <code>lightweight</code>, <code>efficient</code> and <code>modular</code> operating system — inspired by science, lemons, and minimalism.  
Created originally by [Lemonatix](https://github.com/Lemonatix) and [Abgorion](https://github.com/Abgor-ion), lemOS blends playful aesthetics with a powerful, customizable UNIX-like core.

> _"From Lemaître to Lemon — the Big Bang of your desktop experience."_

---

## Features

- Science-inspired design principles (clean, fundamental, extensible)
- Lightweight and fast boot process
- Modular kernel architecture *(planned)*
- Custom lemon-themed UI and branding

---

## Project Structure

```plaintext
lemOS/
├── kernel/           # Core kernel code (WIP or custom)
├── userland/         # Shell, drivers, user programs
├── docs/             # Documentation (licensed under CC BY 4.0)
├── assets/           # Icons, wallpapers, and logo
├── scripts/          # Build tools and install scripts
└── LICENSE           # Software license (GNU GPL 3.0)
```

## Why “lemOS”?
The name is inspired by my username <code>Lem</code>onatix, as well as 

<code>Lem</code>aître, henceforth honoring the father of the Big Bang Theory with the OS.

## Roadmap
 - Minimal bootable system
   - [x] boot via Limine into a Rust kernel and print text
   - [ ] interrupts, timer and keyboard
   - [ ] memory management (paging, heap)
 - Lemon shell (lemsh)
 - Filesystem and basic I/O
 - GUI prototype
 - Package manager (lemonpkg?)
 - Optional science-themed wallpapers & Easter eggs

## License
- Code: Licensed under the MIT License — free for personal and commercial use.
- Documentation & assets: Licensed under CC BY 4.0

## Contributing
Pull requests are welcome! If you'd like to contribute code, ideas, or lemon-themed assets, please open an issue or fork the project.

## Building and running

lemOS is written in Rust and boots with the [Limine](https://github.com/limine-bootloader/limine) bootloader. You can try it in the QEMU emulator without touching your real computer.

### What you need

- [Rust](https://rustup.rs) via `rustup`. The right nightly toolchain installs itself the first time you build, because of `kernel/rust-toolchain.toml`.
- QEMU, xorriso, git, make and a C compiler:
  - Ubuntu/Debian: `sudo apt install qemu-system-x86 xorriso git make gcc`
  - macOS (Homebrew): `brew install qemu xorriso`
  - Windows: use [WSL](https://learn.microsoft.com/windows/wsl/install) with Ubuntu and follow the Ubuntu line.

### Commands

Run these from the `lemOS/` folder:

```bash
make          # build the kernel and the bootable image build/lemos.iso
make run      # start lemOS in QEMU (window plus serial output in your terminal)
make run-nox  # start lemOS without a window, serial output only
make test     # boot without a window and check that lemOS reports "boot ok"
make clean    # remove all build output
```

The first build downloads Limine into `build/limine`.

### Project layout

```plaintext
kernel/
├── src/main.rs      # entry point (kmain), Limine requests, kprintln!, panic handler
├── src/console.rs   # text output on the framebuffer
├── src/serial.rs    # serial port COM1 (output shows up in the terminal)
├── linker.ld        # places the kernel in the higher half
└── rust-toolchain.toml
boot/limine.conf     # bootloader menu
Makefile             # build, ISO, QEMU
```

## Disclaimer & Coming soon
This is a hobbyist project and is not yet stable. Not recommended for production use (unless you're very brave), since
lemOS is in very early development. Build and installation instructions will be published here when the first release is ready.
