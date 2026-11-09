//! Das virtuelle QPU: ein simulierter Quantenprozessor als Gerät.
//!
//! Echte Quantenchips sind Co-Prozessoren: Ein Programm schickt einen
//! Schaltkreis als Job, das System übersetzt ihn für den Chip, führt ihn
//! viele Male aus („Shots“) und liefert eine Statistik der Messergebnisse
//! zurück. Genau so funktioniert dieses Gerät, nur dass statt eines Chips im
//! Kryostaten der Simulator aus [`crate::quantum`] rechnet:
//!
//! ```text
//! Programm ──submit──> Job-Warteschlange ──process──> Compiler ──> Chip (simuliert, mit Rauschen)
//!    ^                                                                   │
//!    └──────────────────────────── result: Zählung je Bitfolge <─────────┘
//! ```
//!
//! Die Schnittstelle für Programme ist [`submit`], [`process`] und [`result`].
//! Später kann hinter derselben Schnittstelle ein Treiber für echte Hardware
//! stecken.

pub mod circuit;
pub mod command;
pub mod compiler;
pub mod noise;
pub mod topology;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use spin::Mutex;

use crate::quantum::{Complex, Gate, State, gates, random};
use circuit::Circuit;
use compiler::{Compiled, Native, compile};
use noise::NoiseModel;
use topology::{MAX_PHYSICAL, Topology};

/// So viele Shots darf ein Job höchstens haben.
pub const MAX_SHOTS: u32 = 10_000;

/// Der Chip: ein 4×4-Gitter.
pub static TOPOLOGY: Topology = Topology::grid(4, 4);

/// Das aktuelle Rauschmodell des Geräts.
pub static NOISE: Mutex<NoiseModel> = Mutex::new(NoiseModel::DEFAULT);

/// √X = ½·[[1+i, 1−i], [1−i, 1+i]]
const SX: Gate = [
    [Complex::new(0.5, 0.5), Complex::new(0.5, -0.5)],
    [Complex::new(0.5, -0.5), Complex::new(0.5, 0.5)],
];

// --- Jobs: die Schnittstelle für Programme ----------------------------------------

pub type JobId = usize;

#[derive(Clone, Debug)]
pub enum Status {
    Queued,
    Done(JobResult),
    Failed(&'static str),
}

#[derive(Clone, Debug)]
pub struct JobResult {
    /// Gemessene Bitfolgen (logische Qubits, Qubit 0 rechts) und wie oft sie
    /// vorkamen, häufigste zuerst.
    pub counts: Vec<(usize, u32)>,
    pub shots: u32,
    pub logical_qubits: usize,
    pub native_gates: usize,
    pub two_qubit_gates: usize,
    pub swaps: usize,
    pub depth: usize,
}

pub struct Job {
    pub id: JobId,
    pub circuit: Circuit,
    pub shots: u32,
    pub status: Status,
}

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());

/// Reiht einen Schaltkreis als Job ein und gibt seine Nummer zurück.
pub fn submit(circuit: Circuit, shots: u32) -> JobId {
    let mut jobs = JOBS.lock();
    let id = jobs.len() + 1;
    jobs.push(Job {
        id,
        circuit,
        shots: shots.clamp(1, MAX_SHOTS),
        status: Status::Queued,
    });
    id
}

/// Führt alle wartenden Jobs aus. Gibt zurück, wie viele es waren.
pub fn process() -> usize {
    let mut done = 0;
    loop {
        // Den Job herausholen und die Sperre freigeben, solange er rechnet.
        let next = {
            let jobs = JOBS.lock();
            jobs.iter()
                .find(|job| matches!(job.status, Status::Queued))
                .map(|job| (job.id, job.circuit.clone(), job.shots))
        };
        let Some((id, circuit, shots)) = next else {
            return done;
        };
        let noise = *NOISE.lock();
        let status = match execute(&circuit, shots, &noise) {
            Ok(result) => Status::Done(result),
            Err(message) => Status::Failed(message),
        };
        JOBS.lock()[id - 1].status = status;
        done += 1;
    }
}

/// Der Status eines Jobs, falls es ihn gibt.
pub fn result(id: JobId) -> Option<Status> {
    let jobs = JOBS.lock();
    id.checked_sub(1)
        .and_then(|i| jobs.get(i))
        .map(|job| job.status.clone())
}

/// Ruft `f` für jeden Job auf (für die Übersicht in der Shell).
pub fn for_each_job(mut f: impl FnMut(&Job)) {
    for job in JOBS.lock().iter() {
        f(job);
    }
}

// --- Ausführung auf dem simulierten Chip --------------------------------------------

