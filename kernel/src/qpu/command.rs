//! Der lemsh-Befehl `qpu`: das virtuelle QPU von der Shell aus bedienen.
//!
//! Die Shell verhält sich dabei wie ein Programm: Sie baut einen Schaltkreis,
//! reicht ihn als Job ein und holt sich das Ergebnis ab.

use alloc::vec::Vec;

use spin::Mutex;

use super::circuit::{Circuit, Op};
use super::compiler::{Native, compile};
use super::noise::NoiseModel;
use super::{NOISE, Status, TOPOLOGY};
use crate::{interrupts, kprint, kprintln};

const DEFAULT_SHOTS: u32 = 100;

/// So viele verschiedene Ergebnisse zeigt das Histogramm höchstens.
const MAX_HISTOGRAM_LINES: usize = 16;

/// Der Schaltkreis, den die Shell gerade zusammenbaut.
static CIRCUIT: Mutex<Circuit> = Mutex::new(Circuit::new());

const USAGE: &str = "\
Virtual QPU (4x4 qubit grid, native gates RZ SX X CZ, with noise):
  qpu info            chip layout and noise
  qpu circuit GATES   new circuit, e.g. qpu circuit h 0; cx 0 5
  qpu add GATES       add gates to the circuit
  qpu demo bell|ghz   load a demo circuit
  qpu show            show the circuit
  qpu compile         show what the chip really runs
  qpu compare         compare compiled and ideal result
  qpu noise off|on|P1 P2 READ DAMP   noise in percent
  qpu submit [SHOTS]  queue the circuit as a job
  qpu jobs            list jobs
  qpu exec            run all queued jobs
  qpu result ID       show the result of a job
  qpu run [SHOTS]     submit, exec and result in one go
Gates: h x y z s t K | rx ry rz K DEG | cx cz swap A B";

/// Wird von lemsh mit allem nach `qpu` aufgerufen.
pub fn command(args: &str) {
    let args = args.trim();
    let (op, rest) = args.split_once(' ').unwrap_or((args, ""));
    if let Err(message) = run(op, rest.trim()) {
        kprintln!("qpu: {message}");
    }
}

fn run(op: &str, rest: &str) -> Result<(), &'static str> {
    match op {
        "" | "help" => kprintln!("{USAGE}"),
        "info" => info(),
        "circuit" => {
            let circuit = Circuit::parse(rest).map_err(|e| e.describe())?;
            kprintln!(
                "Circuit with {} gates on {} qubits.",
                circuit.ops.len(),
                circuit.qubits()
            );
            *CIRCUIT.lock() = circuit;
        }
        "add" => {
            let mut circuit = CIRCUIT.lock();
            circuit.append(rest).map_err(|e| e.describe())?;
            kprintln!(
                "Circuit with {} gates on {} qubits.",
                circuit.ops.len(),
                circuit.qubits()
            );
        }
        "demo" => {
            let text = match rest {
                "bell" => "h 0; cx 0 1",
                // Qubit 0 ist nur mit 1 und 4 verbunden, der Rest braucht SWAPs.
                "ghz" => "h 0; cx 0 1; cx 0 2; cx 0 3; cx 0 4; cx 0 5",
                _ => return Err("demos: bell, ghz"),
            };
            *CIRCUIT.lock() = Circuit::parse(text).map_err(|e| e.describe())?;
            kprintln!("Loaded: {text}");
        }
        "show" => show(&current()?),
        "compile" => show_compiled(&current()?)?,
        "compare" => {
            let fidelity = super::fidelity(&current()?)?;
            kprintln!("Fidelity of compiled vs. ideal circuit: {fidelity:.6}");
        }
        "noise" => noise(rest)?,
        "submit" => {
            let id = super::submit(current()?, shots(rest)?);
            kprintln!("Job {id} queued.");
        }
        "jobs" => jobs(),
        "exec" => {
            kprintln!("Running queued jobs ...");
            let start = interrupts::ticks();
            let n = super::process();
            kprintln!(
                "{n} job(s) done in {}.",
                Seconds(interrupts::ticks() - start)
            );
        }
        "result" => {
            let id: usize = rest.parse().map_err(|_| "expected a job number")?;
            show_result(id)?;
        }
        "run" => {
            let circuit = current()?;
            if circuit.qubits() > 10 && !NOISE.lock().gates_are_ideal() {
                kprintln!(
                    "Simulating noise on many qubits takes a while (try 'qpu noise off') ..."
                );
            }
            let id = super::submit(circuit, shots(rest)?);
            let start = interrupts::ticks();
            super::process();
            show_result(id)?;
            kprintln!("Took {}.", Seconds(interrupts::ticks() - start));
        }
        _ => return Err("unknown operation. Type 'qpu' for help."),
    }
    Ok(())
}

