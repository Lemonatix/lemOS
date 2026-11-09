//! Ein kleiner Quantencomputer-Simulator (Zustandsvektor).
//!
//! Ein System aus n Qubits wird durch 2^n komplexe Amplituden beschrieben, eine
//! für jeden Basiszustand |00…0>, |00…1>, …, |11…1>. Das Betragsquadrat einer
//! Amplitude ist die Wahrscheinlichkeit, bei einer Messung genau diesen Zustand
//! zu sehen. Ein Gatter auf einem Qubit ist eine 2×2-Matrix, die wir auf alle
//! Paare von Amplituden anwenden, die sich nur in diesem einen Bit
//! unterscheiden.
//!
//! Qubit 0 ist das rechte Bit, wie bei Qiskit: In |10> ist Qubit 1 gesetzt.
//!
//! Speicher: 16 Byte pro Amplitude, bei 16 Qubits also 65 536 Amplituden = 1 MiB.

use alloc::vec::Vec;
use core::ops::{Add, Mul};

use spin::Mutex;

use crate::kprintln;

/// Mehr passt mit unserem Heap nicht sinnvoll (17 Qubits wären schon 2 MiB).
pub const MAX_QUBITS: usize = 16;

/// Kleinere Wahrscheinlichkeiten zeigt `state` nicht an.
const EPSILON: f64 = 1e-10;

/// So viele Zeilen gibt `state` höchstens aus.
const MAX_STATE_LINES: usize = 32;

const FRAC_1_SQRT_2: f64 = core::f64::consts::FRAC_1_SQRT_2;

// --- Komplexe Zahlen ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Complex = Complex::new(0.0, 0.0);
    pub const ONE: Complex = Complex::new(1.0, 0.0);
    pub const I: Complex = Complex::new(0.0, 1.0);

    pub const fn new(re: f64, im: f64) -> Self {
        Complex { re, im }
    }

    /// e^(iφ) = cos φ + i·sin φ
    pub fn from_phase(phi: f64) -> Self {
        Complex::new(libm::cos(phi), libm::sin(phi))
    }

    /// |z|², also die Wahrscheinlichkeit, wenn z eine Amplitude ist.
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    pub fn scale(self, factor: f64) -> Self {
        Complex::new(self.re * factor, self.im * factor)
    }
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, other: Complex) -> Complex {
        Complex::new(self.re + other.re, self.im + other.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, other: Complex) -> Complex {
        Complex::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }
}

/// Eine 2×2-Matrix für ein Gatter auf einem Qubit: [[a, b], [c, d]].
pub type Gate = [[Complex; 2]; 2];

/// Die Standardgatter. Winkel sind im Bogenmaß.
pub mod gates {
    use super::{Complex, FRAC_1_SQRT_2, Gate};

    const O: Complex = Complex::ZERO;
    const L: Complex = Complex::ONE;

    /// Hadamard: macht aus |0> die Überlagerung (|0> + |1>)/√2.
    pub const H: Gate = [
        [
            Complex::new(FRAC_1_SQRT_2, 0.0),
            Complex::new(FRAC_1_SQRT_2, 0.0),
        ],
        [
            Complex::new(FRAC_1_SQRT_2, 0.0),
            Complex::new(-FRAC_1_SQRT_2, 0.0),
        ],
    ];
    /// Pauli-X, das Quanten-NOT.
    pub const X: Gate = [[O, L], [L, O]];
    pub const Y: Gate = [[O, Complex::new(0.0, -1.0)], [Complex::I, O]];
    pub const Z: Gate = [[L, O], [O, Complex::new(-1.0, 0.0)]];
    /// Phase um 90°.
    pub const S: Gate = [[L, O], [O, Complex::I]];
    /// Phase um 45°.
    pub const T: Gate = [[L, O], [O, Complex::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2)]];

    pub fn rx(theta: f64) -> Gate {
        let (c, s) = (libm::cos(theta / 2.0), libm::sin(theta / 2.0));
        [
            [Complex::new(c, 0.0), Complex::new(0.0, -s)],
            [Complex::new(0.0, -s), Complex::new(c, 0.0)],
        ]
    }

    pub fn ry(theta: f64) -> Gate {
        let (c, s) = (libm::cos(theta / 2.0), libm::sin(theta / 2.0));
        [
            [Complex::new(c, 0.0), Complex::new(-s, 0.0)],
            [Complex::new(s, 0.0), Complex::new(c, 0.0)],
        ]
    }

    pub fn rz(theta: f64) -> Gate {
        [
            [Complex::from_phase(-theta / 2.0), O],
            [O, Complex::from_phase(theta / 2.0)],
        ]
    }
}