/// Kompiliert und führt einen Schaltkreis aus.
pub fn execute(
    circuit: &Circuit,
    shots: u32,
    noise: &NoiseModel,
) -> Result<JobResult, &'static str> {
    let compiled =
        compile(circuit, &TOPOLOGY).map_err(|_| "circuit needs more qubits than the chip has")?;
    let chip = Chip::new(&compiled)?;
    let n = compiled.logical_qubits;
    let mut counts: BTreeMap<usize, u32> = BTreeMap::new();

    if noise.gates_are_ideal() {
        // Ohne Gatterfehler ist der Endzustand immer gleich: einmal rechnen,
        // dann nur noch würfeln.
        let state = chip.run(&compiled, &NoiseModel::IDEAL)?;
        let mut cumulative = Vec::new();
        cumulative
            .try_reserve_exact(state.amplitudes().len())
            .map_err(|_| "not enough memory")?;
        let mut sum = 0.0;
        for a in state.amplitudes() {
            sum += a.norm_sqr();
            cumulative.push(sum);
        }
        for _ in 0..shots {
            let r = random() * sum;
            let index = cumulative
                .partition_point(|&c| c <= r)
                .min(cumulative.len() - 1);
            let bits = noise.readout(chip.to_logical(index, &compiled), n);
            *counts.entry(bits).or_insert(0) += 1;
        }
    } else {
        // Mit Rauschen sieht jeder Shot anders aus.
        for _ in 0..shots {
            let mut state = chip.run(&compiled, noise)?;
            let index = state.measure_all();
            let bits = noise.readout(chip.to_logical(index, &compiled), n);
            *counts.entry(bits).or_insert(0) += 1;
        }
    }

    let mut counts: Vec<(usize, u32)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(JobResult {
        counts,
        shots,
        logical_qubits: n,
        native_gates: compiled.ops.len(),
        two_qubit_gates: compiled.two_qubit_gates(),
        swaps: compiled.swaps,
        depth: compiled.depth(TOPOLOGY.qubits()),
    })
}

/// Wie gut stimmt der kompilierte Schaltkreis (ohne Rauschen) mit dem
/// idealen überein? 1.0 heißt: exakt derselbe Zustand, bis auf eine globale
/// Phase. Damit lässt sich der Compiler prüfen.
pub fn fidelity(circuit: &Circuit) -> Result<f64, &'static str> {
    let ideal = circuit
        .simulate_ideal()
        .ok_or("could not simulate the circuit")?;
    let compiled =
        compile(circuit, &TOPOLOGY).map_err(|_| "circuit needs more qubits than the chip has")?;
    let chip = Chip::new(&compiled)?;
    let state = chip.run(&compiled, &NoiseModel::IDEAL)?;
    let (mut re, mut im) = (0.0, 0.0);
    for (index, c) in state.amplitudes().iter().enumerate() {
        if c.norm_sqr() < 1e-20 || !chip.ancillas_zero(index, &compiled) {
            continue;
        }
        let i = ideal.amplitudes()[chip.to_logical(index, &compiled)];
        // conj(i) · c
        re += i.re * c.re + i.im * c.im;
        im += i.re * c.im - i.im * c.re;
    }
    Ok(re * re + im * im)
}

/// Der Teil des Chips, den ein kompilierter Schaltkreis wirklich benutzt.
/// Nur diese Qubits werden simuliert, das spart Speicher und Zeit: Ein
/// Schaltkreis auf 3 Qubits braucht 8 Amplituden, nicht 65 536.
struct Chip {
    /// Physisches Qubit → Index im Simulator.
    compact: [usize; MAX_PHYSICAL],
    qubits: usize,
}

impl Chip {
    fn new(compiled: &Compiled) -> Result<Self, &'static str> {
        let mut used = [false; MAX_PHYSICAL];
        for &p in &compiled.final_layout {
            used[p] = true;
        }
        for op in &compiled.ops {
            match *op {
                Native::Rz(q, _) | Native::Sx(q) | Native::X(q) => used[q] = true,
                Native::Cz(a, b) => {
                    used[a] = true;
                    used[b] = true;
                }
            }
        }
        let mut compact = [usize::MAX; MAX_PHYSICAL];
        let mut qubits = 0;
        for (p, &u) in used.iter().enumerate() {
            if u {
                compact[p] = qubits;
                qubits += 1;
            }
        }
        if qubits == 0 {
            return Err("the circuit is empty");
        }
        Ok(Chip { compact, qubits })
    }

    fn run(&self, compiled: &Compiled, noise: &NoiseModel) -> Result<State, &'static str> {
        let mut state = State::new(self.qubits).map_err(|_| "not enough memory for the qubits")?;
        let c = |p: usize| self.compact[p];
        for op in &compiled.ops {
            let _ = match *op {
                Native::Rz(q, theta) => state.apply(&gates::rz(theta), c(q)),
                Native::Sx(q) => state.apply(&SX, c(q)),
                Native::X(q) => state.apply(&gates::X, c(q)),
                Native::Cz(a, b) => state.apply_controlled(&gates::Z, c(a), c(b)),
            };
            match *op {
                Native::Rz(..) => {} // virtuell auf echter Hardware, also ohne Fehler
                Native::Sx(q) | Native::X(q) => noise.after_gate(&mut state, c(q), false),
                Native::Cz(a, b) => {
                    noise.after_gate(&mut state, c(a), true);
                    noise.after_gate(&mut state, c(b), true);
                }
            }
        }
        Ok(state)
    }

    /// Übersetzt einen Basiszustand des Simulators in die Bitfolge der
    /// logischen Qubits.
    fn to_logical(&self, index: usize, compiled: &Compiled) -> usize {
        compiled
            .final_layout
            .iter()
            .enumerate()
            .map(|(logical, &p)| ((index >> self.compact[p]) & 1) << logical)
            .sum()
    }

    /// Sind alle Hilfs-Qubits (benutzt, aber ohne logisches Qubit) im
    /// Basiszustand `index` auf 0?
    fn ancillas_zero(&self, index: usize, compiled: &Compiled) -> bool {
        let mut logical_mask = 0;
        for &p in &compiled.final_layout {
            logical_mask |= 1 << self.compact[p];
        }
        index & !logical_mask == 0
    }
}
