//! Process-scoped native presentation choices.

/// Native window policy chosen before the event loop creates a window.
///
/// This is deliberately not part of [`ViewerConfig`](crate::config::ViewerConfig): native
/// transparency cannot safely be changed after the window has been created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationPolicy {
    opaque_window: bool,
}

impl PresentationPolicy {
    /// Builds a policy for in-crate tests and startup integration.
    pub(crate) const fn with_opaque_window(opaque_window: bool) -> Self {
        Self { opaque_window }
    }

    /// Reads the explicit compatibility override for this process.
    pub fn from_environment() -> Self {
        Self::from_opaque_window_value(std::env::var("VIEWLUME_OPAQUE_WINDOW").ok().as_deref())
    }

    /// Whether this process requested a genuinely opaque native window.
    pub const fn opaque_window(self) -> bool {
        self.opaque_window
    }

    /// Whether the native window should be created with transparency support.
    pub const fn transparent_window(self) -> bool {
        !self.opaque_window
    }

    fn from_opaque_window_value(value: Option<&str>) -> Self {
        Self::with_opaque_window(matches!(value, Some("1")))
    }
}

#[cfg(test)]
mod tests {
    use super::PresentationPolicy;

    #[test]
    fn opaque_override_requires_the_exact_enabled_value() {
        assert!(PresentationPolicy::from_opaque_window_value(Some("1")).opaque_window());
        assert!(!PresentationPolicy::from_opaque_window_value(Some("true")).opaque_window());
        assert!(!PresentationPolicy::from_opaque_window_value(Some(" 1")).opaque_window());
        assert!(!PresentationPolicy::from_opaque_window_value(None).opaque_window());
    }

    #[test]
    fn transparent_windows_remain_the_default() {
        let policy = PresentationPolicy::from_opaque_window_value(None);
        assert!(policy.transparent_window());
    }
}