// --- Zufall -----------------------------------------------------------------------

/// Ein einfacher Zufallsgenerator (xorshift64*). Für Messungen reicht das; als
/// Startwert nehmen wir den Zeitstempelzähler der CPU.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        if self.0 == 0 {
            self.0 = unsafe { core::arch::x86_64::_rdtsc() } | 1;
        }
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Eine Zufallszahl in [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

static RNG: Mutex<Rng> = Mutex::new(Rng(0));

fn random() -> f64 {
    RNG.lock().next_f64()
}

// --- Der Zustand ------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum Error {
    TooManyQubits,
    OutOfMemory,
    NoSuchQubit(usize),
    SameQubit,
}

pub struct State {
    qubits: usize,
    amplitudes: Vec<Complex>,
}

impl State {
    /// n Qubits, alle im Zustand |0>.
    pub fn new(qubits: usize) -> Result<Self, Error> {
        if qubits == 0 || qubits > MAX_QUBITS {
            return Err(Error::TooManyQubits);
        }
        let size = 1usize << qubits;
        let mut amplitudes = Vec::new();
        amplitudes
            .try_reserve_exact(size)
            .map_err(|_| Error::OutOfMemory)?;
        amplitudes.resize(size, Complex::ZERO);
        amplitudes[0] = Complex::ONE;
        Ok(State { qubits, amplitudes })
    }

    pub fn qubits(&self) -> usize {
        self.qubits
    }

    pub fn amplitudes(&self) -> &[Complex] {
        &self.amplitudes
    }

    fn check(&self, qubit: usize) -> Result<usize, Error> {
        if qubit < self.qubits {
            Ok(1 << qubit)
        } else {
            Err(Error::NoSuchQubit(qubit))
        }
    }

    /// Wendet ein Gatter auf ein Qubit an.
    pub fn apply(&mut self, gate: &Gate, qubit: usize) -> Result<(), Error> {
        let bit = self.check(qubit)?;
        self.apply_masked(gate, bit, 0);
        Ok(())
    }

    /// Wendet ein Gatter auf `target` an, aber nur dort, wo `control` 1 ist.
    /// Mit X ist das CNOT, mit Z das CZ.
    pub fn apply_controlled(
        &mut self,
        gate: &Gate,
        control: usize,
        target: usize,
    ) -> Result<(), Error> {
        let control_bit = self.check(control)?;
        let target_bit = self.check(target)?;
        if control == target {
            return Err(Error::SameQubit);
        }
        self.apply_masked(gate, target_bit, control_bit);
        Ok(())
    }

    /// Kern des Simulators: geht alle Indizes durch, bei denen das Ziel-Bit 0
    /// und alle Bits aus `controls` 1 sind, und verrechnet die Amplitude mit
    /// ihrem Partner, bei dem das Ziel-Bit 1 ist.
    fn apply_masked(&mut self, gate: &Gate, target_bit: usize, controls: usize) {
        for i in 0..self.amplitudes.len() {
            if i & target_bit != 0 || i & controls != controls {
                continue;
            }
            let j = i | target_bit;
            let (a, b) = (self.amplitudes[i], self.amplitudes[j]);
            self.amplitudes[i] = gate[0][0] * a + gate[0][1] * b;
            self.amplitudes[j] = gate[1][0] * a + gate[1][1] * b;
        }
    }

    /// Vertauscht zwei Qubits.
    pub fn swap(&mut self, a: usize, b: usize) -> Result<(), Error> {
        let (bit_a, bit_b) = (self.check(a)?, self.check(b)?);
        if a == b {
            return Err(Error::SameQubit);
        }
        for i in 0..self.amplitudes.len() {
            // Nur die Fälle a=1, b=0 tauschen mit a=0, b=1.
            if i & bit_a != 0 && i & bit_b == 0 {
                self.amplitudes.swap(i, i ^ bit_a ^ bit_b);
            }
        }
        Ok(())
    }

