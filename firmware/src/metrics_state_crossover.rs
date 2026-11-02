// ABOUTME: Selects the fixed boot-timed metrics observer crossover phases.
// ABOUTME: Reports actual transitions and marks missed phases without replaying them.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Phase {
    Settling,
    Off1,
    On,
    Off2,
    Finished,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Settling => "settling",
            Self::Off1 => "OFF1",
            Self::On => "ON",
            Self::Off2 => "OFF2",
            Self::Finished => "finished",
        }
    }
}

pub struct Transition {
    pub phase: Phase,
    pub planned_us: u64,
    pub skipped: bool,
}

pub struct Schedule {
    phase: Option<Phase>,
}

impl Schedule {
    pub fn new() -> Self {
        Self { phase: None }
    }

    pub fn advance(&mut self, now_us: u64) -> Option<Transition> {
        let (phase, planned_us) = match now_us {
            0..20_000_000 => (Phase::Settling, 0),
            20_000_000..100_000_000 => (Phase::Off1, 20_000_000),
            100_000_000..180_000_000 => (Phase::On, 100_000_000),
            180_000_000..260_000_000 => (Phase::Off2, 180_000_000),
            _ => (Phase::Finished, 260_000_000),
        };
        if self.phase == Some(phase) {
            return None;
        }
        let expected = self.phase.map_or(0, |previous| previous as u32 + 1);
        let skipped = phase as u32 != expected;
        self.phase = Some(phase);
        Some(Transition {
            phase,
            planned_us,
            skipped,
        })
    }

    pub fn enabled(&self) -> bool {
        self.phase == Some(Phase::On)
    }
}
