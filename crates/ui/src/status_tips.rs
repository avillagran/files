//! Pure, demand-driven status-bar tips. The caller owns clocks and repaint scope.

use std::time::{Duration, Instant};

const FRAME_INTERVAL: Duration = Duration::from_millis(40);
const EDGE_PAUSE: Duration = Duration::from_secs(2);
const PIXELS_PER_SECOND: f64 = 35.0;
const MIN_VIEWPORT: i32 = 80;

const TIPS: [&str; 9] = [
    "Ctrl+L: escribe >comando o :lugar y pulsa Enter para abrir el catálogo filtrado; elige después una acción o destino.",
    "Home/End van a los extremos. Shift extiende el rango; Ctrl mueve el foco sin reemplazar marcas y Ctrl+Shift añade un rango.",
    "Filtra con *.rs o ext:pdf,md y pulsa Ctrl+Shift+A para marcar coincidencias y mostrar todo. Ctrl+Shift+D limpia la selección.",
    "Ctrl+Shift+Espacio calcula o para carpetas listadas. ∑ es tamaño medido; ≥ es avance parcial. Ctrl+R invalida los cálculos.",
    "Ctrl+F filtra nombres: *.pdf, ext:rs,md, type:dir, !draft. Insert marca y avanza; Ctrl+Espacio calcula una carpeta.",
    "Navegador de archivos · Preferencias (Ctrl+,) · Acciones (F10). Ctrl+H muestra u oculta archivos ocultos.",
    "Ctrl+1 Grupos, Ctrl+2 Lista, Ctrl+3 Columnas, Ctrl+4 Espacio y Ctrl+5 Grilla. Ctrl+rueda cambia el zoom de Grilla.",
    "F2 abre el renombrador para la selección. Revisa los nombres de origen y destino en la vista previa antes de aplicar cualquier cambio.",
    "Space abre la vista previa del archivo seleccionado; Esc vuelve a la carpeta. Shift+F10 o la tecla Menú abren las acciones del elemento enfocado.",
];

#[derive(Clone, Copy, Debug)]
enum Phase {
    StartPause(Duration),
    Travel,
    EndPause(Duration),
}

pub struct State {
    tip: usize,
    viewport: i32,
    text_width: f64,
    position: f64,
    phase: Phase,
    hovered: bool,
    last: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tip: 0,
            viewport: 0,
            text_width: 0.0,
            position: 0.0,
            phase: Phase::StartPause(Duration::ZERO),
            hovered: false,
            last: None,
        }
    }
}

