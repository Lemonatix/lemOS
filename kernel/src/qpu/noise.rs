//! Rauschen: was echte Qubits falsch machen.
//!
//! Echte Qubits sind empfindlich. Das virtuelle QPU bildet drei typische
//! Fehler nach, jeweils zufällig pro Durchlauf („Shot“). So entsteht über
//! viele Shots dasselbe Bild wie bei einer Dichtematrix-Simulation, ohne
//! deren 4ⁿ Speicher zu brauchen (Monte-Carlo- bzw. Trajektorien-Methode):
//!
//! - **Depolarisierung:** Nach einem Gatter trifft mit Wahrscheinlichkeit p
//!   ein zufälliger Pauli-Fehler (X, Y oder Z) das Qubit.
//! - **Amplitudendämpfung (T1):** Ein Qubit in |1> fällt mit der Zeit in |0>
//!   zurück, weil es Energie an die Umgebung abgibt.
//! - **Auslesefehler:** Die Messung meldet manchmal das falsche Bit.

use crate::quantum::{Complex, Gate, State, gates, random};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseModel {
    /// Fehlerwahrscheinlichkeit nach einem Ein-Qubit-Gatter.
    pub single: f64,
    /// Fehlerwahrscheinlichkeit pro Qubit nach einem Zwei-Qubit-Gatter.
    pub two: f64,
    /// Wahrscheinlichkeit, dass ein gemessenes Bit kippt.
    pub readout: f64,
    /// Dämpfung γ pro Gatter: Anteil von |1>, der nach |0> zerfällt.
    pub damping: f64,
}

impl NoiseModel {
    pub const IDEAL: NoiseModel = NoiseModel {
        single: 0.0,
        two: 0.0,
        readout: 0.0,
        damping: 0.0,
    };

    /// Ungefähr die Größenordnung heutiger supraleitender Chips.
    pub const DEFAULT: NoiseModel = NoiseModel {
        single: 0.001,
        two: 0.01,
        readout: 0.02,
        damping: 0.002,
    };

    /// Wirken die Gatter fehlerfrei? Dann reicht eine einzige Simulation für
    /// alle Shots.
    pub fn gates_are_ideal(&self) -> bool {
        self.single == 0.0 && self.two == 0.0 && self.damping == 0.0
    }

    /// Rauschen nach einem Gatter auf `qubit`.
    pub fn after_gate(&self, state: &mut State, qubit: usize, two_qubit: bool) {
        let p = if two_qubit { self.two } else { self.single };
        depolarize(state, qubit, p);
        damp(state, qubit, self.damping);
    }

    /// Kippt jedes der `bits` untersten Bits mit der Auslese-Wahrscheinlichkeit.
    pub fn readout(&self, mut value: usize, bits: usize) -> usize {
        if self.readout > 0.0 {
            for k in 0..bits {
                if random() < self.readout {
                    value ^= 1 << k;
                }
            }
        }
        value
    }
}

fn depolarize(state: &mut State, qubit: usize, p: f64) {
    if p <= 0.0 || random() >= p {
        return;
    }
    let pauli = match (random() * 3.0) as u32 {
        0 => gates::X,
        1 => gates::Y,
        _ => gates::Z,
    };
    let _ = state.apply(&pauli, qubit);
}

/// Amplitudendämpfung als Quantensprung: Mit Wahrscheinlichkeit γ·P(1)
/// springt das Qubit nach |0> (Kraus-Operator K1), sonst wird der |1>-Anteil
/// etwas kleiner (K0). Danach wird wieder normiert.
fn damp(state: &mut State, qubit: usize, gamma: f64) {
    if gamma <= 0.0 {
        return;
    }
    let Ok(p_one) = state.probability_one(qubit) else {
        return;
    };
    let o = Complex::ZERO;
    let kraus: Gate = if random() < gamma * p_one {
        [[o, Complex::new(libm::sqrt(gamma), 0.0)], [o, o]]
    } else {
        [
            [Complex::ONE, o],
            [o, Complex::new(libm::sqrt(1.0 - gamma), 0.0)],
        ]
    };
    let _ = state.apply(&kraus, qubit);
    state.normalize();
}
