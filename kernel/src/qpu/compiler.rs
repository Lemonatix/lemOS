//! Der Compiler: macht aus einem Schaltkreis auf logischen Qubits ein
//! Programm, das der Chip wirklich ausführen kann.
//!
//! Echte Quantenchips können nur wenige Gatter direkt (den „nativen“
//! Gatter-Satz), und Zwei-Qubit-Gatter gehen nur zwischen Nachbarn. Der
//! Compiler arbeitet deshalb in drei Schritten:
//!
//! 1. **Routing:** Liegen zwei Qubits eines Zwei-Qubit-Gatters nicht
//!    nebeneinander, schiebt er eines mit SWAPs auf dem kürzesten Weg heran.
//!    Ein logisches `swap` kostet dagegen nichts, es ändert nur die
//!    Zuordnung logisch → physisch.
//! 2. **Zerlegung:** Jedes Gatter wird in die nativen Gatter RZ, SX, X und CZ
//!    übersetzt (bis auf eine globale Phase, die man nicht messen kann).
//! 3. **Optimierung:** Aufeinanderfolgende RZ auf demselben Qubit werden
//!    zusammengefasst, Drehungen um 0° fallen weg.

use alloc::vec::Vec;
use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use super::circuit::{Circuit, Op};
use super::topology::Topology;

/// Die Gatter, die das virtuelle QPU direkt ausführt, auf physischen Qubits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Native {
    /// Drehung um die Z-Achse. Auf echter Hardware „virtuell“ und gratis.
    Rz(usize, f64),
    /// Wurzel aus X, also eine 90°-Drehung um die X-Achse.
    Sx(usize),
    X(usize),
    /// Controlled Z zwischen zwei Nachbarn.
    Cz(usize, usize),
}

impl Native {
    pub fn is_two_qubit(&self) -> bool {
        matches!(self, Native::Cz(..))
    }
}

/// Das Ergebnis des Compilers.
pub struct Compiled {
    pub ops: Vec<Native>,
    /// Wie viele logische Qubits der Schaltkreis hat.
    pub logical_qubits: usize,
    /// Auf welchem physischen Qubit jedes logische Qubit am Ende liegt.
    pub final_layout: Vec<usize>,
    /// Wie viele SWAPs das Routing einfügen musste.
    pub swaps: usize,
}

impl Compiled {
    /// Tiefe: die Zahl der Schritte, wenn alles parallel läuft, was parallel
    /// laufen kann.
    pub fn depth(&self, qubits: usize) -> usize {
        let mut level = alloc::vec![0usize; qubits];
        for op in &self.ops {
            match *op {
                Native::Rz(q, _) | Native::Sx(q) | Native::X(q) => level[q] += 1,
                Native::Cz(a, b) => {
                    let next = level[a].max(level[b]) + 1;
                    level[a] = next;
                    level[b] = next;
                }
            }
        }
        level.into_iter().max().unwrap_or(0)
    }

    pub fn two_qubit_gates(&self) -> usize {
        self.ops.iter().filter(|op| op.is_two_qubit()).count()
    }
}

#[derive(Debug, PartialEq)]
pub enum CompileError {
    TooManyQubits,
}

pub fn compile(circuit: &Circuit, topology: &Topology) -> Result<Compiled, CompileError> {
    let logical_qubits = circuit.qubits();
    if logical_qubits > topology.qubits() {
        return Err(CompileError::TooManyQubits);
    }
    // Am Anfang liegt logisches Qubit i auf physischem Qubit i.
    let mut layout: Vec<usize> = (0..logical_qubits).collect();
    let mut emitter = Emitter::default();
    let mut swaps = 0;

    for op in &circuit.ops {
        match *op {
            Op::Swap(a, b) => layout.swap(a, b),
            Op::Cx(a, b) | Op::Cz(a, b) => {
                swaps += route(&mut layout, a, b, topology, &mut emitter);
                let (pa, pb) = (layout[a], layout[b]);
                if matches!(op, Op::Cx(..)) {
                    emitter.cx(pa, pb);
                } else {
                    emitter.push(Native::Cz(pa, pb));
                }
            }
            _ => emitter.single(op, &layout),
        }
    }

    Ok(Compiled {
        ops: emitter.finish(),
        logical_qubits,
        final_layout: layout,
        swaps,
    })
}

/// Schiebt das logische Qubit `a` mit SWAPs neben `b`. Gibt die Zahl der
/// eingefügten SWAPs zurück.
fn route(
    layout: &mut [usize],
    a: usize,
    b: usize,
    topology: &Topology,
    emitter: &mut Emitter,
) -> usize {
    let path = topology.shortest_path(layout[a], layout[b]);
    // path = [start, …, ziel]; wir laufen bis zum vorletzten Feld.
    let mut swaps = 0;
    for step in path.windows(2).take(path.len().saturating_sub(2)) {
        let (from, to) = (step[0], step[1]);
        emitter.swap(from, to);
        // Liegt auf `to` ein logisches Qubit, wandert es nach `from`.
        for p in layout.iter_mut() {
            if *p == to {
                *p = from;
            } else if *p == from {
                *p = to;
            }
        }
        swaps += 1;
    }
    swaps
}

/// Sammelt die nativen Gatter.
#[derive(Default)]
struct Emitter {
    ops: Vec<Native>,
}

