//! Lifetime ownership for probe children, including abrupt application exit.
#[cfg(windows)]
mod windows;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub(super) use windows::spawn;
#[cfg(unix)]
pub(super) use unix::spawn;

#[cfg(test)]
mod tests;
