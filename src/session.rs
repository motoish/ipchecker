#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Session {
    muted: bool,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn toggle_muted(&mut self) {
        self.muted = !self.muted;
    }
}
