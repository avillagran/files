//! A press belongs to exactly one drag attempt. No filesystem or Wayland I/O.
//! The opaque permit is retained only while the original button remains held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub owner: (u64, u64),
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Started {
    pub permit: u64,
    pub context: Context,
}
#[derive(Default)]
pub struct Origin {
    press: Option<Press>,
}
struct Press {
    start: (i32, i32),
    permit: u64,
    context: Context,
    started: bool,
}
impl Origin {
    pub fn active(&self) -> bool {
        self.press.is_some()
    }
    pub fn press(&mut self, permit: u64, context: Context, x: i32, y: i32) {
        self.press = (permit != 0).then_some(Press {
            start: (x, y),
            permit,
            context,
            started: false,
        });
    }
    /// Once beyond the threshold, start a single asynchronous proof operation.
    pub fn motion(&mut self, context: Context, x: i32, y: i32) -> Option<Started> {
        let press = self.press.as_mut()?;
        if press.context != context {
            self.press = None;
            return None;
        }
        let dx = i128::from(x) - i128::from(press.start.0);
        let dy = i128::from(y) - i128::from(press.start.1);
        if press.started || dx * dx + dy * dy < 64 {
            return None;
        }
        press.started = true;
        Some(Started {
            permit: press.permit,
            context: press.context,
        })
    }
    /// A late proof cannot authorize a replacement press or changed panel.
    pub fn accepts(&self, started: Started, context: Context) -> bool {
        self.press.as_ref().is_some_and(|p| {
            p.started
                && p.permit == started.permit
                && p.context == context
                && p.context == started.context
        })
    }
    pub fn cancel(&mut self) {
        self.press = None;
    }
    pub fn started(&self) -> bool {
        self.press.as_ref().is_some_and(|p| p.started)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context {
            owner: (1, 2),
            generation: 3,
        }
    }
    #[test]
    fn threshold_and_release_prevent_late_proof() {
        let mut o = Origin::default();
        o.press(8, context(), 10, 10);
        assert!(o.motion(context(), 17, 10).is_none());
        let s = o.motion(context(), 18, 10).unwrap();
        assert!(o.accepts(s, context()));
        assert!(o.motion(context(), 30, 10).is_none());
        o.cancel();
        assert!(!o.accepts(s, context()));
    }
    #[test]
    fn changed_context_and_replacement_press_never_authorize_old_proof() {
        let mut o = Origin::default();
        o.press(8, context(), 0, 0);
        let s = o.motion(context(), 8, 0).unwrap();
        o.press(9, context(), 0, 0);
        assert!(!o.accepts(s, context()));
        let other = Context {
            generation: 4,
            ..context()
        };
        assert!(o.motion(other, 10, 0).is_none());
        assert!(!o.accepts(s, other));
    }
    #[test]
    fn zero_permit_and_extreme_coordinates_are_safe() {
        let mut o = Origin::default();
        o.press(0, context(), 0, 0);
        assert!(o.motion(context(), 100, 0).is_none());
        o.press(1, context(), i32::MIN, 0);
        assert!(o.motion(context(), i32::MAX, 0).is_some());
    }
}
