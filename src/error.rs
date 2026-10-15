//! Error handling.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Response {
    /// The interaction completed successfully.
    Success = 0,
    /// The user cancelled the interaction.
    Cancelled = 1,
    /// Something else went wrong.
    OtherError = 2,
}

impl Response {
    pub fn code(self) -> u32 {
        self as u32
    }
}