fn current() -> Result<Circuit, &'static str> {
    let circuit = CIRCUIT.lock();
    if circuit.ops.is_empty() {
        return Err("no circuit yet. Try 'qpu demo ghz'.");
    }
    Ok(circuit.clone())
}

fn shots(text: &str) -> Result<u32, &'static str> {
    if text.is_empty() {
        return Ok(DEFAULT_SHOTS);
    }
    let shots: u32 = text.parse().map_err(|_| "expected a number of shots")?;
    if shots == 0 || shots > super::MAX_SHOTS {
        return Err("shots must be between 1 and 10000");
    }
    Ok(shots)
}

fn info() {
    kprintln!(
        "Chip: {}x{} grid, two-qubit gates only between neighbors:",
        TOPOLOGY.rows(),
        TOPOLOGY.columns()
    );
    for row in 0..TOPOLOGY.rows() {
        kprint!("  ");
        for column in 0..TOPOLOGY.columns() {
            let q = row * TOPOLOGY.columns() + column;
            if column > 0 {
                kprint!(" - ");
            }
            kprint!("{q:>2}");
        }
        kprintln!();
        if row + 1 < TOPOLOGY.rows() {
            kprint!("  ");
            for column in 0..TOPOLOGY.columns() {
                kprint!("{}", if column > 0 { "    |" } else { " |" });
            }
            kprintln!();
        }
    }
    kprintln!("Native gates: RZ (error-free), SX, X, CZ");
    print_noise(&NOISE.lock());
}

fn print_noise(n: &NoiseModel) {
    kprintln!(
        "Noise: 1q {:.2}%, 2q {:.2}%, readout {:.2}%, damping {:.2}%",
        n.single * 100.0,
        n.two * 100.0,
        n.readout * 100.0,
        n.damping * 100.0
    );
}

fn noise(rest: &str) -> Result<(), &'static str> {
    let model = match rest {
        "" => {
            print_noise(&NOISE.lock());
            return Ok(());
        }
        "off" => NoiseModel::IDEAL,
        "on" | "default" => NoiseModel::DEFAULT,
        _ => {
            let values: Vec<f64> = rest
                .split_whitespace()
                .map(|w| w.parse::<f64>().map(|p| p / 100.0))
                .collect::<Result<_, _>>()
                .map_err(|_| "expected four percentages")?;
            let [single, two, readout, damping] = values[..] else {
                return Err("expected four percentages: 1q 2q readout damping");
            };
            if values.iter().any(|&p| !(0.0..=1.0).contains(&p)) {
                return Err("percentages must be between 0 and 100");
            }
            NoiseModel {
                single,
                two,
                readout,
                damping,
            }
        }
    };
    *NOISE.lock() = model;
    print_noise(&model);
    Ok(())
}

