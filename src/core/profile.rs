#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdaptiveProfile {
    #[default]
    Baseline,
    Cautious,
    Defensive,
}

impl AdaptiveProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Cautious => "cautious",
            Self::Defensive => "defensive",
        }
    }
}