impl State {
    pub fn text(&self) -> &'static str {
        TIPS[self.tip]
    }

    /// Nonnegative distance to subtract from the clipped text's initial x.
    pub fn offset(&self) -> i32 {
        self.position.floor() as i32
    }

    /// Supply the measured width of `text()` during the caller's normal paint.
    /// A new tip is unmeasured until this method runs again.
    pub fn configure(&mut self, viewport_px: i32, text_px: f64) {
        let viewport = viewport_px.max(0);
        let text_width = if text_px.is_finite() && text_px > 0.0 {
            text_px.min(f64::from(i32::MAX))
        } else {
            0.0
        };
        if viewport != self.viewport || text_width != self.text_width {
            self.viewport = viewport;
            self.text_width = text_width;
            self.position = 0.0;
            self.phase = Phase::StartPause(Duration::ZERO);
            self.last = None;
        }
    }

    pub fn set_hover(&mut self, hovered: bool) {
        if self.hovered != hovered {
            self.hovered = hovered;
            self.last = None;
        }
    }

    /// Returns true only when the clipped text or its integer offset changes.
    /// No timers, threads, sleeps, or repaint requests are owned by this state.
    pub fn tick(&mut self, now: Instant, active: bool) -> bool {
        let end = (self.text_width - f64::from(self.viewport)).max(0.0);
        if !active || self.hovered || self.viewport < MIN_VIEWPORT || end <= 0.0 {
            self.last = None;
            return false;
        }
        let Some(last) = self.last else {
            self.last = Some(now);
            return false;
        };
        let Some(mut elapsed) = now.checked_duration_since(last) else {
            self.last = Some(now);
            return false;
        };
        if elapsed < FRAME_INTERVAL {
            return false;
        }
        self.last = Some(now);
        let previous = self.offset();
        // At most start-pause -> travel -> end-pause -> one rotation per tick.
        loop {
            match self.phase {
                Phase::StartPause(waited) => {
                    let remaining = EDGE_PAUSE.saturating_sub(waited);
                    if elapsed < remaining {
                        self.phase = Phase::StartPause(waited.saturating_add(elapsed));
                        break;
                    }
                    elapsed = elapsed.saturating_sub(remaining);
                    self.phase = Phase::Travel;
                }
                Phase::Travel => {
                    let distance = elapsed.as_secs_f64() * PIXELS_PER_SECOND;
                    let remaining = (end - self.position).max(0.0);
                    if distance < remaining {
                        self.position += distance;
                        break;
                    }
                    self.position = end;
                    let travel_time = Duration::from_secs_f64(remaining / PIXELS_PER_SECOND);
                    elapsed = elapsed.saturating_sub(travel_time);
                    self.phase = Phase::EndPause(Duration::ZERO);
                }
                Phase::EndPause(waited) => {
                    let remaining = EDGE_PAUSE.saturating_sub(waited);
                    if elapsed < remaining {
                        self.phase = Phase::EndPause(waited.saturating_add(elapsed));
                        break;
                    }
                    self.tip = (self.tip + 1) % TIPS.len();
                    self.position = 0.0;
                    self.text_width = 0.0;
                    self.phase = Phase::StartPause(Duration::ZERO);
                    self.last = None;
                    return true;
                }
            }
        }
        self.offset() != previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(now: Instant, width: f64) -> State {
        let mut state = State::default();
        state.configure(100, width);
        assert!(!state.tick(now, true));
        state
    }

    #[test]
    fn start_pause_speed_and_frame_limit() {
        let now = Instant::now();
        let mut state = ready(now, 450.0);
        assert!(!state.tick(now + EDGE_PAUSE, true));
        assert_eq!(state.offset(), 0);
        assert!(state.tick(now + EDGE_PAUSE + Duration::from_secs(1), true));
        assert_eq!(state.offset(), 35);
        assert!(!state.tick(now + EDGE_PAUSE + Duration::from_millis(1039), true));
        assert_eq!(state.offset(), 35);
        assert!(state.tick(now + EDGE_PAUSE + Duration::from_millis(1040), true));
        assert_eq!(state.offset(), 36);
    }

    #[test]
    fn end_pause_rotates_and_requires_new_measurement() {
        let now = Instant::now();
        let mut state = ready(now, 170.0);
        let original = state.text();
        assert!(!state.tick(now + EDGE_PAUSE, true));
        assert!(state.tick(now + Duration::from_secs(4), true));
        assert_eq!(state.offset(), 70);
        assert!(!state.tick(now + Duration::from_millis(5999), true));
        assert_eq!(state.text(), original);
        assert!(state.tick(now + Duration::from_millis(6039), true));
        assert_ne!(state.text(), original);
        assert_eq!(state.offset(), 0);
        assert!(!state.tick(now + Duration::from_secs(100), true));
        assert_eq!(state.offset(), 0);
        state.configure(100, 170.0);
        assert!(!state.tick(now + Duration::from_secs(101), true));
        assert!(!state.tick(now + Duration::from_secs(103), true));
        assert!(state.tick(now + Duration::from_secs(104), true));
        assert_eq!(state.offset(), 35);
    }

    #[test]
    fn inactive_and_hover_pause_without_resume_jump() {
        let now = Instant::now();
        let mut state = ready(now, 450.0);
        assert!(!state.tick(now + EDGE_PAUSE, true));
        assert!(state.tick(now + Duration::from_secs(3), true));
        assert_eq!(state.offset(), 35);
        assert!(!state.tick(now + Duration::from_secs(50), false));
        assert!(!state.tick(now + Duration::from_secs(100), true));
        assert_eq!(state.offset(), 35);
        assert!(state.tick(now + Duration::from_secs(101), true));
        assert_eq!(state.offset(), 70);
        state.set_hover(true);
        assert!(!state.tick(now + Duration::from_secs(200), true));
        state.set_hover(false);
        assert!(!state.tick(now + Duration::from_secs(300), true));
        assert_eq!(state.offset(), 70);
        assert!(state.tick(now + Duration::from_secs(301), true));
        assert_eq!(state.offset(), 105);
    }

    #[test]
    fn no_overflow_and_narrow_viewports_never_repaint() {
        let now = Instant::now();
        for (viewport, text) in [
            (100, 100.0),
            (100, 99.0),
            (79, 500.0),
            (0, 500.0),
            (-1, 500.0),
        ] {
            let mut state = State::default();
            state.configure(viewport, text);
            let original = state.text();
            assert!(!state.tick(now, true));
            assert!(!state.tick(now + Duration::from_secs(1000), true));
            assert_eq!(state.offset(), 0);
            assert_eq!(state.text(), original);
        }
    }

    #[test]
    fn resize_or_new_measurement_resets_but_identical_paint_does_not() {
        let now = Instant::now();
        let mut state = ready(now, 450.0);
        assert!(state.tick(now + Duration::from_secs(3), true));
        assert_eq!(state.offset(), 35);
        state.configure(100, 450.0);
        assert_eq!(state.offset(), 35);
        assert!(state.tick(now + Duration::from_secs(4), true));
        assert_eq!(state.offset(), 70);
        state.configure(120, 450.0);
        assert_eq!(state.offset(), 0);
        assert!(!state.tick(now + Duration::from_secs(5), true));
        assert!(!state.tick(now + Duration::from_secs(7), true));
        assert!(state.tick(now + Duration::from_secs(8), true));
        state.configure(120, 500.0);
        assert_eq!(state.offset(), 0);
    }

    #[test]
    fn invalid_dimensions_and_reversed_clock_are_safe() {
        let now = Instant::now();
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut state = ready(now, width);
            assert!(!state.tick(now + Duration::from_secs(100), true));
            assert_eq!(state.offset(), 0);
        }
        let mut state = ready(now + Duration::from_secs(1), f64::MAX);
        assert!(!state.tick(now, true));
        assert!(state.tick(now + Duration::from_secs(3), true));
        assert_eq!(state.offset(), 35);
        state.configure(i32::MAX, f64::MAX);
        assert!(!state.tick(now + Duration::from_secs(100), true));
        assert_eq!(state.offset(), 0);
    }
}