    /// Wahrscheinlichkeit, dass `qubit` als 1 gemessen wird.
    pub fn probability_one(&self, qubit: usize) -> Result<f64, Error> {
        let bit = self.check(qubit)?;
        Ok(self
            .amplitudes
            .iter()
            .enumerate()
            .filter(|(i, _)| i & bit != 0)
            .map(|(_, a)| a.norm_sqr())
            .sum())
    }

    /// Misst ein Qubit. Der Zustand „kollabiert“: Alle Amplituden, die nicht
    /// zum Ergebnis passen, werden 0, der Rest wird wieder normiert.
    pub fn measure(&mut self, qubit: usize) -> Result<bool, Error> {
        self.measure_with(qubit, random())
    }

    fn measure_with(&mut self, qubit: usize, r: f64) -> Result<bool, Error> {
        let p_one = self.probability_one(qubit)?;
        let bit = 1 << qubit;
        let one = r < p_one;
        let p = if one { p_one } else { 1.0 - p_one };
        let factor = 1.0 / libm::sqrt(p);
        for (i, a) in self.amplitudes.iter_mut().enumerate() {
            *a = if (i & bit != 0) == one {
                a.scale(factor)
            } else {
                Complex::ZERO
            };
        }
        Ok(one)
    }

    /// Misst alle Qubits auf einmal und gibt den Basiszustand zurück.
    pub fn measure_all(&mut self) -> usize {
        self.measure_all_with(random())
    }

    fn measure_all_with(&mut self, r: f64) -> usize {
        let mut sum = 0.0;
        let mut result = self.amplitudes.len() - 1;
        for (i, a) in self.amplitudes.iter().enumerate() {
            sum += a.norm_sqr();
            if r < sum {
                result = i;
                break;
            }
        }
        self.amplitudes.fill(Complex::ZERO);
        self.amplitudes[result] = Complex::ONE;
        result
    }
}

// --- Shell-Befehl `q` --------------------------------------------------------------

/// Der Zustand, mit dem die Shell arbeitet.
static STATE: Mutex<Option<State>> = Mutex::new(None);

const USAGE: &str = "\
Quantum simulator (state vector, qubit 0 is the rightmost bit):
  q init N          start with N qubits (1-16), all |0>
  q h|x|y|z|s|t K   single-qubit gate on qubit K
  q rx|ry|rz K DEG  rotate qubit K by DEG degrees
  q cx C T          CNOT: flip T where C is 1
  q cz A B          controlled Z
  q swap A B        swap two qubits
  q measure [K]     measure qubit K, or all qubits
  q prob            probability of 1 for each qubit
  q state           show amplitudes
  q bell            demo: entangled pair of 2 qubits";

/// Wird von lemsh mit allem nach `q` aufgerufen.
pub fn command(args: &str) {
    let mut words = args.split_whitespace();
    let Some(op) = words.next() else {
        kprintln!("{USAGE}");
        return;
    };
    let rest: Vec<&str> = words.collect();
    if let Err(message) = run(op, &rest) {
        kprintln!("q: {message}");
    }
}