impl Emitter {
    fn push(&mut self, op: Native) {
        self.ops.push(op);
    }

    fn rz(&mut self, q: usize, theta: f64) {
        self.push(Native::Rz(q, theta));
    }

    /// H = RZ(90°) · SX · RZ(90°)
    fn h(&mut self, q: usize) {
        self.rz(q, FRAC_PI_2);
        self.push(Native::Sx(q));
        self.rz(q, FRAC_PI_2);
    }

    /// RX(θ) = H · RZ(θ) · H
    fn rx(&mut self, q: usize, theta: f64) {
        self.h(q);
        self.rz(q, theta);
        self.h(q);
    }

    /// CNOT = H auf dem Ziel, CZ, H auf dem Ziel.
    fn cx(&mut self, control: usize, target: usize) {
        self.h(target);
        self.push(Native::Cz(control, target));
        self.h(target);
    }

    /// SWAP = drei CNOTs.
    fn swap(&mut self, a: usize, b: usize) {
        self.cx(a, b);
        self.cx(b, a);
        self.cx(a, b);
    }

    fn single(&mut self, op: &Op, layout: &[usize]) {
        let p = |q: usize| layout[q];
        match *op {
            Op::H(q) => self.h(p(q)),
            Op::X(q) => self.push(Native::X(p(q))),
            // Y = X · Z bis auf eine globale Phase.
            Op::Y(q) => {
                self.rz(p(q), PI);
                self.push(Native::X(p(q)));
            }
            Op::Z(q) => self.rz(p(q), PI),
            Op::S(q) => self.rz(p(q), FRAC_PI_2),
            Op::T(q) => self.rz(p(q), FRAC_PI_4),
            Op::Rz(q, theta) => self.rz(p(q), theta),
            Op::Rx(q, theta) => self.rx(p(q), theta),
            // RY(θ) = RZ(90°) · RX(θ) · RZ(-90°)
            Op::Ry(q, theta) => {
                self.rz(p(q), -FRAC_PI_2);
                self.rx(p(q), theta);
                self.rz(p(q), FRAC_PI_2);
            }
            Op::Cx(..) | Op::Cz(..) | Op::Swap(..) => unreachable!(),
        }
    }

    fn finish(self) -> Vec<Native> {
        optimize(self.ops)
    }
}

/// Vereinfacht die Gatterfolge, bis sich nichts mehr ändert. Betrachtet
/// werden jeweils direkt aufeinanderfolgende Gatter auf demselben Qubit;
/// Gatter auf anderen Qubits dazwischen stören nicht. Alle Regeln gelten bis
/// auf eine globale Phase:
///
/// - RZ(a) RZ(b) = RZ(a+b), und RZ(0) fällt weg
/// - SX SX = X, X X = nichts
/// - SX RZ(180°) SX = RZ(180°), so heben sich z. B. zwei H auf
fn optimize(ops: Vec<Native>) -> Vec<Native> {
    let mut ops: Vec<Option<Native>> = ops.into_iter().map(Some).collect();
    while simplify_once(&mut ops) {}
    ops.into_iter().flatten().collect()
}

/// Wendet eine Regel an, falls eine passt.
fn simplify_once(ops: &mut [Option<Native>]) -> bool {
    for op in ops.iter_mut() {
        if let Some(Native::Rz(_, theta)) = *op
            && is_zero(normalize_angle(theta))
        {
            *op = None;
            return true;
        }
    }
    for q in 0..super::topology::MAX_PHYSICAL {
        let on_q: Vec<usize> = (0..ops.len())
            .filter(|&i| ops[i].is_some_and(|op| touches(&op, q)))
            .collect();
        for w in on_q.windows(2) {
            let (i, j) = (w[0], w[1]);
            match (ops[i], ops[j]) {
                (Some(Native::Rz(_, a)), Some(Native::Rz(_, b))) => {
                    ops[i] = Some(Native::Rz(q, normalize_angle(a + b)));
                    ops[j] = None;
                    return true;
                }
                (Some(Native::Sx(_)), Some(Native::Sx(_))) => {
                    ops[i] = Some(Native::X(q));
                    ops[j] = None;
                    return true;
                }
                (Some(Native::X(_)), Some(Native::X(_))) => {
                    ops[i] = None;
                    ops[j] = None;
                    return true;
                }
                _ => {}
            }
        }
        for w in on_q.windows(3) {
            if let (Some(Native::Sx(_)), Some(Native::Rz(_, theta)), Some(Native::Sx(_))) =
                (ops[w[0]], ops[w[1]], ops[w[2]])
                && is_zero(libm::fabs(normalize_angle(theta)) - PI)
            {
                ops[w[0]] = Some(Native::Rz(q, PI));
                ops[w[1]] = None;
                ops[w[2]] = None;
                return true;
            }
        }
    }
    false
}

fn touches(op: &Native, q: usize) -> bool {
    match *op {
        Native::Rz(p, _) | Native::Sx(p) | Native::X(p) => p == q,
        Native::Cz(a, b) => a == q || b == q,
    }
}

/// Bringt einen Winkel in den Bereich (-π, π].
fn normalize_angle(theta: f64) -> f64 {
    let mut t = theta % TAU;
    if t > PI {
        t -= TAU;
    } else if t <= -PI {
        t += TAU;
    }
    t
}

fn is_zero(theta: f64) -> bool {
    libm::fabs(theta) < 1e-12
}
