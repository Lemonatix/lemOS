//! Quantenschaltkreise: die Sprache, in der Programme mit dem QPU reden.
//!
//! Ein Schaltkreis ist eine Liste von Gattern auf *logischen* Qubits. Welche
//! physischen Qubits des Chips am Ende benutzt werden, entscheidet der
//! Compiler. Als Textformat gibt es eine kleine Teilmenge von OpenQASM, ein
//! Gatter pro Anweisung, getrennt durch `;` oder Zeilenumbrüche:
//!
//! ```text
//! h 0; cx 0 1; rz 1 45; measure
//! ```
//!
//! Winkel sind in Grad. Gemessen werden am Ende immer alle logischen Qubits.

use alloc::vec::Vec;

use crate::quantum::{MAX_QUBITS, State, gates};

/// Ein Gatter auf logischen Qubits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    H(usize),
    X(usize),
    Y(usize),
    Z(usize),
    S(usize),
    T(usize),
    /// Drehungen, Winkel im Bogenmaß.
    Rx(usize, f64),
    Ry(usize, f64),
    Rz(usize, f64),
    Cx(usize, usize),
    Cz(usize, usize),
    Swap(usize, usize),
}

impl Op {
    /// Die Qubits, auf die das Gatter wirkt.
    pub fn qubits(&self) -> (usize, Option<usize>) {
        match *self {
            Op::H(q)
            | Op::X(q)
            | Op::Y(q)
            | Op::Z(q)
            | Op::S(q)
            | Op::T(q)
            | Op::Rx(q, _)
            | Op::Ry(q, _)
            | Op::Rz(q, _) => (q, None),
            Op::Cx(a, b) | Op::Cz(a, b) | Op::Swap(a, b) => (a, Some(b)),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    UnknownGate,
    MissingQubit,
    MissingAngle,
    BadNumber,
    TooManyQubits,
    SameQubit,
    Empty,
}

impl ParseError {
    pub fn describe(&self) -> &'static str {
        match self {
            ParseError::UnknownGate => "unknown gate",
            ParseError::MissingQubit => "missing qubit number",
            ParseError::MissingAngle => "missing angle in degrees",
            ParseError::BadNumber => "expected a number",
            ParseError::TooManyQubits => "only logical qubits 0-15 exist",
            ParseError::SameQubit => "the two qubits must be different",
            ParseError::Empty => "the circuit is empty",
        }
    }
}

/// Ein Schaltkreis auf logischen Qubits.
#[derive(Clone, Debug, Default)]
pub struct Circuit {
    pub ops: Vec<Op>,
}

impl Circuit {
    pub const fn new() -> Self {
        Circuit { ops: Vec::new() }
    }

    /// Liest einen Schaltkreis aus Text, z. B. `h 0; cx 0 1`.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut circuit = Circuit::new();
        circuit.append(text)?;
        if circuit.ops.is_empty() {
            return Err(ParseError::Empty);
        }
        Ok(circuit)
    }

    /// Hängt weitere Anweisungen an. Bei einem Fehler bleibt der Schaltkreis
    /// unverändert.
    pub fn append(&mut self, text: &str) -> Result<(), ParseError> {
        let mut new_ops = Vec::new();
        for statement in text.split([';', '\n']) {
            if let Some(op) = parse_statement(statement)? {
                new_ops.push(op);
            }
        }
        self.ops.extend(new_ops);
        Ok(())
    }

    /// Anzahl der logischen Qubits: höchste benutzte Nummer plus eins.
    pub fn qubits(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op.qubits() {
                (a, Some(b)) => a.max(b) + 1,
                (a, None) => a + 1,
            })
            .max()
            .unwrap_or(0)
    }

    /// Führt den Schaltkreis ideal, also ohne Chip und ohne Rauschen, auf dem
    /// Simulator aus. Dient als Referenz für den Compiler.
    pub fn simulate_ideal(&self) -> Option<State> {
        let mut state = State::new(self.qubits()).ok()?;
        for op in &self.ops {
            apply_ideal(&mut state, op).ok()?;
        }
        Some(state)
    }
}

fn apply_ideal(state: &mut State, op: &Op) -> Result<(), crate::quantum::Error> {
    match *op {
        Op::H(q) => state.apply(&gates::H, q),
        Op::X(q) => state.apply(&gates::X, q),
        Op::Y(q) => state.apply(&gates::Y, q),
        Op::Z(q) => state.apply(&gates::Z, q),
        Op::S(q) => state.apply(&gates::S, q),
        Op::T(q) => state.apply(&gates::T, q),
        Op::Rx(q, theta) => state.apply(&gates::rx(theta), q),
        Op::Ry(q, theta) => state.apply(&gates::ry(theta), q),
        Op::Rz(q, theta) => state.apply(&gates::rz(theta), q),
        Op::Cx(c, t) => state.apply_controlled(&gates::X, c, t),
        Op::Cz(a, b) => state.apply_controlled(&gates::Z, a, b),
        Op::Swap(a, b) => state.swap(a, b),
    }
}

fn parse_statement(statement: &str) -> Result<Option<Op>, ParseError> {
    let mut words = statement.split_whitespace();
    let Some(name) = words.next() else {
        return Ok(None);
    };
    let args: Vec<&str> = words.collect();
    let qubit = |i: usize| -> Result<usize, ParseError> {
        let q: usize = args
            .get(i)
            .ok_or(ParseError::MissingQubit)?
            .parse()
            .map_err(|_| ParseError::BadNumber)?;
        if q >= MAX_QUBITS {
            return Err(ParseError::TooManyQubits);
        }
        Ok(q)
    };
    let angle = |i: usize| -> Result<f64, ParseError> {
        let degrees: f64 = args
            .get(i)
            .ok_or(ParseError::MissingAngle)?
            .parse()
            .map_err(|_| ParseError::BadNumber)?;
        Ok(degrees.to_radians())
    };
    let pair = || -> Result<(usize, usize), ParseError> {
        let (a, b) = (qubit(0)?, qubit(1)?);
        if a == b {
            return Err(ParseError::SameQubit);
        }
        Ok((a, b))
    };
    let op = match name {
        // Gemessen wird ohnehin am Ende; `measure` ist nur zur Lesbarkeit da.
        "measure" | "barrier" => return Ok(None),
        "h" => Op::H(qubit(0)?),
        "x" => Op::X(qubit(0)?),
        "y" => Op::Y(qubit(0)?),
        "z" => Op::Z(qubit(0)?),
        "s" => Op::S(qubit(0)?),
        "t" => Op::T(qubit(0)?),
        "rx" => Op::Rx(qubit(0)?, angle(1)?),
        "ry" => Op::Ry(qubit(0)?, angle(1)?),
        "rz" => Op::Rz(qubit(0)?, angle(1)?),
        "cx" | "cnot" => {
            let (a, b) = pair()?;
            Op::Cx(a, b)
        }
        "cz" => {
            let (a, b) = pair()?;
            Op::Cz(a, b)
        }
        "swap" => {
            let (a, b) = pair()?;
            Op::Swap(a, b)
        }
        _ => return Err(ParseError::UnknownGate),
    };
    Ok(Some(op))
}