fn run(op: &str, args: &[&str]) -> Result<(), &'static str> {
    match op {
        "help" => {
            kprintln!("{USAGE}");
            Ok(())
        }
        "init" => {
            let n = number(args, 0)?;
            init(n)
        }
        "bell" => {
            init(2)?;
            with_state(|s| {
                s.apply(&gates::H, 0)?;
                s.apply_controlled(&gates::X, 0, 1)
            })?;
            kprintln!("Applied H on 0 and CNOT 0->1.");
            show_state()
        }
        "h" | "x" | "y" | "z" | "s" | "t" => {
            let gate = match op {
                "h" => gates::H,
                "x" => gates::X,
                "y" => gates::Y,
                "z" => gates::Z,
                "s" => gates::S,
                _ => gates::T,
            };
            let k = number(args, 0)?;
            with_state(|s| s.apply(&gate, k))
        }
        "rx" | "ry" | "rz" => {
            let k = number(args, 0)?;
            let degrees: f64 = args
                .get(1)
                .ok_or("missing angle in degrees")?
                .parse()
                .map_err(|_| "angle must be a number")?;
            let theta = degrees.to_radians();
            let gate = match op {
                "rx" => gates::rx(theta),
                "ry" => gates::ry(theta),
                _ => gates::rz(theta),
            };
            with_state(|s| s.apply(&gate, k))
        }
        "cx" | "cnot" | "cz" => {
            let (a, b) = (number(args, 0)?, number(args, 1)?);
            let gate = if op == "cz" { gates::Z } else { gates::X };
            with_state(|s| s.apply_controlled(&gate, a, b))
        }
        "swap" => {
            let (a, b) = (number(args, 0)?, number(args, 1)?);
            with_state(|s| s.swap(a, b))
        }
        "measure" | "m" => {
            if args.is_empty() {
                let mut guard = STATE.lock();
                let s = guard.as_mut().ok_or(NO_STATE)?;
                let result = s.measure_all();
                kprintln!("Result: |{}>", Basis(result, s.qubits()));
                Ok(())
            } else {
                let k = number(args, 0)?;
                let mut guard = STATE.lock();
                let s = guard.as_mut().ok_or(NO_STATE)?;
                let one = s.measure(k).map_err(describe)?;
                kprintln!("Qubit {k}: {}", one as u8);
                Ok(())
            }
        }
        "prob" => {
            let guard = STATE.lock();
            let s = guard.as_ref().ok_or(NO_STATE)?;
            for k in (0..s.qubits()).rev() {
                let p = s.probability_one(k).map_err(describe)?;
                kprintln!("  qubit {k:>2}: P(1) = {p:.4}");
            }
            Ok(())
        }
        "state" => show_state(),
        _ => Err("unknown operation. Type 'q' for help."),
    }
}

const NO_STATE: &str = "no qubits yet. Start with 'q init N'.";

fn init(n: usize) -> Result<(), &'static str> {
    // Erst den alten Zustand freigeben, sonst bräuchten wir kurz doppelt so
    // viel Speicher.
    if n == 0 || n > MAX_QUBITS {
        return Err(describe(Error::TooManyQubits));
    }
    let mut guard = STATE.lock();
    *guard = None;
    *guard = Some(State::new(n).map_err(describe)?);
    let bytes = (1usize << n) * core::mem::size_of::<Complex>();
    let (size, unit) = if bytes < 1024 {
        (bytes, "bytes")
    } else {
        (bytes / 1024, "KiB")
    };
    kprintln!(
        "{n} qubits ready ({} amplitudes, {size} {unit}).",
        1usize << n
    );
    Ok(())
}

fn with_state(f: impl FnOnce(&mut State) -> Result<(), Error>) -> Result<(), &'static str> {
    let mut guard = STATE.lock();
    let state = guard.as_mut().ok_or(NO_STATE)?;
    f(state).map_err(describe)
}

fn show_state() -> Result<(), &'static str> {
    let guard = STATE.lock();
    let s = guard.as_ref().ok_or(NO_STATE)?;
    let mut shown = 0;
    let mut hidden = 0;
    for (i, a) in s.amplitudes().iter().enumerate() {
        let p = a.norm_sqr();
        if p < EPSILON {
            continue;
        }
        if shown == MAX_STATE_LINES {
            hidden += 1;
            continue;
        }
        let sign = if a.im < 0.0 { '-' } else { '+' };
        kprintln!(
            "  |{}>  {:+.4} {sign} {:.4}i   P = {:.4}",
            Basis(i, s.qubits()),
            a.re,
            libm::fabs(a.im),
            p
        );
        shown += 1;
    }
    if hidden > 0 {
        kprintln!("  ... and {hidden} more");
    }
    Ok(())
}

fn number(args: &[&str], index: usize) -> Result<usize, &'static str> {
    args.get(index)
        .ok_or("missing qubit number")?
        .parse()
        .map_err(|_| "expected a number")
}

fn describe(error: Error) -> &'static str {
    match error {
        Error::TooManyQubits => "between 1 and 16 qubits are possible",
        Error::OutOfMemory => "not enough memory on the heap",
        Error::NoSuchQubit(_) => "no such qubit",
        Error::SameQubit => "the two qubits must be different",
    }
}

/// Gibt einen Basiszustand als Bitfolge aus, z. B. 0101.
struct Basis(usize, usize);

impl core::fmt::Display for Basis {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        for k in (0..self.1).rev() {
            f.write_str(if self.0 >> k & 1 == 1 { "1" } else { "0" })?;
        }
        Ok(())
    }
}
