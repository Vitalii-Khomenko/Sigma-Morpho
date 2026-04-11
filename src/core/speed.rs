use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Default)]
pub enum SpeedMode {
    Safe,
    #[default]
    Balanced,
    Fast,
    Aggressive,
}

impl SpeedMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Balanced => "balanced",
            Self::Fast => "fast",
            Self::Aggressive => "aggressive",
        }
    }

    pub fn delay_cap(self, configured_max: u64) -> u64 {
        match self {
            Self::Safe => configured_max,
            Self::Balanced => configured_max.min(750),
            Self::Fast => configured_max.min(200),
            Self::Aggressive => configured_max.min(50),
        }
    }

    pub fn recovery_bonus_ms(self) -> u64 {
        match self {
            Self::Safe => 0,
            Self::Balanced => 20,
            Self::Fast => 80,
            Self::Aggressive => 150,
        }
    }

    pub fn ignore_soft_404_for_neuro(self) -> bool {
        matches!(self, Self::Fast | Self::Aggressive)
    }

    pub fn pool_max_idle_per_host(self, workers: usize) -> usize {
        let factor = match self {
            Self::Safe => 1,
            Self::Balanced => 2,
            Self::Fast => 3,
            Self::Aggressive => 4,
        };

        (workers.saturating_mul(factor)).max(8)
    }
}
