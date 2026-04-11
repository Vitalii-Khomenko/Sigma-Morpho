use clap::ValueEnum;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, PRAGMA};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ClientProfile {
    ResearchDefault,
    BrowserDesktop,
    MobileSafari,
    ApiDiagnostic,
}

impl ClientProfile {
    pub const ALL: [ClientProfile; 4] = [
        ClientProfile::ResearchDefault,
        ClientProfile::BrowserDesktop,
        ClientProfile::MobileSafari,
        ClientProfile::ApiDiagnostic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResearchDefault => "research-default",
            Self::BrowserDesktop => "browser-desktop",
            Self::MobileSafari => "mobile-safari",
            Self::ApiDiagnostic => "api-diagnostic",
        }
    }

    pub fn user_agent(self) -> &'static str {
        match self {
            Self::ResearchDefault => "sigma-morpho/0.1 research-profile",
            Self::BrowserDesktop => {
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0 Safari/537.36"
            }
            Self::MobileSafari => {
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
            }
            Self::ApiDiagnostic => "sigma-morpho-api/0.1 diagnostic-profile",
        }
    }

    pub fn default_headers(self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));

        match self {
            Self::ResearchDefault => {
                headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
            }
            Self::BrowserDesktop => {
                headers.insert(
                    ACCEPT,
                    HeaderValue::from_static(
                        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    ),
                );
                headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
            }
            Self::MobileSafari => {
                headers.insert(
                    ACCEPT,
                    HeaderValue::from_static(
                        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    ),
                );
                headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-us"));
            }
            Self::ApiDiagnostic => {
                headers.insert(
                    ACCEPT,
                    HeaderValue::from_static("application/json,*/*;q=0.5"),
                );
            }
        }

        headers
    }

    pub fn adjusted_initial_delay(self, initial_ms: u64, min_ms: u64, max_ms: u64) -> u64 {
        let adjusted = match self {
            Self::ResearchDefault => initial_ms,
            Self::BrowserDesktop => initial_ms.saturating_add(25),
            Self::MobileSafari => initial_ms.saturating_add(40),
            Self::ApiDiagnostic => initial_ms.saturating_add(10),
        };

        adjusted.clamp(min_ms, max_ms)
    }

    pub fn adjusted_timeout(self, timeout_ms: u64) -> u64 {
        match self {
            Self::ResearchDefault => timeout_ms,
            Self::BrowserDesktop => timeout_ms.saturating_add(500),
            Self::MobileSafari => timeout_ms.saturating_add(750),
            Self::ApiDiagnostic => timeout_ms.saturating_add(250),
        }
    }
}

impl Default for ClientProfile {
    fn default() -> Self {
        Self::ResearchDefault
    }
}

#[cfg(test)]
mod tests {
    use super::ClientProfile;

    #[test]
    fn initial_delay_adjustment_respects_bounds() {
        let adjusted = ClientProfile::MobileSafari.adjusted_initial_delay(50, 25, 80);
        assert_eq!(adjusted, 80);
    }

    #[test]
    fn headers_are_present_for_profiles() {
        for profile in ClientProfile::ALL {
            let headers = profile.default_headers();
            assert!(!headers.is_empty());
            assert!(!profile.user_agent().is_empty());
        }
    }
}
