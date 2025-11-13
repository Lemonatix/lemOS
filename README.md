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
   - first step boot process (bootloader), probably with assembly
   - as well as compiler (in C after bootloader)
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

## How to compile and run files

Before compiling it is important to be in the right directory, for windows explicitly (with an example)
```bash
cd C:\Users\user\myDirectory\lemOS\userland\    
```
to be sure that the file will be compile in the same folder as the code itself.

First line compiles the file, just replace "file" with the actual file, second line runs the file
```c
g++ file.c -o file.exe
./file.exe
```

## Disclaimer & Coming soon
This is a hobbyist project and is not yet stable. Not recommended for production use (unless you're very brave), since
lemOS is in very early development. Build and installation instructions will be published here when the first release is ready.
