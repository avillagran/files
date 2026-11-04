//! Apparent-byte space-map contract and pure proportional layout.
//!
//! Measurements belong to an ephemeral scanner. Bytes count apparent regular
//! file lengths by name (hard-link aliases may count twice), not allocated disk
//! blocks or an atomic filesystem snapshot. Partial/error observations are lower
//! bounds and NEVER get a fabricated area. No filesystem or UI work occurs here.
use serde::{Deserialize, Serialize};

pub const MAX_NODES: usize = 2048;
pub const MAX_NAME_BYTES: usize = 4096;
pub const MAX_NAMES_BYTES: usize = 256 * 1024;
pub const MAX_ERROR_BYTES: usize = 512;
pub const MAX_ERRORS_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Complete,
    Partial,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceNode {
    /// Lossless single-component filename bytes; decode only for display.
    pub name: Vec<u8>,
    pub kind: NodeKind,
    /// Exact completed apparent-byte measurement only. None means unknown.
    pub bytes: Option<u64>,
    /// Measured lower bound, including observations from an incomplete walk.
    pub observed_bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub status: NodeStatus,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceReport {
    pub nodes: Vec<SpaceNode>,
    /// Checked sum of exact Complete measurements only, not an estimated total.
    pub total_bytes: u64,
    /// Lower bound summed across all measurements, including incomplete nodes.
    pub observed_bytes: u64,
    pub visited: u64,
    pub complete: bool,
    pub reason: Option<String>,
    /// None if root enumeration did not finish, not a claim of zero omissions.
    pub omitted_children: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    /// Original node index, preserved independently of the sorted layout order.
    pub node: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn preflight(nodes: &[SpaceNode]) -> Result<u64, String> {
    if nodes.len() > MAX_NODES {
        return Err("El mapa supera 2048 elementos".into());
    }
    let (mut names, mut errors, mut total, mut observed) = (0usize, 0usize, 0u64, 0u64);
    for node in nodes {
        if node.name.is_empty()
            || node.name.len() > MAX_NAME_BYTES
            || node.name.as_slice() == b"."
            || node.name.as_slice() == b".."
            || node.name.iter().any(|byte| matches!(*byte, b'/' | 0))
        {
            return Err("Nombre de elemento no válido para el mapa".into());
        }
        names = names
            .checked_add(node.name.len())
            .filter(|sum| *sum <= MAX_NAMES_BYTES)
            .ok_or("El mapa excede el presupuesto de nombres")?;
        if let Some(error) = &node.error {
            if error.len() > MAX_ERROR_BYTES {
                return Err("Diagnóstico del mapa demasiado largo".into());
            }
            errors = errors
                .checked_add(error.len())
                .filter(|sum| *sum <= MAX_ERRORS_BYTES)
                .ok_or("El mapa excede el presupuesto de diagnósticos")?;
        }
        observed = observed
            .checked_add(node.observed_bytes)
            .ok_or("Los bytes observados del mapa exceden u64")?;
        match (node.status, node.bytes) {
            (NodeStatus::Complete, Some(bytes)) => {
                if bytes != node.observed_bytes {
                    return Err("Medición completa incoherente con sus bytes observados".into());
                }
                total = total
                    .checked_add(bytes)
                    .ok_or("El tamaño del mapa excede u64")?;
            }
            (_, Some(_)) => {
                return Err("Un elemento incompleto no puede declarar un tamaño exacto".into());
            }
            (_, None) => {}
        }
    }
    Ok(total)
}

/// Validate one bounded scanner report before admitting it to the UI. Complete
/// means enumeration finished and every child has an exact measurement, never
/// that the changing filesystem was an atomic snapshot.
pub fn validate_report(report: &SpaceReport) -> Result<(), String> {
    let total = preflight(&report.nodes)?;
    let mut observed = 0u64;
    let mut counted = 0u64;
    let mut errors = report.reason.as_ref().map_or(0, String::len);
    if errors > MAX_ERROR_BYTES || report.visited > 1_000_000 {
        return Err("Informe de espacio fuera de sus límites".into());
    }
    let mut names = std::collections::HashSet::with_capacity(report.nodes.len());
    for node in &report.nodes {
        if !names.insert(node.name.as_slice()) {
            return Err("El informe de espacio repite un nombre".into());
        }
        observed = observed
            .checked_add(node.observed_bytes)
            .ok_or("Bytes observados fuera de rango")?;
        counted = counted
            .checked_add(node.files)
            .and_then(|n| n.checked_add(node.directories))
            .filter(|n| *n <= report.visited)
            .ok_or("Conteos fuera del recorrido observado")?;
        errors = errors
            .checked_add(node.error.as_ref().map_or(0, String::len))
            .filter(|n| *n <= MAX_ERRORS_BYTES)
            .ok_or("Diagnósticos de espacio fuera de presupuesto")?;
    }
    if report.total_bytes != total || report.observed_bytes != observed {
        return Err("Los totales del informe de espacio no coinciden con sus elementos".into());
    }
    if !report.complete && report.reason.as_deref().is_none_or(str::is_empty) {
        return Err("Un recorrido parcial requiere motivo explícito".into());
    }
    if report.complete
        && (report.omitted_children != Some(0)
            || report.reason.is_some()
            || report
                .nodes
                .iter()
                .any(|n| n.status != NodeStatus::Complete || n.bytes.is_none()))
    {
        return Err("El informe declara completo un recorrido parcial".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Work {
    start: usize,
    end: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}
/// Weighted binary treemap: split near half the bytes along the longer axis.
/// Areas cover the viewport proportionally to completed positive measurements,
/// with stable descending-byte/index tie ordering and original node indices.
/// Zeros/unknowns/Partial/Error are omitted rather than assigned invented sizes.
/// Metadata budgets and checked sums are validated before allocating layout state.
/// Extremely tiny dimensions/ratios unrepresentable in f64 fail explicitly.
pub fn layout(nodes: &[SpaceNode], width: f64, height: f64) -> Result<Vec<Area>, String> {
    layout_where(nodes, width, height, |_| true)
}
/// Filter direct children without cloning their raw names or altering the report.
/// Areas normalize ONLY the included completed positive children.
pub fn layout_where(
    nodes: &[SpaceNode],
    width: f64,
    height: f64,
    include: impl Fn(&SpaceNode) -> bool,
) -> Result<Vec<Area>, String> {
    if !width.is_finite()
        || !height.is_finite()
        || !(0.0..=16384.0).contains(&width)
        || !(0.0..=16384.0).contains(&height)
    {
        return Err("Dimensiones del mapa no válidas".into());
    }
    preflight(nodes)?;
    if width == 0.0 || height == 0.0 {
        return Ok(Vec::new());
    }
    if width * height == 0.0 {
        return Err("Dimensiones del mapa demasiado pequeñas para representarlas".into());
    }
    let mut items = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| include(node))
        .filter_map(|(index, node)| match (node.status, node.bytes) {
            (NodeStatus::Complete, Some(bytes)) if bytes > 0 => Some((index, bytes)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let total = items.iter().map(|(_, b)| *b).sum::<u64>();
    if total == 0 {
        return Ok(Vec::new());
    }
    items.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut prefix = Vec::with_capacity(items.len() + 1);
    prefix.push(0u64);
    for (_, bytes) in &items {
        let sum = prefix
            .last()
            .copied()
            .unwrap_or(0)
            .checked_add(*bytes)
            .ok_or("El tamaño del mapa excede u64")?;
        prefix.push(sum);
    }
    let mut stack = vec![Work {
        start: 0,
        end: items.len(),
        x0: 0.0,
        y0: 0.0,
        x1: width,
        y1: height,
    }];
    let mut result = Vec::with_capacity(items.len());
    while let Some(work) = stack.pop() {
        if work.end - work.start == 1 {
            result.push(Area {
                node: items[work.start].0,
                x: work.x0,
                y: work.y0,
                w: work.x1 - work.x0,
                h: work.y1 - work.y0,
            });
            continue;
        }
        let sum = prefix[work.end] - prefix[work.start];
        let weight = |end: usize| prefix[end] - prefix[work.start];
        let balance = |end: usize| {
            let left = weight(end);
            left.abs_diff(sum - left)
        };
        let (mut low, mut high) = (work.start + 1, work.end - 1);
        while low < high {
            let middle = low + (high - low) / 2;
            if weight(middle) < sum / 2 {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let split = if low > work.start + 1 && balance(low - 1) <= balance(low) {
            low - 1
        } else {
            low
        };
        let left_weight = weight(split);
        let right_weight = sum - left_weight;
        let left_is_small = left_weight <= right_weight;
        let smaller = left_weight.min(right_weight);
        let (axis_start, axis_end, horizontal) = if work.x1 - work.x0 >= work.y1 - work.y0 {
            (work.x0, work.x1, true)
        } else {
            (work.y0, work.y1, false)
        };
        // Compute the SMALLER fraction directly to avoid 1 - epsilon rounding
        // to one when bytes differ by nearly u64::MAX. Place it at the low edge.
        let cut = axis_start + (axis_end - axis_start) * (smaller as f64 / sum as f64);
        if !cut.is_finite() || cut <= axis_start || cut >= axis_end {
            return Err("La relación entre tamaños excede la precisión del mapa".into());
        }
        let mut left = Work { end: split, ..work };
        let mut right = Work {
            start: split,
            ..work
        };
        if horizontal {
            if left_is_small {
                left.x1 = cut;
                right.x0 = cut;
            } else {
                right.x1 = cut;
                left.x0 = cut;
            }
        } else if left_is_small {
            left.y1 = cut;
            right.y0 = cut;
        } else {
            right.y1 = cut;
            left.y0 = cut;
        }
        stack.push(right);
        stack.push(left);
    }
    result.sort_unstable_by(|a, b| {
        nodes[b.node]
            .bytes
            .cmp(&nodes[a.node].bytes)
            .then_with(|| a.node.cmp(&b.node))
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(name: &[u8], bytes: u64) -> SpaceNode {
        SpaceNode {
            name: name.into(),
            kind: NodeKind::File,
            bytes: Some(bytes),
            observed_bytes: bytes,
            files: 1,
            directories: 0,
            status: NodeStatus::Complete,
            error: None,
        }
    }
    fn verify(nodes: &[SpaceNode], width: f64, height: f64) {
        let areas = layout(nodes, width, height).unwrap();
        let total = nodes.iter().filter_map(|node| node.bytes).sum::<u64>();
        let mut covered = 0.0;
        for (i, area) in areas.iter().enumerate() {
            assert!(
                area.x.is_finite()
                    && area.y.is_finite()
                    && area.w.is_finite()
                    && area.h.is_finite()
            );
            assert!(area.x >= 0.0 && area.y >= 0.0 && area.w > 0.0 && area.h > 0.0);
            assert!(area.x + area.w <= width + 1e-9 && area.y + area.h <= height + 1e-9);
            let expected = width * height * (nodes[area.node].bytes.unwrap() as f64 / total as f64);
            assert!((area.w * area.h - expected).abs() <= expected.max(1.0) * 1e-9);
            covered += area.w * area.h;
            for other in &areas[..i] {
                assert!(
                    area.x + area.w <= other.x + 1e-10
                        || other.x + other.w <= area.x + 1e-10
                        || area.y + area.h <= other.y + 1e-10
                        || other.y + other.h <= area.y + 1e-10
                );
            }
        }
        assert!((covered - width * height).abs() <= width * height * 1e-9);
    }
    #[test]
    fn measured_weights_cover_without_overlaps_and_keep_original_indices() {
        let nodes = vec![
            node(b"small", 1),
            node(b"large", 10),
            node(b"middle", 3),
            node(b"tie", 3),
        ];
        verify(&nodes, 1000.0, 650.0);
        let areas = layout(&nodes, 1000.0, 650.0).unwrap();
        assert_eq!(
            areas.iter().map(|a| a.node).collect::<Vec<_>>(),
            vec![1, 2, 3, 0]
        );
        assert_eq!(areas, layout(&nodes, 1000.0, 650.0).unwrap());
    }
    #[test]
    fn zero_unknown_and_incomplete_nodes_never_gain_an_area() {
        let mut partial = node(b"partial", 100);
        partial.bytes = None;
        partial.status = NodeStatus::Partial;
        let mut error = node(b"error", 50);
        error.bytes = None;
        error.status = NodeStatus::Error;
        error.error = Some("offline".into());
        let mut unknown = node(b"unknown", 0);
        unknown.bytes = None;
        let nodes = vec![
            node(b"zero", 0),
            partial,
            error,
            unknown,
            node(b"positive", 7),
        ];
        let areas = layout(&nodes, 120.0, 80.0).unwrap();
        assert_eq!(
            areas,
            vec![Area {
                node: 4,
                x: 0.0,
                y: 0.0,
                w: 120.0,
                h: 80.0
            }]
        );
        assert!(layout(&nodes[..4], 120.0, 80.0).unwrap().is_empty());
        assert!(layout(&nodes, 0.0, 80.0).unwrap().is_empty());
        let mut invalid = nodes[1].clone();
        invalid.bytes = Some(100);
        assert!(layout(&[invalid], 120.0, 80.0).is_err());
    }
    #[test]
    fn checked_sums_and_extreme_u64_weights_do_not_wrap() {
        assert!(layout(&[node(b"a", u64::MAX), node(b"b", 1)], 1000.0, 650.0).is_err());
        verify(&[node(b"a", u64::MAX - 1), node(b"b", 1)], 1000.0, 650.0);
        let mut partial = node(b"partial", u64::MAX);
        partial.bytes = None;
        partial.status = NodeStatus::Partial;
        assert!(layout(&[partial, node(b"b", 1)], 1000.0, 650.0).is_err());
        let mut inconsistent = node(b"a", 3);
        inconsistent.observed_bytes = 2;
        assert!(layout(&[inconsistent], 1000.0, 650.0).is_err());
    }
    #[test]
    fn metadata_caps_are_validated_even_without_positive_bytes() {
        assert!(layout(&vec![node(b"a", 0); MAX_NODES + 1], 0.0, 0.0).is_err());
        assert!(layout(&[node(&vec![b'x'; MAX_NAME_BYTES + 1], 0)], 100.0, 100.0).is_err());
        let names = vec![node(&vec![b'x'; MAX_NAME_BYTES], 0); MAX_NAMES_BYTES / MAX_NAME_BYTES];
        assert!(layout(&names, 100.0, 100.0).unwrap().is_empty());
        let mut excess = names;
        excess.push(node(b"x", 0));
        assert!(layout(&excess, 100.0, 100.0).is_err());
        let mut diagnostic = node(b"a", 0);
        diagnostic.error = Some("x".repeat(MAX_ERROR_BYTES));
        let errors = vec![diagnostic.clone(); MAX_ERRORS_BYTES / MAX_ERROR_BYTES];
        assert!(layout(&errors, 100.0, 100.0).is_ok());
        let mut excess = errors;
        excess.push(diagnostic.clone());
        assert!(layout(&excess, 100.0, 100.0).is_err());
        diagnostic.error.as_mut().unwrap().push('x');
        assert!(layout(&[diagnostic], 100.0, 100.0).is_err());
        assert!(layout(&[node(&[b'x', 0xff], 1)], 100.0, 100.0).is_ok());
        for name in [&b""[..], b".", b"..", b"a/b", b"a\0b"] {
            assert!(layout(&[node(name, 0)], 100.0, 100.0).is_err());
        }
    }
    #[test]
    fn resize_reflows_the_same_measured_nodes_and_dimensions_are_finite_bounded() {
        let nodes = (1..=40)
            .map(|n| node(format!("node{n}").as_bytes(), n))
            .collect::<Vec<_>>();
        for (w, h) in [
            (1000.0, 650.0),
            (650.0, 1000.0),
            (320.0, 180.0),
            (16384.0, 16384.0),
        ] {
            verify(&nodes, w, h);
        }
        for (w, h) in [
            (f64::NAN, 10.0),
            (10.0, f64::INFINITY),
            (-1.0, 10.0),
            (16385.0, 10.0),
        ] {
            assert!(layout(&nodes, w, h).is_err());
        }
    }
    #[test]
    fn report_totals_uniqueness_and_partial_flags_fail_closed() {
        let mut report = SpaceReport {
            nodes: vec![node(b"a", 8), node(b"b", 4)],
            total_bytes: 12,
            observed_bytes: 12,
            visited: 2,
            complete: true,
            reason: None,
            omitted_children: Some(0),
        };
        assert!(validate_report(&report).is_ok());
        report.total_bytes = 13;
        assert!(validate_report(&report).is_err());
        report.total_bytes = 12;
        report.nodes[1].name = b"a".to_vec();
        assert!(validate_report(&report).is_err());
        report.nodes[1].name = b"b".to_vec();
        report.nodes[1].bytes = None;
        report.nodes[1].status = NodeStatus::Partial;
        report.total_bytes = 8;
        assert!(validate_report(&report).is_err());
        report.complete = false;
        report.reason = Some("Recorrido parcial".into());
        assert!(validate_report(&report).is_ok());
        report.observed_bytes = 13;
        assert!(validate_report(&report).is_err());
        report.observed_bytes = 12;
        report.reason = Some("x".repeat(MAX_ERROR_BYTES + 1));
        assert!(validate_report(&report).is_err());
    }
    #[test]
    fn filtered_layout_renormalizes_visible_bytes_without_changing_measured_nodes() {
        let nodes = vec![
            node(b"visible", 8),
            node(b"other", 4),
            node(b".hidden", 12),
            node(b".\\xff", 0),
        ];
        let areas = layout_where(&nodes, 120., 80., |n| n.name[0] != b'.').unwrap();
        assert_eq!(areas.len(), 2);
        for a in areas {
            assert!(
                (a.w * a.h - 120. * 80. * nodes[a.node].bytes.unwrap() as f64 / 12.).abs() < 1e-8
            );
        }
        assert_eq!(nodes.iter().map(|n| n.bytes.unwrap()).sum::<u64>(), 24);
        assert_eq!(layout(&nodes, 120., 80.).unwrap().len(), 3);
    }
}