fn show(circuit: &Circuit) {
    kprintln!("{} qubits, {} gates:", circuit.qubits(), circuit.ops.len());
    for op in &circuit.ops {
        match *op {
            Op::H(q) => kprintln!("  h {q}"),
            Op::X(q) => kprintln!("  x {q}"),
            Op::Y(q) => kprintln!("  y {q}"),
            Op::Z(q) => kprintln!("  z {q}"),
            Op::S(q) => kprintln!("  s {q}"),
            Op::T(q) => kprintln!("  t {q}"),
            Op::Rx(q, t) => kprintln!("  rx {q} {:.1}", t.to_degrees()),
            Op::Ry(q, t) => kprintln!("  ry {q} {:.1}", t.to_degrees()),
            Op::Rz(q, t) => kprintln!("  rz {q} {:.1}", t.to_degrees()),
            Op::Cx(a, b) => kprintln!("  cx {a} {b}"),
            Op::Cz(a, b) => kprintln!("  cz {a} {b}"),
            Op::Swap(a, b) => kprintln!("  swap {a} {b}"),
        }
    }
}

fn show_compiled(circuit: &Circuit) -> Result<(), &'static str> {
    let compiled =
        compile(circuit, &TOPOLOGY).map_err(|_| "circuit needs more qubits than the chip has")?;
    kprintln!(
        "{} native gates ({} CZ), {} SWAPs inserted, depth {}",
        compiled.ops.len(),
        compiled.two_qubit_gates(),
        compiled.swaps,
        compiled.depth(TOPOLOGY.qubits())
    );
    // Lange Programme kompakt in Zeilen zu je sechs Gattern.
    for chunk in compiled.ops.chunks(6) {
        kprint!(" ");
        for op in chunk {
            match *op {
                Native::Rz(q, t) => kprint!(" rz({:.0}) q{q};", t.to_degrees()),
                Native::Sx(q) => kprint!(" sx q{q};"),
                Native::X(q) => kprint!(" x q{q};"),
                Native::Cz(a, b) => kprint!(" cz q{a},q{b};"),
            }
        }
        kprintln!();
    }
    kprint!("Final layout (logical -> physical):");
    for (logical, physical) in compiled.final_layout.iter().enumerate() {
        kprint!(" {logical}->{physical}");
    }
    kprintln!();
    Ok(())
}

fn jobs() {
    let mut any = false;
    super::for_each_job(|job| {
        any = true;
        let status = match &job.status {
            Status::Queued => "queued",
            Status::Done(_) => "done",
            Status::Failed(_) => "failed",
        };
        kprintln!(
            "  job {:>3}: {:>2} qubits, {:>3} gates, {:>5} shots, {status}",
            job.id,
            job.circuit.qubits(),
            job.circuit.ops.len(),
            job.shots
        );
    });
    if !any {
        kprintln!("No jobs yet.");
    }
}

fn show_result(id: usize) -> Result<(), &'static str> {
    let result = match super::result(id).ok_or("no such job")? {
        Status::Queued => return Err("job is still queued. Run 'qpu exec'."),
        Status::Failed(message) => return Err(message),
        Status::Done(result) => result,
    };
    kprintln!(
        "Job {id}: {} shots, {} native gates ({} CZ, {} SWAPs), depth {}",
        result.shots,
        result.native_gates,
        result.two_qubit_gates,
        result.swaps,
        result.depth
    );
    let best = result.counts.first().map_or(1, |c| c.1).max(1);
    for &(bits, count) in result.counts.iter().take(MAX_HISTOGRAM_LINES) {
        kprint!("  |");
        for k in (0..result.logical_qubits).rev() {
            kprint!("{}", (bits >> k) & 1);
        }
        kprint!(">  {count:>5}  ");
        for _ in 0..(count as usize * 30).div_ceil(best as usize) {
            kprint!("#");
        }
        kprintln!();
    }
    if result.counts.len() > MAX_HISTOGRAM_LINES {
        kprintln!(
            "  ... and {} more outcomes",
            result.counts.len() - MAX_HISTOGRAM_LINES
        );
    }
    Ok(())
}

/// Gibt Timer-Ticks als Sekunden aus.
struct Seconds(u64);

impl core::fmt::Display for Seconds {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let hz = u64::from(interrupts::TIMER_HZ);
        write!(f, "{}.{:02} s", self.0 / hz, (self.0 % hz) * 100 / hz)
    }
}
