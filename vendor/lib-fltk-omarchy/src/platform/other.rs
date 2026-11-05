//! Other operating systems (BSDs, …): portable fallbacks only.

pub(super) use super::generic::*;

pub(super) fn prefers_dark() -> Option<bool> {
    None
}
