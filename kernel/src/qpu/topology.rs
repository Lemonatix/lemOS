//! Die Topologie des Chips: welche physischen Qubits direkt miteinander
//! koppeln.
//!
//! Unser virtuelles QPU ist ein 4×4-Gitter wie viele supraleitende Chips.
//! Jedes Qubit ist mit seinen Nachbarn oben, unten, links und rechts
//! verbunden. Zwei-Qubit-Gatter gehen nur zwischen solchen Nachbarn.
//!
//! ```text
//!  0 -  1 -  2 -  3
//!  |    |    |    |
//!  4 -  5 -  6 -  7
//!  |    |    |    |
//!  8 -  9 - 10 - 11
//!  |    |    |    |
//! 12 - 13 - 14 - 15
//! ```

use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// So viele physische Qubits hat das größte Gitter.
pub const MAX_PHYSICAL: usize = 16;

pub struct Topology {
    rows: usize,
    columns: usize,
}

impl Topology {
    pub const fn grid(rows: usize, columns: usize) -> Self {
        assert!(rows * columns <= MAX_PHYSICAL);
        Topology { rows, columns }
    }

    pub fn qubits(&self) -> usize {
        self.rows * self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    /// Sind die physischen Qubits `a` und `b` direkt gekoppelt?
    pub fn connected(&self, a: usize, b: usize) -> bool {
        let (ra, ca) = (a / self.columns, a % self.columns);
        let (rb, cb) = (b / self.columns, b % self.columns);
        ra.abs_diff(rb) + ca.abs_diff(cb) == 1
    }

    fn neighbors(&self, q: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.qubits()).filter(move |&other| self.connected(q, other))
    }

    /// Kürzester Weg von `from` nach `to` (beide eingeschlossen), per
    /// Breitensuche.
    pub fn shortest_path(&self, from: usize, to: usize) -> Vec<usize> {
        let mut previous = [usize::MAX; MAX_PHYSICAL];
        let mut queue = VecDeque::new();
        previous[from] = from;
        queue.push_back(from);
        while let Some(q) = queue.pop_front() {
            if q == to {
                break;
            }
            for next in self.neighbors(q) {
                if previous[next] == usize::MAX {
                    previous[next] = q;
                    queue.push_back(next);
                }
            }
        }
        let mut path = alloc::vec![to];
        let mut q = to;
        while q != from {
            q = previous[q];
            path.push(q);
        }
        path.reverse();
        path
    }
}
